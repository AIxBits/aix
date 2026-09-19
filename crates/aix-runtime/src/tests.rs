use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;

use aix_core::{Capability, PermissionRequest};
use serde_json::{json, Value};

use crate::{
    AppSession, CancellationToken, CapabilityResolver, ConnectorError, ErrorDefinition,
    ExecutionLimits, HostGrant, HttpRequest, HttpResponse, HttpTransport, MemoryStateStore,
    Operation, OperationContext, OperationDefinition, OperationError, OperationRegistry,
    PermissionCheck, PermissionDenied, PermissionResolver, RegisterError, Runtime, RuntimeEvent,
    SideEffect, MAX_APP_DEFINITION_BYTES,
};

struct FixtureHttp;

impl HttpTransport for FixtureHttp {
    fn execute(
        &self,
        _request: &HttpRequest,
        _permissions: &dyn PermissionResolver,
    ) -> Result<HttpResponse, ConnectorError> {
        Ok(HttpResponse {
            status: 200,
            headers: Default::default(),
            body: json!({ "ok": true }),
        })
    }
}

fn runtime() -> Runtime {
    Runtime::new().expect("built-in operation contracts should compile")
}

fn execute(id: &str, input: Value, context: &mut OperationContext) -> Value {
    runtime()
        .execute(id, &input, context)
        .unwrap_or_else(|error| panic!("{id} failed: {error:?}"))
}

#[test]
fn registers_exactly_the_beta_operations() {
    let runtime = runtime();
    let ids = runtime
        .operations()
        .definitions()
        .into_iter()
        .map(|definition| definition.id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        ids,
        BTreeSet::from([
            "ai.generate",
            "collection.filter",
            "collection.sort",
            "data.transform",
            "http.request",
            "logic.if",
            "notification.show",
            "state.get",
            "state.set",
            "time.now",
        ])
    );
}

#[test]
fn builtin_definitions_match_the_public_operation_schema() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../specs/operations/operation.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    for definition in runtime().operations().definitions() {
        let value = serde_json::to_value(definition).unwrap();
        let errors = validator
            .iter_errors(&value)
            .map(|error| error.to_string())
            .collect::<Vec<_>>();
        assert!(errors.is_empty(), "{}: {errors:?}", definition.id);
    }
}

#[test]
fn state_get_and_set_use_json_pointers() {
    let mut context = OperationContext::new(json!({
        "profile": { "name": "Ada" },
        "escaped/key": 1
    }))
    .unwrap();
    assert_eq!(
        execute(
            "state.get",
            json!({ "path": "/profile/name" }),
            &mut context
        ),
        json!({ "value": "Ada" })
    );
    assert_eq!(
        execute(
            "state.set",
            json!({ "path": "/profile/name", "value": "Grace" }),
            &mut context,
        ),
        json!({ "value": "Grace" })
    );
    assert_eq!(context.state()["profile"]["name"], "Grace");
    assert_eq!(
        execute(
            "state.get",
            json!({ "path": "/escaped~1key" }),
            &mut context
        ),
        json!({ "value": 1 })
    );
}

#[test]
fn state_errors_are_structured() {
    let runtime = runtime();
    let mut context = OperationContext::new(json!({ "items": [] })).unwrap();
    let missing = runtime
        .execute("state.get", &json!({ "path": "/missing" }), &mut context)
        .unwrap_err();
    assert_eq!(missing.code, "path_not_found");
    let bad_index = runtime
        .execute(
            "state.set",
            &json!({ "path": "/items/nope", "value": 1 }),
            &mut context,
        )
        .unwrap_err();
    assert_eq!(bad_index.code, "invalid_path");
}

struct DenyLocalEffects;

impl PermissionResolver for DenyLocalEffects {
    fn authorize(&self, check: &PermissionCheck) -> Result<(), PermissionDenied> {
        Err(PermissionDenied {
            capability: check.capability.clone(),
            code: "permission_denied".to_owned(),
            message: "external denied".to_owned(),
        })
    }

    fn authorize_local_effect(&self, _operation_id: &str, _effect: &str) -> bool {
        false
    }
}

#[test]
fn app_local_side_effects_also_cross_the_resolver() {
    let runtime = runtime();
    let mut context = OperationContext::new(json!({ "value": 1 }))
        .unwrap()
        .with_permissions(Arc::new(DenyLocalEffects));
    let error = runtime
        .execute(
            "state.set",
            &json!({ "path": "/value", "value": 2 }),
            &mut context,
        )
        .unwrap_err();
    assert_eq!(error.code, "permission_denied");
    assert_eq!(context.state()["value"], 1);
}

#[test]
fn validates_operation_input_before_execution() {
    let runtime = runtime();
    let mut context = OperationContext::empty();
    let error = runtime
        .execute(
            "logic.if",
            &json!({ "condition": "yes", "then": 1, "else": 2 }),
            &mut context,
        )
        .unwrap_err();
    assert_eq!(error.code, "invalid_input");
    assert!(error.details.is_some());

    let unknown = runtime
        .execute("shell.exec", &json!({}), &mut context)
        .unwrap_err();
    assert_eq!(unknown.code, "unknown_operation");
}

#[test]
fn logic_if_selects_data_without_evaluation() {
    let mut context = OperationContext::empty();
    assert_eq!(
        execute(
            "logic.if",
            json!({ "condition": false, "then": { "unsafe": "ignored" }, "else": [1, 2] }),
            &mut context,
        ),
        json!({ "value": [1, 2] })
    );
}

#[test]
fn filters_and_sorts_collections_declaratively() {
    let mut context = OperationContext::empty();
    let items = json!([
        { "name": "rain", "temperature": 11 },
        { "name": "sun", "temperature": 25 },
        { "name": "cloud", "temperature": 18 }
    ]);
    let filtered = execute(
        "collection.filter",
        json!({
            "items": items,
            "predicate": { "path": "/temperature", "operator": "gte", "value": 18 }
        }),
        &mut context,
    );
    assert_eq!(filtered["items"].as_array().unwrap().len(), 2);
    let sorted = execute(
        "collection.sort",
        json!({ "items": filtered["items"], "path": "/temperature", "order": "desc" }),
        &mut context,
    );
    assert_eq!(sorted["items"][0]["name"], "sun");
    assert_eq!(sorted["items"][1]["name"], "cloud");
}

#[test]
fn sort_rejects_incompatible_keys() {
    let runtime = runtime();
    let mut context = OperationContext::empty();
    let error = runtime
        .execute(
            "collection.sort",
            &json!({ "items": [{"key": 1}, {"key": "two"}], "path": "/key" }),
            &mut context,
        )
        .unwrap_err();
    assert_eq!(error.code, "unsupported_sort_value");
}

#[test]
fn transforms_data_with_bounded_pointer_mapping() {
    let mut context = OperationContext::empty();
    let output = execute(
        "data.transform",
        json!({
            "value": { "location": { "city": "Shanghai" }, "temperature": 26 },
            "mapping": { "city": "/location/city", "degrees": "/temperature" }
        }),
        &mut context,
    );
    assert_eq!(
        output,
        json!({ "value": { "city": "Shanghai", "degrees": 26 } })
    );
}

#[test]
fn host_operations_require_grants_before_reaching_adapters() {
    let runtime = runtime();
    let mut context = OperationContext::empty();
    for (id, input) in [
        ("http.request", json!({ "url": "https://example.com" })),
        ("notification.show", json!({ "message": "hello" })),
        ("ai.generate", json!({ "prompt": "hello" })),
    ] {
        let error = runtime.execute(id, &input, &mut context).unwrap_err();
        assert_eq!(error.code, "permission_denied");
    }

    let requests = vec![
        PermissionRequest {
            capability: Capability::NetworkRequest,
            scopes: vec!["https://example.com".into()],
        },
        PermissionRequest {
            capability: Capability::NotificationShow,
            scopes: vec!["default".into()],
        },
        PermissionRequest {
            capability: Capability::AiGenerate,
            scopes: vec!["default".into()],
        },
    ];
    let grants = requests
        .iter()
        .map(|request| {
            HostGrant::new(
                "test.app",
                request.capability.clone(),
                request.scopes.clone(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let resolver = CapabilityResolver::new("test.app", &requests, &grants).unwrap();
    let mut context = OperationContext::empty()
        .with_permissions(Arc::new(resolver))
        .with_http_transport(Arc::new(FixtureHttp));
    let response = runtime
        .execute(
            "http.request",
            &json!({ "url": "https://example.com/weather" }),
            &mut context,
        )
        .unwrap();
    assert_eq!(response["status"], 200);
    assert_eq!(response["body"]["ok"], true);
    for (id, input) in [
        ("notification.show", json!({ "message": "hello" })),
        ("ai.generate", json!({ "prompt": "hello" })),
    ] {
        let error = runtime.execute(id, &input, &mut context).unwrap_err();
        assert_eq!(error.code, "adapter_unavailable");
    }
    let http = runtime.operations().definition("http.request").unwrap();
    assert_eq!(http.required_permissions, vec![Capability::NetworkRequest]);
    assert_eq!(http.side_effects, vec![SideEffect::NetworkRequest]);
    let ai = runtime.operations().definition("ai.generate").unwrap();
    assert_eq!(ai.required_permissions, vec![Capability::AiGenerate]);
}

#[test]
fn time_now_returns_a_checked_timestamp() {
    let mut context = OperationContext::empty();
    let output = execute("time.now", json!({}), &mut context);
    assert!(output["unixMs"].as_u64().unwrap() > 1_600_000_000_000);
}

struct TestOperation {
    definition: OperationDefinition,
    result: Result<Value, OperationError>,
}

struct MutatingInvalidOutput {
    definition: OperationDefinition,
}

impl Operation for MutatingInvalidOutput {
    fn definition(&self) -> &OperationDefinition {
        &self.definition
    }

    fn execute(
        &self,
        _input: &Value,
        context: &mut OperationContext,
    ) -> Result<Value, OperationError> {
        context.state_mut()["changed"] = json!(true);
        Ok(json!(42))
    }
}

impl TestOperation {
    fn new(id: &str, output_schema: Value, result: Result<Value, OperationError>) -> Self {
        Self {
            definition: OperationDefinition {
                id: id.to_owned(),
                input: json!({ "type": "object" }),
                output: output_schema,
                errors: vec![ErrorDefinition {
                    code: "declared".to_owned(),
                    description: "test error".to_owned(),
                }],
                side_effects: vec![],
                required_permissions: vec![],
            },
            result,
        }
    }
}

impl Operation for TestOperation {
    fn definition(&self) -> &OperationDefinition {
        &self.definition
    }

    fn execute(
        &self,
        _input: &Value,
        _context: &mut OperationContext,
    ) -> Result<Value, OperationError> {
        self.result.clone()
    }
}

#[test]
fn rejects_invalid_outputs_and_undeclared_errors() {
    let mut registry = OperationRegistry::new();
    registry
        .register(TestOperation::new(
            "test.output",
            json!({ "type": "string" }),
            Ok(json!(42)),
        ))
        .unwrap();
    registry
        .register(TestOperation::new(
            "test.error",
            json!(true),
            Err(OperationError::new("test.error", "surprise", "undeclared")),
        ))
        .unwrap();
    let mut context = OperationContext::empty();
    assert_eq!(
        registry
            .execute("test.output", &json!({}), &mut context)
            .unwrap_err()
            .code,
        "invalid_output"
    );
    assert_eq!(
        registry
            .execute("test.error", &json!({}), &mut context)
            .unwrap_err()
            .code,
        "operation_contract_violation"
    );
}

#[test]
fn registry_rejects_duplicate_ids_and_invalid_schemas() {
    let mut registry = OperationRegistry::new();
    registry
        .register(TestOperation::new("test.valid", json!(true), Ok(json!(1))))
        .unwrap();
    assert!(matches!(
        registry.register(TestOperation::new("test.valid", json!(true), Ok(json!(1)))),
        Err(RegisterError::DuplicateId(id)) if id == "test.valid"
    ));

    let invalid_schema = TestOperation::new(
        "test.schema",
        json!({ "type": "not-a-json-type" }),
        Ok(json!(1)),
    );
    assert!(matches!(
        registry.register(invalid_schema),
        Err(RegisterError::InvalidOutputSchema { operation, .. }) if operation == "test.schema"
    ));

    let mut unguarded = TestOperation::new("test.unguarded", json!(true), Ok(json!(1)));
    unguarded.definition.side_effects = vec![SideEffect::NetworkRequest];
    assert!(matches!(
        registry.register(unguarded),
        Err(RegisterError::MissingPermissionForSideEffect {
            operation,
            side_effect: SideEffect::NetworkRequest
        }) if operation == "test.unguarded"
    ));
}

#[test]
fn invalid_result_rolls_back_app_local_state() {
    let mut registry = OperationRegistry::new();
    registry
        .register(MutatingInvalidOutput {
            definition: OperationDefinition {
                id: "test.rollback".to_owned(),
                input: json!({ "type": "object" }),
                output: json!({ "type": "string" }),
                errors: vec![],
                side_effects: vec![SideEffect::StateWrite],
                required_permissions: vec![],
            },
        })
        .unwrap();
    let mut context = OperationContext::new(json!({ "changed": false })).unwrap();
    assert_eq!(
        registry
            .execute("test.rollback", &json!({}), &mut context)
            .unwrap_err()
            .code,
        "invalid_output"
    );
    assert_eq!(context.state()["changed"], false);
}

#[test]
fn rust_boundary_validates_the_example() {
    runtime()
        .load_json(include_str!("../../../examples/hello.aix.json"))
        .unwrap();
}

#[test]
fn rust_boundary_rejects_unknown_fields_and_oversized_input() {
    let runtime = runtime();
    let source = include_str!("../../../examples/hello.aix.json").replace(
        "\"specVersion\":",
        "\"script\": \"unsafe\", \"specVersion\":",
    );
    assert_eq!(
        runtime.load_json(&source).unwrap_err().issues[0].code,
        "invalid_structure"
    );
    let oversized = " ".repeat(MAX_APP_DEFINITION_BYTES + 1);
    assert_eq!(
        runtime.load_json(&oversized).unwrap_err().issues[0].code,
        "definition_too_large"
    );
}

#[test]
fn rust_boundary_enforces_the_normative_json_schema() {
    let runtime = runtime();
    let mut app = example_value();
    app["metadata"]["description"] = Value::Null;
    let error = runtime.load_json(&app.to_string()).unwrap_err();
    assert!(error
        .issues
        .iter()
        .any(|issue| issue.code == "invalid_structure"));
}

fn example_value() -> Value {
    serde_json::from_str(include_str!("../../../examples/hello.aix.json")).unwrap()
}

#[test]
fn validates_workflow_graph_and_registered_operations() {
    let runtime = runtime();
    let mut app = example_value();
    app["workflows"] = json!([{
        "id": "start",
        "on": { "type": "app.start" },
        "entry": "choose",
        "steps": [{
            "id": "choose",
            "operation": "logic.if",
            "input": { "condition": true, "then": 1, "else": 2 },
            "next": []
        }]
    }]);
    runtime.load_json(&app.to_string()).unwrap();

    app["workflows"][0]["steps"][0]["operation"] = json!("shell.exec");
    let error = runtime.load_json(&app.to_string()).unwrap_err();
    assert!(error
        .issues
        .iter()
        .any(|issue| issue.code == "unknown_operation"));
}

#[test]
fn rust_boundary_validates_static_workflow_inputs() {
    let runtime = runtime();
    let mut app = example_value();
    app["workflows"] = json!([{
        "id": "invalid",
        "on": { "type": "app.start" },
        "entry": "choose",
        "steps": [{
            "id": "choose",
            "operation": "logic.if",
            "input": { "condition": "yes", "then": 1, "else": 2 }
        }]
    }]);
    let error = runtime.load_json(&app.to_string()).unwrap_err();
    assert!(error
        .issues
        .iter()
        .any(|issue| issue.code == "invalid_operation_input"));
}

#[test]
fn requires_declared_capability_for_effectful_operation() {
    let runtime = runtime();
    let mut app = example_value();
    app["workflows"] = json!([{
        "id": "fetch",
        "on": { "type": "app.start" },
        "entry": "request",
        "steps": [{
            "id": "request",
            "operation": "http.request",
            "input": { "url": "https://example.com" }
        }]
    }]);
    let error = runtime.load_json(&app.to_string()).unwrap_err();
    assert!(error
        .issues
        .iter()
        .any(|issue| issue.code == "missing_permission_request"));

    app["permissions"] = json!([{
        "capability": "network.request",
        "scopes": ["https://example.com"]
    }]);
    runtime.load_json(&app.to_string()).unwrap();
}

#[test]
fn connector_declarations_become_executable_operations() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let size = stream.read(&mut request).unwrap();
        let request = String::from_utf8_lossy(&request[..size]);
        assert!(request.starts_with("GET /api/weather/Paris?units=metric "));
        let body = r#"{"city":"Paris","temperature":22}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let base = format!("http://{address}/api");
    let mut value = example_value();
    value["connectors"] = json!([{
        "id": "weather",
        "type": "http",
        "baseUrl": base,
        "operations": [{ "id": "weather.get", "method": "GET", "path": "/weather/{city}" }]
    }]);
    value["permissions"] = json!([{
        "capability": "network.request",
        "scopes": [format!("http://{address}/api")]
    }]);
    value["workflows"] = json!([{
        "id": "fetch",
        "on": { "type": "app.start" },
        "entry": "request",
        "steps": [{
            "id": "request",
            "operation": "weather.get",
            "input": {
                "pathParams": { "city": "Paris" },
                "query": { "units": "metric" }
            }
        }]
    }]);
    let runtime = runtime();
    let app = runtime.load_json(&value.to_string()).unwrap();
    let grant = HostGrant::new(
        &app.metadata.id,
        Capability::NetworkRequest,
        vec![format!("http://{address}/api")],
    )
    .unwrap();
    let mut session = AppSession::with_grants(
        runtime,
        app,
        MemoryStateStore::new(),
        ExecutionLimits::default(),
        &[grant],
    )
    .unwrap();
    let result = session
        .dispatch(
            &RuntimeEvent::AppStart {
                payload: Value::Null,
            },
            &CancellationToken::new(),
        )
        .unwrap();
    assert_eq!(result.workflows[0].steps[0].output["body"]["city"], "Paris");
    server.join().unwrap();
}

#[test]
fn rejects_workflow_cycles() {
    let runtime = runtime();
    let mut app = example_value();
    app["workflows"] = json!([{
        "id": "cycle",
        "on": { "type": "app.start" },
        "entry": "again",
        "steps": [{
            "id": "again",
            "operation": "time.now",
            "input": {},
            "next": ["again"]
        }]
    }]);
    let error = runtime.load_json(&app.to_string()).unwrap_err();
    assert!(error
        .issues
        .iter()
        .any(|issue| issue.code == "workflow_cycle"));
}
