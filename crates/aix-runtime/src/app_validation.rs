//! Authoritative Rust-side application validation.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use aix_core::{AppDefinition, Capability, EventKind, UiNode, Workflow, SPEC_VERSION};
use jsonschema::Validator;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::OperationRegistry;

/// Maximum accepted JSON definition size at the Rust trust boundary.
pub const MAX_APP_DEFINITION_BYTES: usize = 1_048_576;

/// One machine-readable problem found in an app definition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidationIssue {
    pub path: String,
    pub code: String,
    pub message: String,
}

/// All structural or semantic failures returned for an app definition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppValidationError {
    pub issues: Vec<ValidationIssue>,
}

impl std::fmt::Display for AppValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, issue) in self.issues.iter().enumerate() {
            if index > 0 {
                writeln!(formatter)?;
            }
            write!(
                formatter,
                "{} [{}] {}",
                issue.path, issue.code, issue.message
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for AppValidationError {}

/// Parse bounded JSON and apply authoritative semantic validation.
pub fn validate_app_json(
    source: &str,
    operations: &OperationRegistry,
) -> Result<AppDefinition, AppValidationError> {
    if source.len() > MAX_APP_DEFINITION_BYTES {
        return Err(single_issue(
            "/",
            "definition_too_large",
            "app definition exceeds 1 MiB",
        ));
    }
    let value = serde_json::from_str::<Value>(source).map_err(|error| {
        single_issue(
            "/",
            "invalid_structure",
            format!("app definition is not valid JSON: {error}"),
        )
    })?;
    let schema_issues = app_schema_validator()
        .iter_errors(&value)
        .map(|error| {
            let path = error.instance_path().to_string();
            ValidationIssue {
                path: if path.is_empty() {
                    "/".to_owned()
                } else {
                    path
                },
                code: "invalid_structure".to_owned(),
                message: error.to_string(),
            }
        })
        .collect::<Vec<_>>();
    if !schema_issues.is_empty() {
        return Err(AppValidationError {
            issues: schema_issues,
        });
    }
    let app = serde_json::from_value::<AppDefinition>(value).map_err(|error| {
        single_issue(
            "/",
            "invalid_structure",
            format!("app definition could not be decoded: {error}"),
        )
    })?;
    validate_app(&app, operations)?;
    Ok(app)
}

fn app_schema_validator() -> &'static Validator {
    static VALIDATOR: OnceLock<Validator> = OnceLock::new();
    VALIDATOR.get_or_init(|| {
        let schema =
            serde_json::from_str(include_str!("../../../packages/aix-schema/app.schema.json"))
                .expect("embedded App Schema must be valid JSON");
        jsonschema::validator_for(&schema).expect("embedded App Schema must compile")
    })
}

/// Validate semantic invariants that serde cannot express.
pub fn validate_app(
    app: &AppDefinition,
    operations: &OperationRegistry,
) -> Result<(), AppValidationError> {
    let mut issues = Vec::new();
    if app.spec_version != SPEC_VERSION {
        push_issue(
            &mut issues,
            "/specVersion",
            "unsupported_spec_version",
            format!(
                "runtime supports `{SPEC_VERSION}`, received `{}`",
                app.spec_version
            ),
        );
    }
    validate_required_text(&app.metadata.id, "/metadata/id", &mut issues);
    validate_required_text(&app.metadata.name, "/metadata/name", &mut issues);
    validate_required_text(&app.metadata.version, "/metadata/version", &mut issues);
    validate_id(&app.metadata.id, "/metadata/id", &mut issues);

    for resource_id in app.resources.keys() {
        validate_id(
            resource_id,
            &format!("/resources/{resource_id}"),
            &mut issues,
        );
    }

    let mut ui_ids = BTreeSet::new();
    validate_ui_node(&app.ui, "/ui", &mut ui_ids, &mut issues);

    let requested_capabilities = app
        .permissions
        .iter()
        .map(|permission| permission.capability.clone())
        .collect::<BTreeSet<_>>();
    for (index, permission) in app.permissions.iter().enumerate() {
        if permission.scopes.is_empty() {
            push_issue(
                &mut issues,
                &format!("/permissions/{index}/scopes"),
                "empty_permission_scope",
                "permission request must include at least one scope",
            );
        }
        if permission.scopes.iter().any(|scope| scope.is_empty()) {
            push_issue(
                &mut issues,
                &format!("/permissions/{index}/scopes"),
                "empty_permission_scope",
                "permission scopes cannot be empty strings",
            );
        }
    }

    let mut workflow_ids = BTreeSet::new();
    for (index, workflow) in app.workflows.iter().enumerate() {
        let path = format!("/workflows/{index}");
        validate_id(&workflow.id, &format!("{path}/id"), &mut issues);
        if !workflow_ids.insert(workflow.id.as_str()) {
            push_issue(
                &mut issues,
                &format!("{path}/id"),
                "duplicate_workflow_id",
                format!("workflow id `{}` is duplicated", workflow.id),
            );
        }
        validate_event(workflow, &path, &ui_ids, &mut issues);
        validate_workflow(
            workflow,
            &path,
            operations,
            &requested_capabilities,
            &mut issues,
        );
    }

    let mut connector_ids = BTreeSet::new();
    let mut connector_operation_ids = BTreeSet::new();
    for (connector_index, connector) in app.connectors.iter().enumerate() {
        let path = format!("/connectors/{connector_index}");
        validate_id(&connector.id, &format!("{path}/id"), &mut issues);
        if !connector_ids.insert(connector.id.as_str()) {
            push_issue(
                &mut issues,
                &format!("{path}/id"),
                "duplicate_connector_id",
                format!("connector id `{}` is duplicated", connector.id),
            );
        }
        if !(connector.base_url.starts_with("https://")
            || connector.base_url.starts_with("http://"))
        {
            push_issue(
                &mut issues,
                &format!("{path}/baseUrl"),
                "invalid_base_url",
                "connector baseUrl must use HTTP or HTTPS",
            );
        }
        for (operation_index, operation) in connector.operations.iter().enumerate() {
            let operation_path = format!("{path}/operations/{operation_index}");
            validate_id(&operation.id, &format!("{operation_path}/id"), &mut issues);
            if !connector_operation_ids.insert(operation.id.as_str()) {
                push_issue(
                    &mut issues,
                    &format!("{operation_path}/id"),
                    "duplicate_connector_operation_id",
                    format!("connector operation id `{}` is duplicated", operation.id),
                );
            }
            if !operation.path.starts_with('/') {
                push_issue(
                    &mut issues,
                    &format!("{operation_path}/path"),
                    "invalid_connector_path",
                    "connector operation path must start with `/`",
                );
            }
        }
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(AppValidationError { issues })
    }
}

fn validate_ui_node(
    node: &UiNode,
    path: &str,
    ids: &mut BTreeSet<String>,
    issues: &mut Vec<ValidationIssue>,
) {
    validate_id(&node.id, &format!("{path}/id"), issues);
    if !ids.insert(node.id.clone()) {
        push_issue(
            issues,
            &format!("{path}/id"),
            "duplicate_ui_id",
            format!("UI id `{}` is duplicated", node.id),
        );
    }
    for (index, child) in node.children.iter().enumerate() {
        validate_ui_node(child, &format!("{path}/children/{index}"), ids, issues);
    }
}

fn validate_event(
    workflow: &Workflow,
    path: &str,
    ui_ids: &BTreeSet<String>,
    issues: &mut Vec<ValidationIssue>,
) {
    match workflow.on.kind {
        EventKind::UiClick | EventKind::UiChange => match workflow.on.target.as_deref() {
            Some(target) if ui_ids.contains(target) => {}
            Some(target) => push_issue(
                issues,
                &format!("{path}/on/target"),
                "unknown_event_target",
                format!("UI target `{target}` does not exist"),
            ),
            None => push_issue(
                issues,
                &format!("{path}/on/target"),
                "missing_event_target",
                "UI events require a target",
            ),
        },
        EventKind::Timer => {
            if workflow
                .on
                .interval_ms
                .is_none_or(|interval| interval < 100)
            {
                push_issue(
                    issues,
                    &format!("{path}/on/intervalMs"),
                    "invalid_timer_interval",
                    "timer intervalMs must be at least 100",
                );
            }
        }
        EventKind::AppStart => {}
    }
}

fn validate_workflow(
    workflow: &Workflow,
    path: &str,
    operations: &OperationRegistry,
    requested_capabilities: &BTreeSet<Capability>,
    issues: &mut Vec<ValidationIssue>,
) {
    validate_id(&workflow.id, &format!("{path}/id"), issues);
    if workflow.steps.is_empty() {
        push_issue(
            issues,
            &format!("{path}/steps"),
            "empty_workflow",
            "workflow must contain at least one step",
        );
        return;
    }
    let mut steps = BTreeMap::new();
    for (index, step) in workflow.steps.iter().enumerate() {
        let step_path = format!("{path}/steps/{index}");
        validate_id(&step.id, &format!("{step_path}/id"), issues);
        if steps.insert(step.id.as_str(), step).is_some() {
            push_issue(
                issues,
                &format!("{step_path}/id"),
                "duplicate_step_id",
                format!("step id `{}` is duplicated", step.id),
            );
        }
        if !step.input.is_object() {
            push_issue(
                issues,
                &format!("{step_path}/input"),
                "invalid_operation_input",
                "workflow step input must be a JSON object",
            );
        }
        match operations.definition(&step.operation) {
            None => push_issue(
                issues,
                &format!("{step_path}/operation"),
                "unknown_operation",
                format!("operation `{}` is not registered", step.operation),
            ),
            Some(definition) => {
                if let Err(error) = operations.validate_input(&step.operation, &step.input) {
                    push_issue(
                        issues,
                        &format!("{step_path}/input"),
                        "invalid_operation_input",
                        error.message,
                    );
                }
                for capability in &definition.required_permissions {
                    if !requested_capabilities.contains(capability) {
                        push_issue(
                            issues,
                            &format!("{step_path}/operation"),
                            "missing_permission_request",
                            format!(
                                "operation `{}` requires capability `{}`",
                                step.operation,
                                capability_name(capability)
                            ),
                        );
                    }
                }
            }
        }
    }
    if !steps.contains_key(workflow.entry.as_str()) {
        push_issue(
            issues,
            &format!("{path}/entry"),
            "unknown_workflow_entry",
            format!("entry step `{}` does not exist", workflow.entry),
        );
    }
    for (index, step) in workflow.steps.iter().enumerate() {
        for next in &step.next {
            if !steps.contains_key(next.as_str()) {
                push_issue(
                    issues,
                    &format!("{path}/steps/{index}/next"),
                    "unknown_step_edge",
                    format!("next step `{next}` does not exist"),
                );
            }
        }
    }
    detect_cycles(workflow, path, &steps, issues);
}

fn detect_cycles(
    workflow: &Workflow,
    path: &str,
    steps: &BTreeMap<&str, &aix_core::WorkflowStep>,
    issues: &mut Vec<ValidationIssue>,
) {
    fn visit<'a>(
        id: &'a str,
        steps: &BTreeMap<&'a str, &'a aix_core::WorkflowStep>,
        states: &mut BTreeMap<&'a str, u8>,
    ) -> Option<&'a str> {
        match states.get(id) {
            Some(1) => return Some(id),
            Some(2) => return None,
            _ => {}
        }
        let step = steps.get(id)?;
        states.insert(id, 1);
        for next in &step.next {
            if let Some(cycle) = visit(next, steps, states) {
                return Some(cycle);
            }
        }
        states.insert(id, 2);
        None
    }

    let mut states = BTreeMap::new();
    for step in &workflow.steps {
        if let Some(cycle) = visit(&step.id, steps, &mut states) {
            push_issue(
                issues,
                &format!("{path}/steps"),
                "workflow_cycle",
                format!("workflow contains a cycle at step `{cycle}`"),
            );
            break;
        }
    }
}

fn validate_id(value: &str, path: &str, issues: &mut Vec<ValidationIssue>) {
    let mut characters = value.chars();
    let valid = characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic())
        && characters.all(|character| {
            character.is_ascii_alphanumeric()
                || character == '_'
                || character == '-'
                || character == '.'
        });
    if !valid {
        push_issue(
            issues,
            path,
            "invalid_id",
            format!("`{value}` is not a valid AIX identifier"),
        );
    }
}

fn validate_required_text(value: &str, path: &str, issues: &mut Vec<ValidationIssue>) {
    if value.is_empty() {
        push_issue(issues, path, "empty_value", "value cannot be empty");
    }
}

fn capability_name(capability: &Capability) -> &'static str {
    match capability {
        Capability::AiGenerate => "ai.generate",
        Capability::NetworkRequest => "network.request",
        Capability::FileRead => "file.read",
        Capability::FileWrite => "file.write",
        Capability::NotificationShow => "notification.show",
    }
}

fn single_issue(
    path: impl Into<String>,
    code: impl Into<String>,
    message: impl Into<String>,
) -> AppValidationError {
    AppValidationError {
        issues: vec![ValidationIssue {
            path: path.into(),
            code: code.into(),
            message: message.into(),
        }],
    }
}

fn push_issue(
    issues: &mut Vec<ValidationIssue>,
    path: &str,
    code: &str,
    message: impl Into<String>,
) {
    issues.push(ValidationIssue {
        path: path.to_owned(),
        code: code.to_owned(),
        message: message.into(),
    });
}
