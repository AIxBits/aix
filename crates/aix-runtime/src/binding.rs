//! Safe recursive resolution of declarative workflow bindings.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Maximum nesting accepted while inspecting or resolving a template.
pub const MAX_BINDING_DEPTH: usize = 64;

/// Data sources visible to one workflow step.
pub struct BindingSources<'a> {
    /// Current app-local state, including writes from earlier steps.
    pub state: &'a Value,
    /// Payload supplied with the triggering event.
    pub event: &'a Value,
    /// Successful outputs from earlier steps in the same workflow.
    pub steps: &'a BTreeMap<String, Value>,
}

/// Structured binding failure suitable for workflow diagnostics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BindingError {
    /// JSON Pointer locating the binding inside the input template.
    pub path: String,
    /// Stable failure category.
    pub code: String,
    /// Human-readable diagnostic.
    pub message: String,
}

impl std::fmt::Display for BindingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{} [{}] {}", self.path, self.code, self.message)
    }
}

impl std::error::Error for BindingError {}

/// Return whether a JSON template contains an AIX binding marker.
pub fn contains_bindings(value: &Value) -> bool {
    match value {
        Value::Array(values) => values.iter().any(contains_bindings),
        Value::Object(object) => {
            object.keys().any(|key| key.starts_with('$')) || object.values().any(contains_bindings)
        }
        _ => false,
    }
}

/// Validate binding shapes and referenced step IDs without resolving values.
pub fn validate_binding_template(template: &Value, step_ids: &BTreeSet<&str>) -> Vec<BindingError> {
    let mut errors = Vec::new();
    inspect_template(template, "", 0, step_ids, &mut errors);
    errors
}

/// Resolve all bindings into ordinary JSON before operation validation.
pub fn resolve_bindings(
    template: &Value,
    sources: &BindingSources<'_>,
) -> Result<Value, BindingError> {
    resolve_value(template, "", 0, sources)
}

fn inspect_template(
    value: &Value,
    path: &str,
    depth: usize,
    step_ids: &BTreeSet<&str>,
    errors: &mut Vec<BindingError>,
) {
    if depth > MAX_BINDING_DEPTH {
        errors.push(error(
            path,
            "binding_depth_exceeded",
            format!("binding template exceeds {MAX_BINDING_DEPTH} levels"),
        ));
        return;
    }
    match value {
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                inspect_template(
                    value,
                    &join_pointer(path, &index.to_string()),
                    depth + 1,
                    step_ids,
                    errors,
                );
            }
        }
        Value::Object(object) => {
            let special = object
                .keys()
                .filter(|key| key.starts_with('$'))
                .collect::<Vec<_>>();
            if special.is_empty() {
                for (key, value) in object {
                    inspect_template(value, &join_pointer(path, key), depth + 1, step_ids, errors);
                }
                return;
            }
            match binding_kind(object) {
                Ok(BindingKind::State(pointer)) => {
                    if !valid_state_path(pointer) {
                        errors.push(error(
                            path,
                            "invalid_binding_path",
                            "binding path must be empty, a JSON Pointer, or a top-level state key",
                        ));
                    }
                }
                Ok(BindingKind::Event(pointer)) => {
                    if !valid_pointer(pointer) {
                        errors.push(error(
                            path,
                            "invalid_binding_path",
                            "event binding path must be empty or a JSON Pointer",
                        ));
                    }
                }
                Ok(BindingKind::Step {
                    step,
                    path: pointer,
                }) => {
                    if !step_ids.contains(step) {
                        errors.push(error(
                            path,
                            "unknown_binding_step",
                            format!("binding references unknown step `{step}`"),
                        ));
                    }
                    if !valid_pointer(pointer) {
                        errors.push(error(
                            path,
                            "invalid_binding_path",
                            "step binding path must be empty or a JSON Pointer",
                        ));
                    }
                }
                Err(message) => errors.push(error(path, "invalid_binding", message)),
            }
        }
        _ => {}
    }
}

fn resolve_value(
    value: &Value,
    path: &str,
    depth: usize,
    sources: &BindingSources<'_>,
) -> Result<Value, BindingError> {
    if depth > MAX_BINDING_DEPTH {
        return Err(error(
            path,
            "binding_depth_exceeded",
            format!("binding template exceeds {MAX_BINDING_DEPTH} levels"),
        ));
    }
    match value {
        Value::Array(values) => values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                resolve_value(
                    value,
                    &join_pointer(path, &index.to_string()),
                    depth + 1,
                    sources,
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(object) => {
            let special = object.keys().any(|key| key.starts_with('$'));
            if special {
                return match binding_kind(object)
                    .map_err(|message| error(path, "invalid_binding", message))?
                {
                    BindingKind::State(pointer) => resolve_state_path(sources.state, pointer, path),
                    BindingKind::Event(pointer) => resolve_pointer(
                        sources.event,
                        pointer,
                        path,
                        "event_binding_not_found",
                        "event payload",
                    ),
                    BindingKind::Step {
                        step,
                        path: pointer,
                    } => {
                        let output = sources.steps.get(step).ok_or_else(|| {
                            error(
                                path,
                                "step_binding_not_ready",
                                format!("step `{step}` has no completed output"),
                            )
                        })?;
                        resolve_pointer(
                            output,
                            pointer,
                            path,
                            "step_binding_not_found",
                            &format!("step `{step}` output"),
                        )
                    }
                };
            }
            let mut resolved = Map::new();
            for (key, value) in object {
                resolved.insert(
                    key.clone(),
                    resolve_value(value, &join_pointer(path, key), depth + 1, sources)?,
                );
            }
            Ok(Value::Object(resolved))
        }
        _ => Ok(value.clone()),
    }
}

enum BindingKind<'a> {
    State(&'a str),
    Event(&'a str),
    Step { step: &'a str, path: &'a str },
}

fn binding_kind(object: &Map<String, Value>) -> Result<BindingKind<'_>, String> {
    if object.len() == 1 {
        if let Some(pointer) = object.get("$state").and_then(Value::as_str) {
            return Ok(BindingKind::State(pointer));
        }
        if let Some(pointer) = object.get("$event").and_then(Value::as_str) {
            return Ok(BindingKind::Event(pointer));
        }
    }
    if object.contains_key("$step") && object.keys().all(|key| key == "$step" || key == "path") {
        let step = object
            .get("$step")
            .and_then(Value::as_str)
            .ok_or_else(|| "`$step` must be a string step ID".to_owned())?;
        let path = match object.get("path") {
            Some(path) => path
                .as_str()
                .ok_or_else(|| "step binding `path` must be a string".to_owned())?,
            None => "",
        };
        return Ok(BindingKind::Step { step, path });
    }
    Err(
        "binding object must be exactly `$state`, `$event`, or `$step` with optional `path`"
            .to_owned(),
    )
}

fn resolve_pointer(
    source: &Value,
    pointer: &str,
    template_path: &str,
    code: &str,
    source_name: &str,
) -> Result<Value, BindingError> {
    let value = if pointer.is_empty() {
        Some(source)
    } else if pointer.starts_with('/') {
        source.pointer(pointer)
    } else {
        None
    };
    value.cloned().ok_or_else(|| {
        error(
            template_path,
            code,
            format!("{source_name} path `{pointer}` does not exist"),
        )
    })
}

fn resolve_state_path(
    state: &Value,
    pointer: &str,
    template_path: &str,
) -> Result<Value, BindingError> {
    if pointer.is_empty() || pointer.starts_with('/') {
        return resolve_pointer(
            state,
            pointer,
            template_path,
            "state_binding_not_found",
            "state",
        );
    }
    state
        .as_object()
        .and_then(|object| object.get(pointer))
        .cloned()
        .ok_or_else(|| {
            error(
                template_path,
                "state_binding_not_found",
                format!("state key `{pointer}` does not exist"),
            )
        })
}

fn valid_state_path(pointer: &str) -> bool {
    valid_pointer(pointer) || (!pointer.is_empty() && !pointer.starts_with('/'))
}

fn valid_pointer(pointer: &str) -> bool {
    if pointer.is_empty() {
        return true;
    }
    if !pointer.starts_with('/') {
        return false;
    }
    let bytes = pointer.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'~' {
            index += 1;
            if index >= bytes.len() || !matches!(bytes[index], b'0' | b'1') {
                return false;
            }
        }
        index += 1;
    }
    true
}

fn join_pointer(parent: &str, token: &str) -> String {
    format!("{parent}/{}", token.replace('~', "~0").replace('/', "~1"))
}

fn error(path: &str, code: &str, message: impl Into<String>) -> BindingError {
    BindingError {
        path: if path.is_empty() {
            "/".to_owned()
        } else {
            path.to_owned()
        },
        code: code.to_owned(),
        message: message.into(),
    }
}
