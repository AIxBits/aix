//! Desktop-owned AI provider profiles, credentials, generation and export.

use std::collections::{BTreeSet, HashMap};
use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aix_authoring::{
    AppBuilder, AppGenerationProvider, BuilderErrorCode, GenerateAppRequest, ProviderError,
    ProviderRequest, ProviderResponse, MAX_PROVIDER_OUTPUT_BYTES,
};
use aix_core::PermissionRequest;
use aix_runtime::Runtime;
use async_trait::async_trait;
use reqwest::blocking::Client;
use reqwest::redirect::Policy;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use url::Url;

use crate::{desktop_error, DesktopError};

const MAX_PROFILE_ID_BYTES: usize = 64;
const MAX_PROFILE_NAME_BYTES: usize = 128;
const MAX_MODEL_BYTES: usize = 256;
const MAX_BASE_URL_BYTES: usize = 2_048;
const MAX_DIFF_ENTRIES: usize = 256;
const PROVIDER_TIMEOUT: Duration = Duration::from_secs(60);
#[cfg(feature = "desktop")]
const KEYRING_SERVICE: &str = "io.aixbits.runtime.providers";

/// Supported OpenAI chat-completions compatible provider families.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    OpenAiCompatible,
    LocalOpenAiCompatible,
}

/// Non-secret provider profile returned to the desktop UI.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderProfile {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub model: String,
    pub has_api_key: bool,
}

/// Profile update. An omitted key preserves the credential already in the OS store.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderProfileInput {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

/// Request to create or revise an App Definition.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GenerateDefinitionInput {
    pub profile_id: String,
    pub requirement: String,
    #[serde(default)]
    pub existing_source: Option<String>,
}

/// A bounded semantic change shown before accepting generated output.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionChange {
    pub path: String,
    pub kind: String,
}

/// Validated model output returned for review, never activated automatically.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedDefinition {
    pub app_id: String,
    pub app_name: String,
    pub source: String,
    pub attempts: u8,
    pub permissions: Vec<PermissionRequest>,
    pub changes: Vec<DefinitionChange>,
}

/// Supported App Definition export formats.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Json,
    Yaml,
}

/// Result of writing a generated definition into the user's export directory.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedDefinition {
    pub path: String,
}

pub(crate) trait SecretStore: Send + Sync {
    fn set(&self, profile_id: &str, secret: &str) -> Result<(), DesktopError>;
    fn get(&self, profile_id: &str) -> Result<Option<String>, DesktopError>;
    fn delete(&self, profile_id: &str) -> Result<(), DesktopError>;
}

#[derive(Default)]
pub(crate) struct MemorySecretStore {
    values: Mutex<HashMap<String, String>>,
}

impl SecretStore for MemorySecretStore {
    fn set(&self, profile_id: &str, secret: &str) -> Result<(), DesktopError> {
        self.values
            .lock()
            .map_err(|_| secret_error())?
            .insert(profile_id.to_owned(), secret.to_owned());
        Ok(())
    }

    fn get(&self, profile_id: &str) -> Result<Option<String>, DesktopError> {
        Ok(self
            .values
            .lock()
            .map_err(|_| secret_error())?
            .get(profile_id)
            .cloned())
    }

    fn delete(&self, profile_id: &str) -> Result<(), DesktopError> {
        self.values
            .lock()
            .map_err(|_| secret_error())?
            .remove(profile_id);
        Ok(())
    }
}

#[cfg(feature = "desktop")]
pub(crate) struct OsSecretStore;

#[cfg(feature = "desktop")]
impl SecretStore for OsSecretStore {
    fn set(&self, profile_id: &str, secret: &str) -> Result<(), DesktopError> {
        keyring_entry(profile_id)?
            .set_password(secret)
            .map_err(|_| secret_error())
    }

    fn get(&self, profile_id: &str) -> Result<Option<String>, DesktopError> {
        match keyring_entry(profile_id)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(secret_error()),
        }
    }

    fn delete(&self, profile_id: &str) -> Result<(), DesktopError> {
        match keyring_entry(profile_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(secret_error()),
        }
    }
}

#[cfg(feature = "desktop")]
fn keyring_entry(profile_id: &str) -> Result<keyring::Entry, DesktopError> {
    keyring::Entry::new(KEYRING_SERVICE, profile_id).map_err(|_| secret_error())
}

fn secret_error() -> DesktopError {
    DesktopError {
        code: "credential_store_unavailable".to_owned(),
        message: "the operating system credential store is unavailable".to_owned(),
    }
}

/// Host service that keeps profile metadata separate from OS-backed credentials.
pub struct AuthoringHost {
    database_path: PathBuf,
    export_directory: PathBuf,
    secrets: Arc<dyn SecretStore>,
}

impl AuthoringHost {
    pub(crate) fn new(
        database_path: impl Into<PathBuf>,
        export_directory: impl Into<PathBuf>,
        secrets: Arc<dyn SecretStore>,
    ) -> Self {
        Self {
            database_path: database_path.into(),
            export_directory: export_directory.into(),
            secrets,
        }
    }

    pub fn list_profiles(&self) -> Result<Vec<ProviderProfile>, DesktopError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT id, name, kind, base_url, model FROM provider_profiles ORDER BY name, id",
            )
            .map_err(|error| desktop_error("profile_store", error))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(|error| desktop_error("profile_store", error))?;
        let mut profiles = Vec::new();
        for row in rows {
            let (id, name, kind, base_url, model) =
                row.map_err(|error| desktop_error("profile_store", error))?;
            let kind = parse_kind(&kind)?;
            profiles.push(ProviderProfile {
                has_api_key: self.secrets.get(&id)?.is_some(),
                id,
                name,
                kind,
                base_url,
                model,
            });
        }
        Ok(profiles)
    }

    pub fn save_profile(
        &self,
        mut input: ProviderProfileInput,
    ) -> Result<ProviderProfile, DesktopError> {
        input.id = input.id.trim().to_owned();
        input.name = input.name.trim().to_owned();
        input.base_url = input.base_url.trim().trim_end_matches('/').to_owned();
        input.model = input.model.trim().to_owned();
        validate_profile(&input)?;
        if let Some(secret) = input.api_key.take() {
            let secret = secret.trim();
            if secret.is_empty() {
                self.secrets.delete(&input.id)?;
            } else {
                self.secrets.set(&input.id, secret)?;
            }
        }
        let connection = self.connection()?;
        connection
            .execute(
                "INSERT INTO provider_profiles (id, name, kind, base_url, model) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(id) DO UPDATE SET name=excluded.name, kind=excluded.kind, base_url=excluded.base_url, model=excluded.model",
                params![input.id, input.name, kind_name(&input.kind), input.base_url, input.model],
            )
            .map_err(|error| desktop_error("profile_store", error))?;
        Ok(ProviderProfile {
            has_api_key: self.secrets.get(&input.id)?.is_some(),
            id: input.id,
            name: input.name,
            kind: input.kind,
            base_url: input.base_url,
            model: input.model,
        })
    }

    pub fn delete_profile(&self, profile_id: &str) -> Result<(), DesktopError> {
        validate_id(profile_id)?;
        self.secrets.delete(profile_id)?;
        self.connection()?
            .execute("DELETE FROM provider_profiles WHERE id = ?1", [profile_id])
            .map_err(|error| desktop_error("profile_store", error))?;
        Ok(())
    }

    pub fn generate(
        &self,
        input: GenerateDefinitionInput,
    ) -> Result<GeneratedDefinition, DesktopError> {
        let profile = self
            .list_profiles()?
            .into_iter()
            .find(|profile| profile.id == input.profile_id)
            .ok_or_else(|| DesktopError {
                code: "provider_profile_not_found".to_owned(),
                message: "select a saved provider profile".to_owned(),
            })?;
        let api_key = self.secrets.get(&profile.id)?;
        if profile.kind == ProviderKind::OpenAiCompatible && api_key.is_none() {
            return Err(DesktopError {
                code: "provider_key_required".to_owned(),
                message: "this provider profile requires an API key".to_owned(),
            });
        }
        let runtime = Runtime::new().map_err(|error| desktop_error("runtime_init", error))?;
        let existing_definition = input
            .existing_source
            .as_deref()
            .map(|source| {
                runtime
                    .load_json(source)
                    .map_err(|error| desktop_error("invalid_existing_app", error))
            })
            .transpose()?;
        let provider = Arc::new(OpenAiCompatibleProvider::new(&profile, api_key)?);
        let builder =
            AppBuilder::new(provider).map_err(|error| desktop_error("authoring_init", error))?;
        let generated = futures::executor::block_on(builder.generate(GenerateAppRequest {
            requirement: input.requirement,
            existing_definition,
        }))
        .map_err(|error| DesktopError {
            code: authoring_error_code(&error.code).to_owned(),
            message: error.message,
        })?;
        let old = input
            .existing_source
            .as_deref()
            .and_then(|source| serde_json::from_str::<Value>(source).ok());
        let new: Value = serde_json::from_str(&generated.source)
            .map_err(|error| desktop_error("generated_serialization", error))?;
        let changes = definition_diff(old.as_ref(), &new);
        Ok(GeneratedDefinition {
            app_id: generated.definition.metadata.id,
            app_name: generated.definition.metadata.name,
            permissions: generated.definition.permissions,
            source: generated.source,
            attempts: generated.attempts,
            changes,
        })
    }

    pub fn save_definition(
        &self,
        source: &str,
        format: ExportFormat,
    ) -> Result<SavedDefinition, DesktopError> {
        let app = Runtime::new()
            .map_err(|error| desktop_error("runtime_init", error))?
            .load_json(source)
            .map_err(|error| desktop_error("invalid_app", error))?;
        std::fs::create_dir_all(&self.export_directory)
            .map_err(|error| desktop_error("export_failed", error))?;
        let (contents, extension) = match format {
            ExportFormat::Json => (
                serde_json::to_string_pretty(&app)
                    .map_err(|error| desktop_error("export_failed", error))?,
                "json",
            ),
            ExportFormat::Yaml => (
                serde_yaml::to_string(&app)
                    .map_err(|error| desktop_error("export_failed", error))?,
                "yaml",
            ),
        };
        let path = self
            .export_directory
            .join(format!("{}.aix.{extension}", app.metadata.id));
        let temporary = path.with_extension(format!("{extension}.tmp"));
        std::fs::write(&temporary, contents)
            .and_then(|_| std::fs::rename(&temporary, &path))
            .map_err(|error| desktop_error("export_failed", error))?;
        Ok(SavedDefinition {
            path: path.to_string_lossy().into_owned(),
        })
    }

    fn connection(&self) -> Result<Connection, DesktopError> {
        if let Some(parent) = self.database_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| desktop_error("profile_store", error))?;
        }
        let connection = Connection::open(&self.database_path)
            .map_err(|error| desktop_error("profile_store", error))?;
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS provider_profiles (id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL, kind TEXT NOT NULL, base_url TEXT NOT NULL, model TEXT NOT NULL);",
            )
            .map_err(|error| desktop_error("profile_store", error))?;
        Ok(connection)
    }
}

struct OpenAiCompatibleProvider {
    endpoint: Url,
    model: String,
    api_key: Option<String>,
    client: Client,
}

impl OpenAiCompatibleProvider {
    fn new(profile: &ProviderProfile, api_key: Option<String>) -> Result<Self, DesktopError> {
        let endpoint = provider_endpoint(&profile.base_url, &profile.kind)?;
        let client = Client::builder()
            .timeout(PROVIDER_TIMEOUT)
            .redirect(Policy::none())
            .build()
            .map_err(|_| provider_configuration_error("HTTP client could not be initialized"))?;
        Ok(Self {
            endpoint,
            model: profile.model.clone(),
            api_key,
            client,
        })
    }
}

#[async_trait]
impl AppGenerationProvider for OpenAiCompatibleProvider {
    async fn generate(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        let mut builder = self.client.post(self.endpoint.clone()).json(&json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": request.system },
                { "role": "user", "content": request.user }
            ]
        }));
        if let Some(api_key) = &self.api_key {
            builder = builder.bearer_auth(api_key);
        }
        let response = builder.send().map_err(|error| {
            if error.is_timeout() {
                ProviderError::new("timeout", "provider request timed out")
            } else {
                ProviderError::new("transport", "provider request failed")
            }
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(ProviderError::new(
                "http_status",
                format!("provider returned HTTP {}", status.as_u16()),
            ));
        }
        let mut bytes = Vec::new();
        response
            .take((MAX_PROVIDER_OUTPUT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|_| {
                ProviderError::new("response_read", "provider response could not be read")
            })?;
        if bytes.len() > MAX_PROVIDER_OUTPUT_BYTES {
            return Err(ProviderError::new(
                "response_too_large",
                "provider response exceeds 1 MiB",
            ));
        }
        let envelope: Value = serde_json::from_slice(&bytes).map_err(|_| {
            ProviderError::new("invalid_response", "provider returned invalid JSON")
        })?;
        let content = envelope
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                ProviderError::new(
                    "invalid_response",
                    "provider response has no assistant message content",
                )
            })?;
        Ok(ProviderResponse {
            content: content.to_owned(),
        })
    }
}

fn validate_profile(input: &ProviderProfileInput) -> Result<(), DesktopError> {
    validate_id(&input.id)?;
    if input.name.is_empty() || input.name.len() > MAX_PROFILE_NAME_BYTES {
        return Err(provider_configuration_error(
            "profile name must contain 1 to 128 bytes",
        ));
    }
    if input.model.is_empty() || input.model.len() > MAX_MODEL_BYTES {
        return Err(provider_configuration_error(
            "model must contain 1 to 256 bytes",
        ));
    }
    provider_endpoint(&input.base_url, &input.kind)?;
    Ok(())
}

fn validate_id(profile_id: &str) -> Result<(), DesktopError> {
    if profile_id.is_empty()
        || profile_id.len() > MAX_PROFILE_ID_BYTES
        || !profile_id.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        return Err(provider_configuration_error(
            "profile id must use 1 to 64 lowercase letters, digits, or hyphens",
        ));
    }
    Ok(())
}

fn provider_endpoint(base_url: &str, kind: &ProviderKind) -> Result<Url, DesktopError> {
    if base_url.is_empty() || base_url.len() > MAX_BASE_URL_BYTES {
        return Err(provider_configuration_error("provider URL is invalid"));
    }
    let mut url = Url::parse(base_url)
        .map_err(|_| provider_configuration_error("provider URL is invalid"))?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.host_str().is_none()
    {
        return Err(provider_configuration_error(
            "provider URL cannot contain credentials, query, or fragment",
        ));
    }
    match kind {
        ProviderKind::OpenAiCompatible if url.scheme() != "https" => {
            return Err(provider_configuration_error(
                "hosted provider URLs must use HTTPS",
            ));
        }
        ProviderKind::LocalOpenAiCompatible if url.scheme() == "http" => {
            let local = url.host_str().is_some_and(|host| {
                host.eq_ignore_ascii_case("localhost")
                    || host
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())
            });
            if !local {
                return Err(provider_configuration_error(
                    "unencrypted local providers must use a loopback address",
                ));
            }
        }
        ProviderKind::LocalOpenAiCompatible if url.scheme() != "https" => {
            return Err(provider_configuration_error(
                "local provider URLs must use HTTPS or loopback HTTP",
            ));
        }
        _ => {}
    }
    if !url
        .path()
        .trim_end_matches('/')
        .ends_with("/chat/completions")
    {
        let path = format!("{}/chat/completions", url.path().trim_end_matches('/'));
        url.set_path(&path);
    }
    Ok(url)
}

fn provider_configuration_error(message: &str) -> DesktopError {
    DesktopError {
        code: "invalid_provider_profile".to_owned(),
        message: message.to_owned(),
    }
}

fn authoring_error_code(code: &BuilderErrorCode) -> &'static str {
    match code {
        BuilderErrorCode::InvalidRequest => "authoring_invalid_request",
        BuilderErrorCode::InvalidConfiguration => "authoring_invalid_configuration",
        BuilderErrorCode::ProviderFailure => "authoring_provider_failure",
        BuilderErrorCode::OutputTooLarge => "authoring_output_too_large",
        BuilderErrorCode::ValidationFailed => "authoring_validation_failed",
    }
}

fn kind_name(kind: &ProviderKind) -> &'static str {
    match kind {
        ProviderKind::OpenAiCompatible => "openai_compatible",
        ProviderKind::LocalOpenAiCompatible => "local_openai_compatible",
    }
}

fn parse_kind(kind: &str) -> Result<ProviderKind, DesktopError> {
    match kind {
        "openai_compatible" => Ok(ProviderKind::OpenAiCompatible),
        "local_openai_compatible" => Ok(ProviderKind::LocalOpenAiCompatible),
        _ => Err(DesktopError {
            code: "profile_store".to_owned(),
            message: "a provider profile has an unsupported kind".to_owned(),
        }),
    }
}

fn definition_diff(old: Option<&Value>, new: &Value) -> Vec<DefinitionChange> {
    let mut changes = Vec::new();
    match old {
        Some(old) => diff_value(old, new, "", &mut changes),
        None => changes.push(DefinitionChange {
            path: "/".to_owned(),
            kind: "added".to_owned(),
        }),
    }
    changes
}

fn diff_value(old: &Value, new: &Value, path: &str, changes: &mut Vec<DefinitionChange>) {
    if changes.len() >= MAX_DIFF_ENTRIES || old == new {
        return;
    }
    match (old, new) {
        (Value::Object(old), Value::Object(new)) => {
            let keys: BTreeSet<_> = old.keys().chain(new.keys()).collect();
            for key in keys {
                if changes.len() >= MAX_DIFF_ENTRIES {
                    break;
                }
                let escaped = key.replace('~', "~0").replace('/', "~1");
                let child = format!("{path}/{escaped}");
                match (old.get(key), new.get(key)) {
                    (None, Some(_)) => changes.push(DefinitionChange {
                        path: child,
                        kind: "added".to_owned(),
                    }),
                    (Some(_), None) => changes.push(DefinitionChange {
                        path: child,
                        kind: "removed".to_owned(),
                    }),
                    (Some(old), Some(new)) => diff_value(old, new, &child, changes),
                    (None, None) => {}
                }
            }
        }
        _ => changes.push(DefinitionChange {
            path: if path.is_empty() {
                "/".to_owned()
            } else {
                path.to_owned()
            },
            kind: "changed".to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aix_core::AppDefinition;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::Path;
    use std::thread;

    fn test_host(name: &str) -> (AuthoringHost, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("aix-authoring-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let host = AuthoringHost::new(
            root.join("state.sqlite"),
            root.join("exports"),
            Arc::new(MemorySecretStore::default()),
        );
        (host, root)
    }

    fn profile(key: Option<&str>) -> ProviderProfileInput {
        ProviderProfileInput {
            id: "local".to_owned(),
            name: "Local model".to_owned(),
            kind: ProviderKind::LocalOpenAiCompatible,
            base_url: "http://127.0.0.1:11434/v1".to_owned(),
            model: "test-model".to_owned(),
            api_key: key.map(str::to_owned),
        }
    }

    #[test]
    fn profile_metadata_persists_without_the_api_key() {
        let (host, root) = test_host("profiles");
        let saved = host.save_profile(profile(Some("secret-value"))).unwrap();
        assert!(saved.has_api_key);
        let database = std::fs::read(root.join("state.sqlite")).unwrap();
        assert!(!String::from_utf8_lossy(&database).contains("secret-value"));
        assert_eq!(host.list_profiles().unwrap()[0].model, "test-model");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn local_plain_http_is_confined_to_loopback() {
        let mut input = profile(None);
        input.base_url = "http://example.com/v1".to_owned();
        assert_eq!(
            validate_profile(&input).unwrap_err().code,
            "invalid_provider_profile"
        );
    }

    #[test]
    fn exports_validated_json_and_yaml() {
        let (host, root) = test_host("exports");
        let source = include_str!("../../../../examples/hello.aix.json");
        let json = host.save_definition(source, ExportFormat::Json).unwrap();
        let yaml = host.save_definition(source, ExportFormat::Yaml).unwrap();
        assert!(Path::new(&json.path).exists());
        assert!(Path::new(&yaml.path).exists());
        let parsed: AppDefinition =
            serde_yaml::from_str(&std::fs::read_to_string(yaml.path).unwrap()).unwrap();
        assert_eq!(parsed.metadata.id, "hello");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn diff_reports_json_pointer_changes() {
        let old = json!({"state":{"city":"Paris"},"permissions":[]});
        let new = json!({"state":{"city":"Tokyo"},"permissions":[{"x":1}]});
        let changes = definition_diff(Some(&old), &new);
        assert!(changes.iter().any(|change| change.path == "/state/city"));
        assert!(changes.iter().any(|change| change.path == "/permissions"));
    }

    #[test]
    fn local_provider_generates_through_the_validating_builder() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 8_192];
            loop {
                let read = stream.read(&mut buffer).unwrap();
                request.extend_from_slice(&buffer[..read]);
                let Some(header_end) = request.windows(4).position(|part| part == b"\r\n\r\n")
                else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .map(str::to_owned)
                    })
                    .and_then(|length| length.parse::<usize>().ok())
                    .unwrap();
                if request.len() >= header_end + 4 + content_length {
                    break;
                }
            }
            assert!(
                String::from_utf8_lossy(&request).starts_with("POST /v1/chat/completions HTTP/1.1")
            );
            let response_body = serde_json::to_string(&json!({
                "choices": [{ "message": { "content": include_str!("../../../../examples/hello.aix.json") } }]
            }))
            .unwrap();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(), response_body
            )
            .unwrap();
        });
        let (host, root) = test_host("local-provider");
        let mut input = profile(None);
        input.base_url = format!("http://{address}/v1");
        host.save_profile(input).unwrap();
        let generated = host
            .generate(GenerateDefinitionInput {
                profile_id: "local".to_owned(),
                requirement: "Create a greeting app".to_owned(),
                existing_source: None,
            })
            .unwrap();
        assert_eq!(generated.app_id, "hello");
        assert_eq!(generated.attempts, 1);
        server.join().unwrap();
        let _ = std::fs::remove_dir_all(root);
    }
}
