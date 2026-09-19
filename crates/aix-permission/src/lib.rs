//! Host-owned capability grants and scoped authorization.
//!
//! An app's declarations are requests only. Authorization requires both the
//! declaration and a separately constructed host grant to cover the concrete
//! target of an operation invocation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use aix_core::{Capability, PermissionRequest};
use serde::Serialize;
use url::Url;

/// Concrete resource an operation wants to access.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PermissionTarget {
    NetworkUrl(String),
    FilePath(PathBuf),
    Named(String),
}

/// One decision requested by the Runtime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionCheck {
    pub operation_id: String,
    pub capability: Capability,
    pub target: PermissionTarget,
}

/// User-approved grant constructed by trusted host code.
///
/// It deliberately does not implement `Deserialize`; an App Definition cannot
/// turn its own request into a grant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostGrant {
    app_id: String,
    capability: Capability,
    scopes: Vec<String>,
}

impl HostGrant {
    pub fn new(
        app_id: impl Into<String>,
        capability: Capability,
        scopes: Vec<String>,
    ) -> Result<Self, ResolverBuildError> {
        let app_id = app_id.into();
        if app_id.trim().is_empty()
            || scopes.is_empty()
            || scopes.iter().any(|scope| scope.trim().is_empty())
        {
            return Err(ResolverBuildError::new(
                "invalid_grant",
                "grant app id and scopes must be non-empty",
            ));
        }
        Ok(Self {
            app_id,
            capability,
            scopes,
        })
    }
}

/// Stable denial returned before an operation performs a side effect.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionDenied {
    pub capability: Capability,
    pub code: String,
    pub message: String,
}

/// Invalid declarations or grants found while building a resolver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolverBuildError {
    pub code: String,
    pub message: String,
}

impl ResolverBuildError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ResolverBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ResolverBuildError {}

/// Authorization boundary used by the Runtime.
pub trait PermissionResolver: Send + Sync {
    fn authorize(&self, check: &PermissionCheck) -> Result<(), PermissionDenied>;

    /// Decide app-local effects that do not expose a host capability.
    fn authorize_local_effect(&self, _operation_id: &str, effect: &str) -> bool {
        matches!(effect, "state.write" | "time.read")
    }
}

/// Resolver used when the host has approved nothing.
#[derive(Clone, Debug, Default)]
pub struct DenyAllResolver;

impl PermissionResolver for DenyAllResolver {
    fn authorize(&self, check: &PermissionCheck) -> Result<(), PermissionDenied> {
        Err(denied(
            &check.capability,
            "no host grant covers this request",
        ))
    }
}

#[derive(Clone, Debug)]
enum Scope {
    Network(NetworkScope),
    File(PathBuf),
    Named(String),
}

#[derive(Clone, Debug)]
struct NetworkScope {
    scheme: String,
    host: String,
    port: u16,
    path: String,
}

/// Resolver that intersects app requests with user-approved grants.
#[derive(Clone, Debug)]
pub struct CapabilityResolver {
    requests: BTreeMap<Capability, Vec<Scope>>,
    grants: BTreeMap<Capability, Vec<Scope>>,
}

impl CapabilityResolver {
    pub fn new(
        app_id: &str,
        requests: &[PermissionRequest],
        grants: &[HostGrant],
    ) -> Result<Self, ResolverBuildError> {
        let mut request_scopes = BTreeMap::new();
        for request in requests {
            add_scopes(&mut request_scopes, &request.capability, &request.scopes)?;
        }
        let mut grant_scopes = BTreeMap::new();
        for grant in grants.iter().filter(|grant| grant.app_id == app_id) {
            add_scopes(&mut grant_scopes, &grant.capability, &grant.scopes)?;
        }
        Ok(Self {
            requests: request_scopes,
            grants: grant_scopes,
        })
    }
}

impl PermissionResolver for CapabilityResolver {
    fn authorize(&self, check: &PermissionCheck) -> Result<(), PermissionDenied> {
        let matches = |map: &BTreeMap<Capability, Vec<Scope>>| {
            map.get(&check.capability).is_some_and(|scopes| {
                scopes
                    .iter()
                    .any(|scope| scope_matches(scope, &check.target, &check.capability))
            })
        };
        if matches(&self.requests) && matches(&self.grants) {
            Ok(())
        } else {
            Err(denied(
                &check.capability,
                "the app request and host grant do not both cover this target",
            ))
        }
    }
}

fn add_scopes(
    target: &mut BTreeMap<Capability, Vec<Scope>>,
    capability: &Capability,
    scopes: &[String],
) -> Result<(), ResolverBuildError> {
    if scopes.is_empty() {
        return Err(ResolverBuildError::new(
            "invalid_scope",
            "a capability must contain at least one scope",
        ));
    }
    let normalized = scopes
        .iter()
        .map(|scope| normalize_scope(capability, scope))
        .collect::<Result<Vec<_>, _>>()?;
    target
        .entry(capability.clone())
        .or_default()
        .extend(normalized);
    Ok(())
}

fn normalize_scope(capability: &Capability, value: &str) -> Result<Scope, ResolverBuildError> {
    match capability {
        Capability::NetworkRequest => normalize_url(value).map(Scope::Network),
        Capability::FileRead | Capability::FileWrite => {
            let path = std::fs::canonicalize(value).map_err(|error| {
                ResolverBuildError::new(
                    "invalid_file_scope",
                    format!("file scope `{value}` cannot be resolved: {error}"),
                )
            })?;
            if !path.is_dir() {
                return Err(ResolverBuildError::new(
                    "invalid_file_scope",
                    "file scopes must name existing directories",
                ));
            }
            Ok(Scope::File(path))
        }
        Capability::NotificationShow | Capability::AiGenerate => {
            if value.trim().is_empty() {
                Err(ResolverBuildError::new(
                    "invalid_scope",
                    "named scope cannot be empty",
                ))
            } else {
                Ok(Scope::Named(value.to_owned()))
            }
        }
    }
}

fn normalize_url(value: &str) -> Result<NetworkScope, ResolverBuildError> {
    let parsed = Url::parse(value)
        .map_err(|error| ResolverBuildError::new("invalid_url_scope", error.to_string()))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(ResolverBuildError::new(
            "invalid_url_scope",
            "network scopes require an http(s) URL without credentials, query, or fragment",
        ));
    }
    let path = parsed.path().to_ascii_lowercase();
    if ["%2f", "%5c", "%2e", "%00"]
        .iter()
        .any(|part| path.contains(part))
    {
        return Err(ResolverBuildError::new(
            "invalid_url_scope",
            "encoded separators and dot segments are not allowed",
        ));
    }
    let path = parsed.path().trim_end_matches('/');
    Ok(NetworkScope {
        scheme: parsed.scheme().to_owned(),
        host: parsed
            .host_str()
            .ok_or_else(|| ResolverBuildError::new("invalid_url_scope", "URL host is required"))?
            .to_owned(),
        port: parsed
            .port_or_known_default()
            .ok_or_else(|| ResolverBuildError::new("invalid_url_scope", "URL port is required"))?,
        path: if path.is_empty() {
            "/".to_owned()
        } else {
            path.to_owned()
        },
    })
}

fn scope_matches(scope: &Scope, target: &PermissionTarget, capability: &Capability) -> bool {
    match (scope, target) {
        (Scope::Network(scope), PermissionTarget::NetworkUrl(value)) => {
            normalize_url_target(value).is_some_and(|target| network_contains(scope, &target))
        }
        (Scope::File(directory), PermissionTarget::FilePath(path)) => {
            canonical_target(path, capability).is_some_and(|target| target.starts_with(directory))
        }
        (Scope::Named(scope), PermissionTarget::Named(target)) => scope == target,
        _ => false,
    }
}

fn normalize_url_target(value: &str) -> Option<NetworkScope> {
    let mut parsed = Url::parse(value).ok()?;
    if parsed.fragment().is_some() {
        return None;
    }
    parsed.set_query(None);
    normalize_url(parsed.as_str()).ok()
}

fn network_contains(scope: &NetworkScope, target: &NetworkScope) -> bool {
    if scope.scheme != target.scheme || scope.host != target.host || scope.port != target.port {
        return false;
    }
    scope.path == "/"
        || target.path == scope.path
        || target
            .path
            .strip_prefix(&scope.path)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn canonical_target(path: &Path, capability: &Capability) -> Option<PathBuf> {
    if matches!(capability, Capability::FileWrite) && !path.exists() {
        let parent = std::fs::canonicalize(path.parent()?).ok()?;
        return Some(parent.join(path.file_name()?));
    }
    std::fs::canonicalize(path).ok()
}

fn denied(capability: &Capability, message: &str) -> PermissionDenied {
    PermissionDenied {
        capability: capability.clone(),
        code: "permission_denied".to_owned(),
        message: message.to_owned(),
    }
}

/// Compatibility helper retained for callers of the Phase 1 placeholder.
pub fn is_allowed(_capability: &str, _scope: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(capability: Capability, scope: &str) -> PermissionRequest {
        PermissionRequest {
            capability,
            scopes: vec![scope.to_owned()],
        }
    }

    fn check(capability: Capability, target: PermissionTarget) -> PermissionCheck {
        PermissionCheck {
            operation_id: "test.operation".into(),
            capability,
            target,
        }
    }

    #[test]
    fn declaration_without_host_grant_is_denied() {
        let resolver = CapabilityResolver::new(
            "app",
            &[request(
                Capability::NetworkRequest,
                "https://api.example.com/v1",
            )],
            &[],
        )
        .unwrap();
        assert!(resolver
            .authorize(&check(
                Capability::NetworkRequest,
                PermissionTarget::NetworkUrl("https://api.example.com/v1/weather?q=x".into())
            ))
            .is_err());
    }

    #[test]
    fn network_scope_checks_origin_and_path_boundary() {
        let requests = [request(
            Capability::NetworkRequest,
            "https://api.example.com/v1",
        )];
        let grants = [HostGrant::new(
            "app",
            Capability::NetworkRequest,
            vec!["https://api.example.com/v1".into()],
        )
        .unwrap()];
        let resolver = CapabilityResolver::new("app", &requests, &grants).unwrap();
        let allows = |url: &str| {
            resolver
                .authorize(&check(
                    Capability::NetworkRequest,
                    PermissionTarget::NetworkUrl(url.into()),
                ))
                .is_ok()
        };
        assert!(allows("https://api.example.com/v1/weather?q=Paris"));
        assert!(!allows("https://api.example.com.evil/v1/weather"));
        assert!(!allows("https://api.example.com/v10/weather"));
        assert!(!allows("http://api.example.com/v1/weather"));
    }

    #[test]
    fn grant_for_another_app_does_not_apply() {
        let requests = [request(Capability::NotificationShow, "default")];
        let grants = [HostGrant::new(
            "other",
            Capability::NotificationShow,
            vec!["default".into()],
        )
        .unwrap()];
        let resolver = CapabilityResolver::new("app", &requests, &grants).unwrap();
        assert!(resolver
            .authorize(&check(
                Capability::NotificationShow,
                PermissionTarget::Named("default".into())
            ))
            .is_err());
    }

    #[test]
    fn file_scope_blocks_sibling_escape_and_allows_new_child() {
        let root = std::env::temp_dir().join(format!("aix-permission-{}", std::process::id()));
        let allowed = root.join("allowed");
        let sibling = root.join("sibling");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&sibling).unwrap();
        let scope = allowed.to_string_lossy().into_owned();
        let requests = [request(Capability::FileWrite, &scope)];
        let grants = [HostGrant::new("app", Capability::FileWrite, vec![scope]).unwrap()];
        let resolver = CapabilityResolver::new("app", &requests, &grants).unwrap();
        assert!(resolver
            .authorize(&check(
                Capability::FileWrite,
                PermissionTarget::FilePath(allowed.join("new.txt"))
            ))
            .is_ok());
        assert!(resolver
            .authorize(&check(
                Capability::FileWrite,
                PermissionTarget::FilePath(sibling.join("new.txt"))
            ))
            .is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn file_scope_blocks_symlink_escape() {
        use std::os::unix::fs::symlink;

        let root =
            std::env::temp_dir().join(format!("aix-permission-symlink-{}", std::process::id()));
        let allowed = root.join("allowed");
        let outside = root.join("outside");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), "secret").unwrap();
        symlink(&outside, allowed.join("link")).unwrap();
        let scope = allowed.to_string_lossy().into_owned();
        let requests = [request(Capability::FileRead, &scope)];
        let grants = [HostGrant::new("app", Capability::FileRead, vec![scope]).unwrap()];
        let resolver = CapabilityResolver::new("app", &requests, &grants).unwrap();
        assert!(resolver
            .authorize(&check(
                Capability::FileRead,
                PermissionTarget::FilePath(allowed.join("link/secret.txt"))
            ))
            .is_err());
        let _ = std::fs::remove_dir_all(root);
    }
}
