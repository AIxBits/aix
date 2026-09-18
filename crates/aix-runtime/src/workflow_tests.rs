use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::{
    AppSession, CancellationToken, ExecutionLimits, MemoryStateStore, Runtime, RuntimeEvent,
    SqliteStateStore, StateStore, StateStoreError, WorkflowErrorCode,
};

fn app_with_workflows(workflows: Value) -> aix_core::AppDefinition {
    let runtime = Runtime::new().unwrap();
    let mut app: Value =
        serde_json::from_str(include_str!("../../../examples/hello.aix.json")).unwrap();
    app["workflows"] = workflows;
    runtime.load_json(&app.to_string()).unwrap()
}

fn session(workflows: Value) -> AppSession<MemoryStateStore> {
    AppSession::new(
        Runtime::new().unwrap(),
        app_with_workflows(workflows),
        MemoryStateStore::new(),
    )
    .unwrap()
}

#[test]
fn resolves_event_state_and_completed_step_bindings() {
    let workflows = json!([{
        "id": "start",
        "on": { "type": "app.start" },
        "entry": "choose",
        "steps": [
            {
                "id": "write",
                "operation": "state.set",
                "input": {
                    "path": "/result",
                    "value": {
                        "choice": { "$step": "choose", "path": "/value" },
                        "city": { "$state": "/city" }
                    }
                }
            },
            {
                "id": "choose",
                "operation": "logic.if",
                "input": {
                    "condition": { "$event": "/enabled" },
                    "then": "ready",
                    "else": "disabled"
                },
                "next": ["write"]
            }
        ]
    }]);
    let mut session = session(workflows);
    let result = session
        .dispatch(
            &RuntimeEvent::AppStart {
                payload: json!({ "enabled": true }),
            },
            &CancellationToken::new(),
        )
        .unwrap();

    assert_eq!(
        result.workflows[0]
            .steps
            .iter()
            .map(|step| step.step_id.as_str())
            .collect::<Vec<_>>(),
        ["choose", "write"]
    );
    assert_eq!(
        result.state["result"],
        json!({ "choice": "ready", "city": "Shanghai" })
    );
}

#[test]
fn conditional_edges_activate_only_the_matching_branch() {
    let workflows = json!([{
        "id": "branch",
        "on": { "type": "app.start" },
        "entry": "choose",
        "steps": [
            {
                "id": "off",
                "operation": "state.set",
                "input": { "path": "/status", "value": "off" }
            },
            {
                "id": "on",
                "operation": "state.set",
                "input": { "path": "/status", "value": "on" }
            },
            {
                "id": "choose",
                "operation": "logic.if",
                "input": { "condition": true, "then": "on", "else": "off" },
                "next": [
                    { "step": "on", "when": { "path": "/value", "equals": "on" } },
                    { "step": "off", "when": { "path": "/value", "equals": "off" } }
                ]
            }
        ]
    }]);
    let mut session = session(workflows);
    let result = session
        .dispatch(
            &RuntimeEvent::AppStart {
                payload: Value::Null,
            },
            &CancellationToken::new(),
        )
        .unwrap();

    assert_eq!(result.state["status"], "on");
    assert_eq!(
        result.workflows[0]
            .steps
            .iter()
            .map(|step| step.step_id.as_str())
            .collect::<Vec<_>>(),
        ["choose", "on"]
    );
}

#[test]
fn dispatches_supported_events_to_matching_workflows() {
    let workflows = json!([
        workflow_for_event("started", json!({ "type": "app.start" }), "/started"),
        workflow_for_event(
            "clicked",
            json!({ "type": "ui.click", "target": "city" }),
            "/clicked"
        ),
        workflow_for_event(
            "changed",
            json!({ "type": "ui.change", "target": "city" }),
            "/changed"
        ),
        workflow_for_event(
            "heartbeat",
            json!({ "type": "timer", "intervalMs": 1000 }),
            "/timer"
        )
    ]);
    let mut session = session(workflows);
    let token = CancellationToken::new();

    assert_eq!(session.timer_subscriptions()[0].workflow_id, "heartbeat");
    session
        .dispatch(
            &RuntimeEvent::AppStart {
                payload: Value::Null,
            },
            &token,
        )
        .unwrap();
    assert!(session
        .dispatch(
            &RuntimeEvent::UiClick {
                target: "other".to_owned(),
                payload: Value::Null,
            },
            &token,
        )
        .unwrap()
        .workflows
        .is_empty());
    session
        .dispatch(
            &RuntimeEvent::UiClick {
                target: "city".to_owned(),
                payload: Value::Null,
            },
            &token,
        )
        .unwrap();
    session
        .dispatch(
            &RuntimeEvent::UiChange {
                target: "city".to_owned(),
                payload: json!({ "value": "Paris" }),
            },
            &token,
        )
        .unwrap();
    session
        .dispatch(
            &RuntimeEvent::Timer {
                workflow_id: "heartbeat".to_owned(),
                payload: Value::Null,
            },
            &token,
        )
        .unwrap();

    assert_eq!(
        session.state(),
        &json!({
            "city": "Shanghai",
            "started": true,
            "clicked": true,
            "changed": true,
            "timer": true
        })
    );
}

fn workflow_for_event(id: &str, on: Value, path: &str) -> Value {
    json!({
        "id": id,
        "on": on,
        "entry": "write",
        "steps": [{
            "id": "write",
            "operation": "state.set",
            "input": { "path": path, "value": true }
        }]
    })
}

#[test]
fn operation_failure_rolls_back_the_workflow_state() {
    let workflows = json!([{
        "id": "rollback",
        "on": { "type": "app.start" },
        "entry": "write",
        "steps": [
            {
                "id": "readMissing",
                "operation": "state.get",
                "input": { "path": "/missing" }
            },
            {
                "id": "write",
                "operation": "state.set",
                "input": { "path": "/temporary", "value": true },
                "next": ["readMissing"]
            }
        ]
    }]);
    let mut session = session(workflows);
    let error = session
        .dispatch(
            &RuntimeEvent::AppStart {
                payload: Value::Null,
            },
            &CancellationToken::new(),
        )
        .unwrap_err();

    assert_eq!(error.code, WorkflowErrorCode::OperationFailed);
    assert_eq!(error.step_id.as_deref(), Some("readMissing"));
    assert!(session.state().get("temporary").is_none());
}

struct FailOnCommitStore {
    state: RefCell<Option<Value>>,
    saves: Cell<usize>,
}

impl FailOnCommitStore {
    fn new() -> Self {
        Self {
            state: RefCell::new(None),
            saves: Cell::new(0),
        }
    }
}

impl StateStore for FailOnCommitStore {
    fn load(&self, _app_id: &str) -> Result<Option<Value>, StateStoreError> {
        Ok(self.state.borrow().clone())
    }

    fn save(&self, _app_id: &str, state: &Value) -> Result<(), StateStoreError> {
        let saves = self.saves.get();
        self.saves.set(saves + 1);
        if saves > 0 {
            return Err(StateStoreError {
                code: "fixture_failure".to_owned(),
                message: "commit rejected".to_owned(),
            });
        }
        self.state.replace(Some(state.clone()));
        Ok(())
    }
}

#[test]
fn persistence_failure_rolls_back_in_memory_state() {
    let workflows = json!([{
        "id": "persist",
        "on": { "type": "app.start" },
        "entry": "write",
        "steps": [{
            "id": "write",
            "operation": "state.set",
            "input": { "path": "/temporary", "value": true }
        }]
    }]);
    let mut session = AppSession::new(
        Runtime::new().unwrap(),
        app_with_workflows(workflows),
        FailOnCommitStore::new(),
    )
    .unwrap();
    let error = session
        .dispatch(
            &RuntimeEvent::AppStart {
                payload: Value::Null,
            },
            &CancellationToken::new(),
        )
        .unwrap_err();

    assert_eq!(error.code, WorkflowErrorCode::PersistenceFailed);
    assert!(session.state().get("temporary").is_none());
}

#[test]
fn cancellation_and_step_limits_roll_back_state() {
    let workflows = json!([{
        "id": "bounded",
        "on": { "type": "app.start" },
        "entry": "first",
        "steps": [
            { "id": "second", "operation": "state.set", "input": { "path": "/second", "value": true } },
            { "id": "first", "operation": "state.set", "input": { "path": "/first", "value": true }, "next": ["second"] }
        ]
    }]);
    let app = app_with_workflows(workflows.clone());
    let mut limited = AppSession::with_limits(
        Runtime::new().unwrap(),
        app,
        MemoryStateStore::new(),
        ExecutionLimits {
            max_steps_per_workflow: 1,
        },
    )
    .unwrap();
    let error = limited
        .dispatch(
            &RuntimeEvent::AppStart {
                payload: Value::Null,
            },
            &CancellationToken::new(),
        )
        .unwrap_err();
    assert_eq!(error.code, WorkflowErrorCode::StepLimitExceeded);
    assert!(limited.state().get("first").is_none());

    let mut cancelled = session(workflows);
    let token = CancellationToken::new();
    token.cancel();
    let error = cancelled
        .dispatch(
            &RuntimeEvent::AppStart {
                payload: Value::Null,
            },
            &token,
        )
        .unwrap_err();
    assert_eq!(error.code, WorkflowErrorCode::Cancelled);
    assert!(cancelled.state().get("first").is_none());
}

#[test]
fn sqlite_state_survives_session_reopen() {
    let path = unique_database_path();
    let workflows = json!([{
        "id": "persist",
        "on": { "type": "app.start" },
        "entry": "write",
        "steps": [{
            "id": "write",
            "operation": "state.set",
            "input": { "path": "/persisted", "value": 42 }
        }]
    }]);
    let app = app_with_workflows(workflows);
    {
        let store = SqliteStateStore::open(&path).unwrap();
        let mut session = AppSession::new(Runtime::new().unwrap(), app.clone(), store).unwrap();
        session
            .dispatch(
                &RuntimeEvent::AppStart {
                    payload: Value::Null,
                },
                &CancellationToken::new(),
            )
            .unwrap();
    }
    {
        let store = SqliteStateStore::open(&path).unwrap();
        let session = AppSession::new(Runtime::new().unwrap(), app, store).unwrap();
        assert_eq!(session.state()["persisted"], 42);
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn binding_validation_rejects_unknown_step_and_bad_event_path() {
    let runtime = Runtime::new().unwrap();
    let mut app: Value =
        serde_json::from_str(include_str!("../../../examples/hello.aix.json")).unwrap();
    app["workflows"] = json!([{
        "id": "badBinding",
        "on": { "type": "app.start" },
        "entry": "write",
        "steps": [{
            "id": "write",
            "operation": "state.set",
            "input": {
                "path": { "$event": "not-a-pointer" },
                "value": { "$step": "missing" }
            }
        }]
    }]);
    let error = runtime.load_json(&app.to_string()).unwrap_err();
    let codes = error
        .issues
        .iter()
        .map(|issue| issue.code.as_str())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"invalid_binding_path"));
    assert!(codes.contains(&"unknown_binding_step"));
}

#[test]
fn state_store_round_trips_json() {
    let store = SqliteStateStore::in_memory().unwrap();
    assert_eq!(store.load("app").unwrap(), None);
    store.save("app", &json!({ "value": [1, 2] })).unwrap();
    assert_eq!(store.load("app").unwrap(), Some(json!({ "value": [1, 2] })));
}

#[test]
fn timer_event_uses_camel_case_at_the_ipc_boundary() {
    let event = RuntimeEvent::Timer {
        workflow_id: "heartbeat".to_owned(),
        payload: Value::Null,
    };
    assert_eq!(
        serde_json::to_value(event).unwrap(),
        json!({ "type": "timer", "workflowId": "heartbeat", "payload": null })
    );
}

fn unique_database_path() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("aix-state-{}-{nonce}.sqlite", std::process::id()))
}
