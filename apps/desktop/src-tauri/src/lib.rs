//! Tauri command boundary for the AIX desktop host.

use std::collections::BTreeMap;
use std::path::PathBuf;
#[cfg(feature = "desktop")]
use std::sync::Mutex;

use aix_core::{AppDefinition, Capability, PermissionRequest, Resource, ResourceKind};
use aix_runtime::{
    AppSession, CancellationToken, ExecutionLimits, HostGrant, Runtime, RuntimeEvent,
    SqliteStateStore, TimerSubscription,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

type DesktopSession = AppSession<SqliteStateStore>;

/// Renderer-safe resource resolved by the host.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ResourceHandle {
    Text {
        text: String,
    },
    Data {
        data: Value,
    },
    Image {
        uri: String,
        #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
    },
    Audio {
        uri: String,
        #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
    },
    Video {
        uri: String,
        #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
    },
    Unavailable {
        reason: String,
    },
}

/// Minimal immutable view sent to React after each Runtime transaction.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopSnapshot {
    pub app_id: String,
    pub app_name: String,
    pub ui: aix_core::UiNode,
    pub state: Value,
    pub resources: BTreeMap<String, ResourceHandle>,
    pub timers: Vec<TimerSubscription>,
}

/// Sanitized error returned across IPC.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopError {
    pub code: String,
    pub message: String,
}

/// Validated permission request shown before any grant is constructed.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionPreview {
    pub token: u64,
    pub app_id: String,
    pub app_name: String,
    pub requests: Vec<PermissionRequest>,
}

/// Scopes explicitly selected in the trusted desktop shell.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionApproval {
    pub capability: Capability,
    pub scopes: Vec<String>,
}

struct PendingApp {
    token: u64,
    app: AppDefinition,
}

/// Stateful command implementation shared by Tauri commands and unit tests.
pub struct DesktopHost {
    database_path: PathBuf,
    session: Option<DesktopSession>,
    pending: Option<PendingApp>,
    next_token: u64,
}

impl DesktopHost {
    /// Create a host that persists app state at the supplied SQLite path.
    pub fn new(database_path: impl Into<PathBuf>) -> Self {
        Self {
            database_path: database_path.into(),
            session: None,
            pending: None,
            next_token: 1,
        }
    }

    /// Compatibility loader for apps that request no capabilities.
    pub fn load_app(&mut self, source: &str) -> Result<DesktopSnapshot, DesktopError> {
        let preview = self.prepare_app(source)?;
        if !preview.requests.is_empty() {
            return Err(DesktopError {
                code: "permission_approval_required".to_owned(),
                message: "review the requested capabilities before loading this app".to_owned(),
            });
        }
        self.activate_app(preview.token, vec![])
    }

    /// Validate a definition and return its requests without granting them.
    pub fn prepare_app(&mut self, source: &str) -> Result<PermissionPreview, DesktopError> {
        let runtime = Runtime::new().map_err(|error| desktop_error("runtime_init", error))?;
        let app = runtime
            .load_json(source)
            .map_err(|error| desktop_error("invalid_app", error))?;
        let token = self.next_token;
        self.next_token = self.next_token.wrapping_add(1).max(1);
        let preview = PermissionPreview {
            token,
            app_id: app.metadata.id.clone(),
            app_name: app.metadata.name.clone(),
            requests: app.permissions.clone(),
        };
        self.pending = Some(PendingApp { token, app });
        Ok(preview)
    }

    /// Construct grants from selected requested scopes, then load and start.
    pub fn activate_app(
        &mut self,
        token: u64,
        approvals: Vec<PermissionApproval>,
    ) -> Result<DesktopSnapshot, DesktopError> {
        let pending = self
            .pending
            .take()
            .filter(|pending| pending.token == token)
            .ok_or_else(|| DesktopError {
                code: "stale_permission_review".to_owned(),
                message: "validate this App Definition again before approving it".to_owned(),
            })?;
        let app = pending.app;
        let mut grants = Vec::new();
        for approval in approvals {
            let requested = app.permissions.iter().any(|request| {
                request.capability == approval.capability
                    && approval
                        .scopes
                        .iter()
                        .all(|scope| request.scopes.contains(scope))
            });
            if !requested {
                return Err(DesktopError {
                    code: "invalid_permission_approval".to_owned(),
                    message: "an approval contained a capability or scope the app did not request"
                        .to_owned(),
                });
            }
            grants.push(
                HostGrant::new(&app.metadata.id, approval.capability, approval.scopes)
                    .map_err(|error| desktop_error("invalid_permission_approval", error))?,
            );
        }
        let runtime = Runtime::new().map_err(|error| desktop_error("runtime_init", error))?;
        let store = SqliteStateStore::open(&self.database_path)
            .map_err(|error| desktop_error("state_store", error))?;
        let mut session =
            AppSession::with_grants(runtime, app, store, ExecutionLimits::default(), &grants)
                .map_err(|error| desktop_error("session_init", error))?;
        session
            .dispatch(
                &RuntimeEvent::AppStart {
                    payload: Value::Null,
                },
                &CancellationToken::new(),
            )
            .map_err(|error| desktop_error("workflow_failed", error))?;
        let snapshot = build_snapshot(&session);
        self.session = Some(session);
        Ok(snapshot)
    }

    /// Dispatch one typed event through the loaded app session.
    pub fn dispatch(&mut self, event: &RuntimeEvent) -> Result<DesktopSnapshot, DesktopError> {
        let session = self.session.as_mut().ok_or_else(|| DesktopError {
            code: "app_not_loaded".to_owned(),
            message: "load an AIX App Definition before dispatching events".to_owned(),
        })?;
        session
            .dispatch(event, &CancellationToken::new())
            .map_err(|error| desktop_error("workflow_failed", error))?;
        Ok(build_snapshot(session))
    }
}

#[cfg(feature = "desktop")]
#[tauri::command]
fn prepare_app(
    source: String,
    host: tauri::State<'_, Mutex<DesktopHost>>,
) -> Result<PermissionPreview, DesktopError> {
    lock_host(&host)?.prepare_app(&source)
}

#[cfg(feature = "desktop")]
#[tauri::command]
fn activate_app(
    token: u64,
    approvals: Vec<PermissionApproval>,
    host: tauri::State<'_, Mutex<DesktopHost>>,
) -> Result<DesktopSnapshot, DesktopError> {
    lock_host(&host)?.activate_app(token, approvals)
}

#[cfg(feature = "desktop")]
#[tauri::command]
fn load_app(
    source: String,
    host: tauri::State<'_, Mutex<DesktopHost>>,
) -> Result<DesktopSnapshot, DesktopError> {
    lock_host(&host)?.load_app(&source)
}

#[cfg(feature = "desktop")]
#[tauri::command]
fn dispatch_event(
    event: RuntimeEvent,
    host: tauri::State<'_, Mutex<DesktopHost>>,
) -> Result<DesktopSnapshot, DesktopError> {
    lock_host(&host)?.dispatch(&event)
}

#[cfg(feature = "desktop")]
fn lock_host<'a>(
    host: &'a tauri::State<'_, Mutex<DesktopHost>>,
) -> Result<std::sync::MutexGuard<'a, DesktopHost>, DesktopError> {
    host.lock().map_err(|_| DesktopError {
        code: "host_unavailable".to_owned(),
        message: "desktop Runtime state is unavailable".to_owned(),
    })
}

/// Start the native Tauri application.
#[cfg(feature = "desktop")]
pub fn run() {
    use tauri::Manager;

    tauri::Builder::default()
        .setup(|app| {
            let directory = app.path().app_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            app.manage(Mutex::new(DesktopHost::new(directory.join("state.sqlite"))));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_app,
            prepare_app,
            activate_app,
            dispatch_event
        ])
        .run(tauri::generate_context!())
        .expect("failed to run AIX desktop host");
}

fn build_snapshot(session: &DesktopSession) -> DesktopSnapshot {
    let app = session.app();
    DesktopSnapshot {
        app_id: app.metadata.id.clone(),
        app_name: app.metadata.name.clone(),
        ui: app.ui.clone(),
        state: session.state().clone(),
        resources: resolve_resources(app),
        timers: session.timer_subscriptions(),
    }
}

fn resolve_resources(app: &AppDefinition) -> BTreeMap<String, ResourceHandle> {
    app.resources
        .iter()
        .map(|(id, resource)| (id.clone(), resolve_resource(resource)))
        .collect()
}

fn resolve_resource(resource: &Resource) -> ResourceHandle {
    match &resource.kind {
        ResourceKind::Text => resource
            .value
            .as_str()
            .map(|text| ResourceHandle::Text {
                text: text.to_owned(),
            })
            .unwrap_or_else(invalid_resource),
        ResourceKind::Data => ResourceHandle::Data {
            data: resource.value.clone(),
        },
        ResourceKind::Image => media_resource(resource, "image", |uri, mime_type| {
            ResourceHandle::Image { uri, mime_type }
        }),
        ResourceKind::Audio => media_resource(resource, "audio", |uri, mime_type| {
            ResourceHandle::Audio { uri, mime_type }
        }),
        ResourceKind::Video => media_resource(resource, "video", |uri, mime_type| {
            ResourceHandle::Video { uri, mime_type }
        }),
    }
}

fn media_resource(
    resource: &Resource,
    family: &str,
    constructor: impl FnOnce(String, Option<String>) -> ResourceHandle,
) -> ResourceHandle {
    let Some(uri) = resource.value.as_str() else {
        return invalid_resource();
    };
    let allowed: &[&str] = match family {
        "image" => [
            "data:image/png",
            "data:image/jpeg",
            "data:image/gif",
            "data:image/webp",
        ]
        .as_slice(),
        "audio" => ["data:audio/mpeg", "data:audio/ogg", "data:audio/wav"].as_slice(),
        "video" => ["data:video/mp4", "data:video/webm"].as_slice(),
        _ => &[],
    };
    if !allowed.iter().any(|prefix| uri.starts_with(prefix)) {
        return ResourceHandle::Unavailable {
            reason: "media must be a supported host-approved data URI".to_owned(),
        };
    }
    constructor(uri.to_owned(), resource.mime_type.clone())
}

fn invalid_resource() -> ResourceHandle {
    ResourceHandle::Unavailable {
        reason: "resource value has the wrong type".to_owned(),
    }
}

fn desktop_error(code: &str, error: impl std::fmt::Display) -> DesktopError {
    DesktopError {
        code: code.to_owned(),
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn database_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("aix-desktop-{name}-{}.sqlite", std::process::id()))
    }

    #[test]
    fn load_runs_app_start_and_ui_events_return_new_snapshots() {
        let path = database_path("events");
        let _ = std::fs::remove_file(&path);
        let mut host = DesktopHost::new(&path);
        let source = include_str!("../../../../examples/workflow.aix.json");
        let loaded = host.load_app(source).unwrap();
        assert_eq!(loaded.state["status"], "ready");
        let changed = host
            .dispatch(&RuntimeEvent::UiChange {
                target: "city".to_owned(),
                payload: serde_json::json!({ "value": "Tokyo" }),
            })
            .unwrap();
        assert_eq!(changed.state["city"], "Tokyo");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn external_media_never_becomes_a_renderer_uri() {
        let path = database_path("media");
        let _ = std::fs::remove_file(&path);
        let source = include_str!("../../../../examples/hello.aix.json")
            .replace("\"resources\": {", "\"resources\": { \"remote\": { \"type\": \"image\", \"value\": \"https://example.com/x.png\" },");
        let snapshot = DesktopHost::new(&path).load_app(&source).unwrap();
        assert!(matches!(
            snapshot.resources["remote"],
            ResourceHandle::Unavailable { .. }
        ));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rejects_events_before_an_app_is_loaded() {
        let mut host = DesktopHost::new(database_path("empty"));
        let error = host
            .dispatch(&RuntimeEvent::UiClick {
                target: "button".to_owned(),
                payload: Value::Null,
            })
            .unwrap_err();
        assert_eq!(error.code, "app_not_loaded");
    }

    #[test]
    fn permission_review_rejects_scopes_the_app_did_not_request() {
        let path = database_path("permissions");
        let _ = std::fs::remove_file(&path);
        let source = include_str!("../../../../examples/hello.aix.json").replace(
            "\"permissions\": []",
            "\"permissions\": [{ \"capability\": \"network.request\", \"scopes\": [\"https://api.example.com/weather\"] }]",
        );
        let mut host = DesktopHost::new(&path);
        let preview = host.prepare_app(&source).unwrap();
        assert_eq!(preview.requests.len(), 1);
        let error = host
            .activate_app(
                preview.token,
                vec![PermissionApproval {
                    capability: Capability::NetworkRequest,
                    scopes: vec!["https://evil.example".to_owned()],
                }],
            )
            .unwrap_err();
        assert_eq!(error.code, "invalid_permission_approval");
        let _ = std::fs::remove_file(path);
    }
}
