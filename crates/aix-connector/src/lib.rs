//! Bounded external API transport and connector import.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::time::Duration;

use aix_core::{Capability, Connector, ConnectorKind, ConnectorOperation, HttpMethod};
use aix_permission::{PermissionCheck, PermissionResolver, PermissionTarget};
use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderName, HeaderValue, CONTENT_TYPE, LOCATION};
use serde_json::Value;
use url::Url;

/// Maximum local OpenAPI JSON source accepted by the bounded importer.
pub const MAX_OPENAPI_DOCUMENT_BYTES: usize = 1_048_576;

/// Maximum operations emitted from one OpenAPI document.
pub const MAX_IMPORTED_OPERATIONS: usize = 256;

/// Request limits enforced independently of App Definition data.
#[derive(Clone, Debug)]
pub struct HttpLimits {
    pub timeout: Duration,
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
    pub max_redirects: usize,
    pub max_url_bytes: usize,
    pub max_headers: usize,
    pub max_header_bytes: usize,
}

impl Default for HttpLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
            max_request_bytes: 256 * 1024,
            max_response_bytes: 1024 * 1024,
            max_redirects: 5,
            max_url_bytes: 8 * 1024,
            max_headers: 64,
            max_header_bytes: 16 * 1024,
        }
    }
}

/// Provider-neutral HTTP request accepted by a host transport.
#[derive(Clone, Debug, PartialEq)]
pub struct HttpRequest {
    pub operation_id: String,
    pub url: String,
    pub method: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<Value>,
}

/// Bounded HTTP response returned to an AIX Operation.
#[derive(Clone, Debug, PartialEq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Value,
}

/// Structured transport failure safe to map into an Operation error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectorError {
    pub code: String,
    pub message: String,
}

impl ConnectorError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ConnectorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ConnectorError {}

/// Injectable HTTP boundary used by the Runtime and deterministic tests.
pub trait HttpTransport: Send + Sync {
    fn execute(
        &self,
        request: &HttpRequest,
        permissions: &dyn PermissionResolver,
    ) -> Result<HttpResponse, ConnectorError>;
}

/// Reqwest transport with redirects disabled at the library layer.
///
/// Redirects are followed manually so every destination crosses the same
/// capability resolver before a new request is sent.
#[derive(Clone, Debug)]
pub struct ReqwestHttpTransport {
    limits: HttpLimits,
}

impl ReqwestHttpTransport {
    pub fn new(limits: HttpLimits) -> Self {
        Self { limits }
    }
}

impl Default for ReqwestHttpTransport {
    fn default() -> Self {
        Self::new(HttpLimits::default())
    }
}

impl HttpTransport for ReqwestHttpTransport {
    fn execute(
        &self,
        request: &HttpRequest,
        permissions: &dyn PermissionResolver,
    ) -> Result<HttpResponse, ConnectorError> {
        execute_request(request, permissions, &self.limits)
    }
}

fn execute_request(
    request: &HttpRequest,
    permissions: &dyn PermissionResolver,
    limits: &HttpLimits,
) -> Result<HttpResponse, ConnectorError> {
    validate_limits(limits)?;
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(limits.timeout)
        .build()
        .map_err(|error| ConnectorError::new("transport_error", error.to_string()))?;
    let mut url = parse_http_url(&request.url)?;
    check_url_length(&url, limits)?;
    let mut method = reqwest::Method::from_bytes(request.method.as_bytes())
        .map_err(|_| ConnectorError::new("invalid_request", "HTTP method is invalid"))?;
    let mut headers = checked_headers(&request.headers, limits)?;
    let mut body = encode_body(request.body.as_ref(), limits)?;

    for redirect_count in 0..=limits.max_redirects {
        authorize_url(permissions, &request.operation_id, &url)?;
        let mut builder = client.request(method.clone(), url.clone());
        for (name, value) in &headers {
            builder = builder.header(name, value);
        }
        if let Some(bytes) = &body {
            if !headers.iter().any(|(name, _)| name == CONTENT_TYPE) {
                builder = builder.header(CONTENT_TYPE, "application/json");
            }
            builder = builder.body(bytes.clone());
        }
        let response = builder.send().map_err(map_reqwest_error)?;
        if !response.status().is_redirection() {
            return decode_response(response, limits);
        }
        let Some(location) = response.headers().get(LOCATION) else {
            return decode_response(response, limits);
        };
        if redirect_count == limits.max_redirects {
            return Err(ConnectorError::new(
                "redirect_limit",
                "HTTP redirect limit was reached",
            ));
        }
        let location = location.to_str().map_err(|_| {
            ConnectorError::new("invalid_redirect", "redirect location is not UTF-8")
        })?;
        let next = url
            .join(location)
            .map_err(|error| ConnectorError::new("invalid_redirect", error.to_string()))?;
        check_url_length(&next, limits)?;
        if !matches!(next.scheme(), "http" | "https") {
            return Err(ConnectorError::new(
                "invalid_redirect",
                "redirect destination must use HTTP or HTTPS",
            ));
        }
        if origin(&url) != origin(&next) {
            headers.retain(|(name, _)| !is_sensitive_header(name));
        }
        match response.status().as_u16() {
            303 => {
                method = reqwest::Method::GET;
                body = None;
            }
            301 | 302 if method != reqwest::Method::GET && method != reqwest::Method::HEAD => {
                method = reqwest::Method::GET;
                body = None;
            }
            _ => {}
        }
        url = next;
    }
    unreachable!("redirect loop returns at its configured bound")
}

fn validate_limits(limits: &HttpLimits) -> Result<(), ConnectorError> {
    if limits.timeout.is_zero()
        || limits.max_request_bytes == 0
        || limits.max_response_bytes == 0
        || limits.max_url_bytes == 0
        || limits.max_headers == 0
        || limits.max_header_bytes == 0
    {
        return Err(ConnectorError::new(
            "invalid_limits",
            "HTTP limits must be greater than zero",
        ));
    }
    Ok(())
}

fn check_url_length(url: &Url, limits: &HttpLimits) -> Result<(), ConnectorError> {
    if url.as_str().len() > limits.max_url_bytes {
        Err(ConnectorError::new(
            "request_too_large",
            "request URL exceeds the configured limit",
        ))
    } else {
        Ok(())
    }
}

fn parse_http_url(value: &str) -> Result<Url, ConnectorError> {
    let url =
        Url::parse(value).map_err(|error| ConnectorError::new("invalid_url", error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(ConnectorError::new(
            "invalid_url",
            "URL must use HTTP(S) without credentials or a fragment",
        ));
    }
    Ok(url)
}

fn authorize_url(
    permissions: &dyn PermissionResolver,
    operation_id: &str,
    url: &Url,
) -> Result<(), ConnectorError> {
    permissions
        .authorize(&PermissionCheck {
            operation_id: operation_id.to_owned(),
            capability: Capability::NetworkRequest,
            target: PermissionTarget::NetworkUrl(url.as_str().to_owned()),
        })
        .map_err(|error| ConnectorError::new(error.code, error.message))
}

fn checked_headers(
    values: &BTreeMap<String, String>,
    limits: &HttpLimits,
) -> Result<Vec<(HeaderName, HeaderValue)>, ConnectorError> {
    if values.len() > limits.max_headers {
        return Err(ConnectorError::new(
            "request_too_large",
            "request contains too many headers",
        ));
    }
    let bytes = values
        .iter()
        .map(|(key, value)| key.len() + value.len())
        .sum::<usize>();
    if bytes > limits.max_header_bytes {
        return Err(ConnectorError::new(
            "request_too_large",
            "request headers exceed the configured limit",
        ));
    }
    values
        .iter()
        .map(|(name, value)| {
            let name = HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| ConnectorError::new("invalid_header", "invalid HTTP header name"))?;
            if is_forbidden_header(&name) {
                return Err(ConnectorError::new(
                    "invalid_header",
                    format!("header `{name}` is controlled by the HTTP adapter"),
                ));
            }
            let value = HeaderValue::from_str(value)
                .map_err(|_| ConnectorError::new("invalid_header", "invalid HTTP header value"))?;
            Ok((name, value))
        })
        .collect()
}

fn is_forbidden_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "host" | "content-length" | "connection" | "transfer-encoding" | "proxy-authorization"
    )
}

fn is_sensitive_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "authorization" | "cookie" | "proxy-authorization"
    )
}

fn encode_body(
    body: Option<&Value>,
    limits: &HttpLimits,
) -> Result<Option<Vec<u8>>, ConnectorError> {
    let Some(body) = body else { return Ok(None) };
    let bytes = serde_json::to_vec(body)
        .map_err(|error| ConnectorError::new("invalid_request", error.to_string()))?;
    if bytes.len() > limits.max_request_bytes {
        return Err(ConnectorError::new(
            "request_too_large",
            "request body exceeds the configured limit",
        ));
    }
    Ok(Some(bytes))
}

fn decode_response(
    response: Response,
    limits: &HttpLimits,
) -> Result<HttpResponse, ConnectorError> {
    if response
        .content_length()
        .is_some_and(|length| length > limits.max_response_bytes as u64)
    {
        return Err(ConnectorError::new(
            "response_too_large",
            "response body exceeds the configured limit",
        ));
    }
    let response_header_bytes = response
        .headers()
        .iter()
        .map(|(name, value)| name.as_str().len() + value.as_bytes().len())
        .sum::<usize>();
    if response.headers().len() > limits.max_headers
        || response_header_bytes > limits.max_header_bytes
    {
        return Err(ConnectorError::new(
            "response_too_large",
            "response headers exceed the configured limit",
        ));
    }
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let headers = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.to_string(), value.to_owned()))
        })
        .collect();
    let mut bytes = Vec::new();
    response
        .take(limits.max_response_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::TimedOut {
                ConnectorError::new("timeout", "HTTP response body timed out")
            } else {
                ConnectorError::new("transport_error", error.to_string())
            }
        })?;
    if bytes.len() > limits.max_response_bytes {
        return Err(ConnectorError::new(
            "response_too_large",
            "response body exceeds the configured limit",
        ));
    }
    let body = if bytes.is_empty() {
        Value::Null
    } else if content_type.contains("application/json") || content_type.contains("+json") {
        serde_json::from_slice(&bytes).map_err(|error| {
            ConnectorError::new(
                "invalid_response",
                format!("invalid JSON response: {error}"),
            )
        })?
    } else {
        Value::String(String::from_utf8(bytes).map_err(|_| {
            ConnectorError::new("invalid_response", "non-JSON response is not valid UTF-8")
        })?)
    };
    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

fn map_reqwest_error(error: reqwest::Error) -> ConnectorError {
    if error.is_timeout() {
        ConnectorError::new("timeout", "HTTP request timed out")
    } else {
        ConnectorError::new("transport_error", error.to_string())
    }
}

fn origin(url: &Url) -> (String, Option<String>, Option<u16>) {
    (
        url.scheme().to_owned(),
        url.host_str().map(str::to_owned),
        url.port_or_known_default(),
    )
}

/// Convert a declarative connector operation invocation into a concrete request.
pub fn build_connector_request(
    connector: &Connector,
    operation: &ConnectorOperation,
    input: &Value,
) -> Result<HttpRequest, ConnectorError> {
    validate_connector(connector)?;
    let object = input.as_object().ok_or_else(|| {
        ConnectorError::new("invalid_request", "connector input must be an object")
    })?;
    let path_params = object.get("pathParams").and_then(Value::as_object);
    let path = expand_path(&operation.path, path_params)?;
    let joined = format!(
        "{}/{}",
        connector.base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    );
    let mut url = parse_http_url(&joined)?;
    if let Some(query) = object.get("query").and_then(Value::as_object) {
        let mut pairs = url.query_pairs_mut();
        for (name, value) in query {
            match value {
                Value::Array(values) => {
                    for value in values {
                        pairs.append_pair(name, &scalar_parameter(value)?);
                    }
                }
                value => {
                    pairs.append_pair(name, &scalar_parameter(value)?);
                }
            }
        }
    }
    let headers = object
        .get("headers")
        .and_then(Value::as_object)
        .map(|headers| {
            headers
                .iter()
                .map(|(name, value)| {
                    value
                        .as_str()
                        .map(|value| (name.clone(), value.to_owned()))
                        .ok_or_else(|| {
                            ConnectorError::new(
                                "invalid_request",
                                "connector header values must be strings",
                            )
                        })
                })
                .collect::<Result<BTreeMap<_, _>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    Ok(HttpRequest {
        operation_id: operation.id.clone(),
        url: url.to_string(),
        method: method_name(&operation.method).to_owned(),
        headers,
        body: object.get("body").cloned(),
    })
}

/// Validate transport-specific connector invariants before registration.
pub fn validate_connector(connector: &Connector) -> Result<(), ConnectorError> {
    let base = parse_http_url(&connector.base_url)?;
    if base.query().is_some() {
        return Err(ConnectorError::new(
            "invalid_base_url",
            "connector base URL cannot contain a query",
        ));
    }
    for operation in &connector.operations {
        validate_path_template(&operation.path)?;
    }
    Ok(())
}

fn validate_path_template(template: &str) -> Result<(), ConnectorError> {
    if !template.starts_with('/') {
        return Err(ConnectorError::new(
            "invalid_path",
            "connector path must start with `/`",
        ));
    }
    let mut parameter = false;
    let mut name_length = 0_usize;
    for character in template.chars() {
        match character {
            '{' if !parameter => {
                parameter = true;
                name_length = 0;
            }
            '}' if parameter && name_length > 0 => parameter = false,
            '{' | '}' if parameter => {
                return Err(ConnectorError::new(
                    "invalid_path",
                    "connector path contains a malformed parameter",
                ));
            }
            '}' => {
                return Err(ConnectorError::new(
                    "invalid_path",
                    "connector path contains an unmatched `}`",
                ));
            }
            value
                if parameter
                    && (value.is_ascii_alphanumeric() || matches!(value, '_' | '-' | '.')) =>
            {
                name_length += 1;
            }
            _ if parameter => {
                return Err(ConnectorError::new(
                    "invalid_path",
                    "connector path parameter name contains unsupported characters",
                ));
            }
            _ => {}
        }
    }
    if parameter {
        return Err(ConnectorError::new(
            "invalid_path",
            "connector path contains an unclosed parameter",
        ));
    }
    Ok(())
}

fn expand_path(
    template: &str,
    parameters: Option<&serde_json::Map<String, Value>>,
) -> Result<String, ConnectorError> {
    let mut output = String::new();
    let mut rest = template;
    let mut used = BTreeSet::new();
    while let Some(start) = rest.find('{') {
        output.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after
            .find('}')
            .ok_or_else(|| ConnectorError::new("invalid_path", "unclosed path parameter"))?;
        let name = &after[..end];
        if name.is_empty() {
            return Err(ConnectorError::new(
                "invalid_path",
                "path parameter name cannot be empty",
            ));
        }
        let value = parameters
            .and_then(|values| values.get(name))
            .ok_or_else(|| {
                ConnectorError::new(
                    "missing_parameter",
                    format!("path parameter `{name}` is required"),
                )
            })?;
        used.insert(name.to_owned());
        output.push_str(&encode_path_segment(&scalar_parameter(value)?));
        rest = &after[end + 1..];
    }
    output.push_str(rest);
    if parameters.is_some_and(|values| values.keys().any(|name| !used.contains(name))) {
        return Err(ConnectorError::new(
            "invalid_request",
            "an unknown path parameter was supplied",
        ));
    }
    Ok(output)
}

fn scalar_parameter(value: &Value) -> Result<String, ConnectorError> {
    match value {
        Value::String(value) => Ok(value.clone()),
        Value::Number(value) => Ok(value.to_string()),
        Value::Bool(value) => Ok(value.to_string()),
        _ => Err(ConnectorError::new(
            "invalid_request",
            "URL parameters must be strings, numbers, or booleans",
        )),
    }
}

fn encode_path_segment(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            write!(encoded, "%{byte:02X}").expect("writing to String cannot fail");
        }
    }
    encoded
}

fn method_name(method: &HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "GET",
        HttpMethod::Post => "POST",
        HttpMethod::Put => "PUT",
        HttpMethod::Patch => "PATCH",
        HttpMethod::Delete => "DELETE",
    }
}

/// Import the intentionally small AIX OpenAPI 3 subset.
pub fn import_openapi(document: &Value, connector_id: &str) -> Result<Connector, ConnectorError> {
    if !is_connector_id(connector_id) {
        return Err(ConnectorError::new(
            "invalid_connector_id",
            "connector id must start with a letter and contain only letters, digits, `_`, `-`, or `.`",
        ));
    }
    let root = document.as_object().ok_or_else(|| {
        ConnectorError::new("invalid_openapi", "OpenAPI document must be an object")
    })?;
    let version = root
        .get("openapi")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !(version.starts_with("3.0.") || version.starts_with("3.1.")) {
        return Err(ConnectorError::new(
            "unsupported_openapi",
            "only OpenAPI 3.0 and 3.1 are supported",
        ));
    }
    if contains_reference(document) {
        return Err(ConnectorError::new(
            "unsupported_reference",
            "OpenAPI $ref is not supported in the Beta importer",
        ));
    }
    if root
        .get("security")
        .and_then(Value::as_array)
        .is_some_and(|security| !security.is_empty())
    {
        return Err(ConnectorError::new(
            "unsupported_openapi",
            "top-level security requirements are not supported",
        ));
    }
    let servers = root
        .get("servers")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ConnectorError::new("invalid_openapi", "one top-level server is required")
        })?;
    if servers.len() != 1 {
        return Err(ConnectorError::new(
            "unsupported_openapi",
            "exactly one top-level server is supported",
        ));
    }
    let base_url = servers[0]
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| ConnectorError::new("invalid_openapi", "server URL is required"))?;
    if base_url.contains('{') {
        return Err(ConnectorError::new(
            "unsupported_openapi",
            "server variables are not supported",
        ));
    }
    parse_http_url(base_url)?;
    let paths = root
        .get("paths")
        .and_then(Value::as_object)
        .ok_or_else(|| ConnectorError::new("invalid_openapi", "paths object is required"))?;
    let mut operations = Vec::new();
    let mut ids = BTreeSet::new();
    for (path, item) in paths {
        if !path.starts_with('/') {
            return Err(ConnectorError::new(
                "invalid_openapi",
                "OpenAPI paths must start with `/`",
            ));
        }
        let item = item
            .as_object()
            .ok_or_else(|| ConnectorError::new("invalid_openapi", "path item must be an object"))?;
        for (method, value) in item {
            let Some(http_method) = parse_method(method) else {
                continue;
            };
            let operation = value.as_object().ok_or_else(|| {
                ConnectorError::new("invalid_openapi", "operation must be an object")
            })?;
            let has_security = operation.get("security").is_some_and(
                |security| !matches!(security, Value::Array(values) if values.is_empty()),
            );
            if operation.contains_key("callbacks")
                || operation.contains_key("servers")
                || has_security
            {
                return Err(ConnectorError::new(
                    "unsupported_openapi",
                    "operation callbacks, servers, and security are not supported",
                ));
            }
            let id = operation
                .get("operationId")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    ConnectorError::new(
                        "invalid_openapi",
                        "every imported operation requires operationId",
                    )
                })?;
            if !is_operation_id(id) || !ids.insert(id.to_owned()) {
                return Err(ConnectorError::new(
                    "invalid_operation_id",
                    format!("operationId `{id}` must be unique and use namespace.name syntax"),
                ));
            }
            operations.push(ConnectorOperation {
                id: id.to_owned(),
                method: http_method,
                path: path.clone(),
            });
            if operations.len() > MAX_IMPORTED_OPERATIONS {
                return Err(ConnectorError::new(
                    "openapi_too_large",
                    "OpenAPI document contains too many importable operations",
                ));
            }
        }
    }
    if operations.is_empty() {
        return Err(ConnectorError::new(
            "invalid_openapi",
            "no supported HTTP operations were found",
        ));
    }
    let connector = Connector {
        id: connector_id.to_owned(),
        kind: ConnectorKind::Http,
        base_url: base_url.to_owned(),
        operations,
    };
    validate_connector(&connector)?;
    Ok(connector)
}

/// Parse bounded local OpenAPI JSON and import its supported operations.
pub fn import_openapi_json(source: &str, connector_id: &str) -> Result<Connector, ConnectorError> {
    if source.len() > MAX_OPENAPI_DOCUMENT_BYTES {
        return Err(ConnectorError::new(
            "openapi_too_large",
            "OpenAPI document exceeds 1 MiB",
        ));
    }
    let document = serde_json::from_str(source)
        .map_err(|error| ConnectorError::new("invalid_openapi", error.to_string()))?;
    import_openapi(&document, connector_id)
}

fn contains_reference(value: &Value) -> bool {
    match value {
        Value::Object(values) => {
            values.contains_key("$ref") || values.values().any(contains_reference)
        }
        Value::Array(values) => values.iter().any(contains_reference),
        _ => false,
    }
}

fn parse_method(value: &str) -> Option<HttpMethod> {
    match value {
        "get" => Some(HttpMethod::Get),
        "post" => Some(HttpMethod::Post),
        "put" => Some(HttpMethod::Put),
        "patch" => Some(HttpMethod::Patch),
        "delete" => Some(HttpMethod::Delete),
        _ => None,
    }
}

fn is_operation_id(id: &str) -> bool {
    let segments = id.split('.').collect::<Vec<_>>();
    segments.len() == 2
        && segments.iter().all(|segment| {
            !segment.is_empty()
                && segment.starts_with(|character: char| character.is_ascii_lowercase())
                && segment.chars().all(|character| {
                    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
                })
        })
}

fn is_connector_id(id: &str) -> bool {
    id.chars()
        .next()
        .is_some_and(|value| value.is_ascii_alphabetic())
        && id
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, '_' | '-' | '.'))
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use aix_core::PermissionRequest;
    use aix_permission::{CapabilityResolver, HostGrant};
    use serde_json::json;

    use super::*;

    fn fixture(response: String) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request).unwrap();
            stream.write_all(response.as_bytes()).unwrap();
        });
        (format!("http://{address}"), handle)
    }

    fn resolver(scope: &str) -> CapabilityResolver {
        resolver_scopes(&[scope])
    }

    fn resolver_scopes(scopes: &[&str]) -> CapabilityResolver {
        let requests = [PermissionRequest {
            capability: Capability::NetworkRequest,
            scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
        }];
        let grants = [HostGrant::new(
            "test",
            Capability::NetworkRequest,
            scopes.iter().map(|scope| (*scope).to_owned()).collect(),
        )
        .unwrap()];
        CapabilityResolver::new("test", &requests, &grants).unwrap()
    }

    #[test]
    fn sends_bounded_request_and_decodes_json() {
        let body = r#"{"temperature":21}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let (base, server) = fixture(response);
        let result = ReqwestHttpTransport::default()
            .execute(
                &HttpRequest {
                    operation_id: "test.request".to_owned(),
                    url: format!("{base}/weather"),
                    method: "GET".to_owned(),
                    headers: BTreeMap::new(),
                    body: None,
                },
                &resolver(&base),
            )
            .unwrap();
        assert_eq!(result.status, 200);
        assert_eq!(result.body["temperature"], 21);
        server.join().unwrap();
    }

    #[test]
    fn redirect_destination_is_reauthorized() {
        let response = "HTTP/1.1 302 Found\r\nLocation: /outside\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned();
        let (base, server) = fixture(response);
        let transport = ReqwestHttpTransport::default();
        let error = transport
            .execute(
                &HttpRequest {
                    operation_id: "test.request".to_owned(),
                    url: format!("{base}/allowed"),
                    method: "GET".to_owned(),
                    headers: BTreeMap::new(),
                    body: None,
                },
                &resolver(&format!("{base}/allowed")),
            )
            .unwrap_err();
        assert_eq!(error.code, "permission_denied");
        server.join().unwrap();
    }

    #[test]
    fn cross_origin_redirect_removes_sensitive_headers() {
        let destination = TcpListener::bind("127.0.0.1:0").unwrap();
        let destination_address = destination.local_addr().unwrap();
        let destination_server = thread::spawn(move || {
            let (mut stream, _) = destination.accept().unwrap();
            let mut request = [0_u8; 4096];
            let size = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..size]).to_ascii_lowercase();
            assert!(!request.contains("authorization:"));
            assert!(!request.contains("cookie:"));
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .unwrap();
        });
        let redirect = format!(
            "HTTP/1.1 302 Found\r\nLocation: http://{destination_address}/final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        let (source, source_server) = fixture(redirect);
        let destination_scope = format!("http://{destination_address}");
        let mut headers = BTreeMap::new();
        headers.insert("authorization".to_owned(), "Bearer secret".to_owned());
        headers.insert("cookie".to_owned(), "session=secret".to_owned());
        let response = ReqwestHttpTransport::default()
            .execute(
                &HttpRequest {
                    operation_id: "test.request".to_owned(),
                    url: format!("{source}/start"),
                    method: "GET".to_owned(),
                    headers,
                    body: None,
                },
                &resolver_scopes(&[&source, &destination_scope]),
            )
            .unwrap();
        assert_eq!(response.status, 200);
        source_server.join().unwrap();
        destination_server.join().unwrap();
    }

    #[test]
    fn rejects_response_larger_than_limit() {
        let body = "123456789";
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let (base, server) = fixture(response);
        let transport = ReqwestHttpTransport::new(HttpLimits {
            max_response_bytes: 8,
            ..HttpLimits::default()
        });
        let error = transport
            .execute(
                &HttpRequest {
                    operation_id: "test.request".to_owned(),
                    url: base.clone(),
                    method: "GET".to_owned(),
                    headers: BTreeMap::new(),
                    body: None,
                },
                &resolver(&base),
            )
            .unwrap_err();
        assert_eq!(error.code, "response_too_large");
        server.join().unwrap();
    }

    #[test]
    fn reports_request_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request).unwrap();
            thread::sleep(Duration::from_millis(150));
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
        });
        let base = format!("http://{address}");
        let transport = ReqwestHttpTransport::new(HttpLimits {
            timeout: Duration::from_millis(25),
            ..HttpLimits::default()
        });
        let error = transport
            .execute(
                &HttpRequest {
                    operation_id: "test.request".to_owned(),
                    url: base.clone(),
                    method: "GET".to_owned(),
                    headers: BTreeMap::new(),
                    body: None,
                },
                &resolver(&base),
            )
            .unwrap_err();
        assert_eq!(error.code, "timeout");
        server.join().unwrap();
    }

    #[test]
    fn connector_mapping_expands_path_and_query_as_data() {
        let connector = Connector {
            id: "weather".to_owned(),
            kind: ConnectorKind::Http,
            base_url: "https://api.example.com/v1".to_owned(),
            operations: vec![],
        };
        let operation = ConnectorOperation {
            id: "weather.get".to_owned(),
            method: HttpMethod::Get,
            path: "/weather/{city}".to_owned(),
        };
        let request = build_connector_request(
            &connector,
            &operation,
            &json!({ "pathParams": { "city": "New York" }, "query": { "units": "metric" } }),
        )
        .unwrap();
        assert_eq!(
            request.url,
            "https://api.example.com/v1/weather/New%20York?units=metric"
        );
    }

    #[test]
    fn imports_documented_openapi_subset_and_rejects_refs() {
        let document = json!({
            "openapi": "3.1.0",
            "servers": [{ "url": "https://api.example.com/v1" }],
            "paths": {
                "/weather/{city}": {
                    "get": { "operationId": "weather.get", "parameters": [] }
                }
            }
        });
        let connector = import_openapi(&document, "weather").unwrap();
        assert_eq!(connector.operations.len(), 1);
        assert_eq!(connector.operations[0].id, "weather.get");

        let mut referenced = document;
        referenced["paths"]["/weather/{city}"]["get"]["responses"] =
            json!({ "200": { "$ref": "#/components/responses/Weather" } });
        assert_eq!(
            import_openapi(&referenced, "weather").unwrap_err().code,
            "unsupported_reference"
        );
        assert_eq!(
            import_openapi_json(&" ".repeat(MAX_OPENAPI_DOCUMENT_BYTES + 1), "weather")
                .unwrap_err()
                .code,
            "openapi_too_large"
        );
    }

    #[test]
    fn connector_validation_rejects_ambiguous_base_url() {
        let connector = Connector {
            id: "weather".to_owned(),
            kind: ConnectorKind::Http,
            base_url: "https://api.example.com/v1?token=definition-secret".to_owned(),
            operations: vec![ConnectorOperation {
                id: "weather.get".to_owned(),
                method: HttpMethod::Get,
                path: "/weather".to_owned(),
            }],
        };
        assert_eq!(
            validate_connector(&connector).unwrap_err().code,
            "invalid_base_url"
        );
    }
}
