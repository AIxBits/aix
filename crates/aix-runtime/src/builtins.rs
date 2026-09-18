//! Built-in atomic operations for the Beta runtime.

use std::cmp::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

use aix_core::Capability;
use serde_json::{json, Map, Value};

use crate::operation::{
    ErrorDefinition, Operation, OperationContext, OperationDefinition, OperationError,
    OperationRegistry, RegisterError, SideEffect,
};

const JSON_SCHEMA: &str = "https://json-schema.org/draft/2020-12/schema";

#[derive(Clone, Copy)]
enum BuiltinKind {
    StateGet,
    StateSet,
    LogicIf,
    CollectionFilter,
    CollectionSort,
    DataTransform,
    HttpRequest,
    TimeNow,
    NotificationShow,
    AiGenerate,
}

struct BuiltinOperation {
    definition: OperationDefinition,
    kind: BuiltinKind,
}

impl Operation for BuiltinOperation {
    fn definition(&self) -> &OperationDefinition {
        &self.definition
    }

    fn execute(
        &self,
        input: &Value,
        context: &mut OperationContext,
    ) -> Result<Value, OperationError> {
        match self.kind {
            BuiltinKind::StateGet => state_get(input, context),
            BuiltinKind::StateSet => state_set(input, context),
            BuiltinKind::LogicIf => Ok(json!({
                "value": if input["condition"].as_bool().unwrap_or(false) {
                    input["then"].clone()
                } else {
                    input["else"].clone()
                }
            })),
            BuiltinKind::CollectionFilter => collection_filter(input),
            BuiltinKind::CollectionSort => collection_sort(input),
            BuiltinKind::DataTransform => data_transform(input),
            BuiltinKind::HttpRequest => Err(adapter_unavailable(
                "http.request",
                "HTTP adapter is not installed; it is introduced in Phase 6",
            )),
            BuiltinKind::TimeNow => time_now(),
            BuiltinKind::NotificationShow => Err(adapter_unavailable(
                "notification.show",
                "notification adapter is not installed",
            )),
            BuiltinKind::AiGenerate => Err(adapter_unavailable(
                "ai.generate",
                "AI provider adapter is not installed",
            )),
        }
    }
}

/// Create a registry containing all Beta built-in operation contracts.
pub fn builtin_registry() -> Result<OperationRegistry, RegisterError> {
    let mut registry = OperationRegistry::new();
    for operation in builtins() {
        registry.register(operation)?;
    }
    Ok(registry)
}

fn builtins() -> Vec<BuiltinOperation> {
    vec![
        builtin(
            "state.get",
            BuiltinKind::StateGet,
            object_schema(
                json!({
                    "path": { "type": "string", "pattern": "^(|/.*)$" }
                }),
                &["path"],
            ),
            value_output_schema(),
            &[error("path_not_found", "state path does not exist")],
            &[],
            &[],
        ),
        builtin(
            "state.set",
            BuiltinKind::StateSet,
            object_schema(
                json!({
                    "path": { "type": "string", "pattern": "^/.*" },
                    "value": true
                }),
                &["path", "value"],
            ),
            value_output_schema(),
            &[
                error("invalid_path", "state path is not a valid JSON Pointer"),
                error("path_not_found", "state path parent does not exist"),
                error("type_mismatch", "state path parent is not a container"),
            ],
            &[SideEffect::StateWrite],
            &[],
        ),
        builtin(
            "logic.if",
            BuiltinKind::LogicIf,
            object_schema(
                json!({
                    "condition": { "type": "boolean" },
                    "then": true,
                    "else": true
                }),
                &["condition", "then", "else"],
            ),
            value_output_schema(),
            &[],
            &[],
            &[],
        ),
        builtin(
            "collection.filter",
            BuiltinKind::CollectionFilter,
            object_schema(
                json!({
                    "items": { "type": "array" },
                    "predicate": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "pattern": "^(|/.*)$", "default": "" },
                            "operator": { "enum": ["eq", "ne", "gt", "gte", "lt", "lte", "contains"] },
                            "value": true
                        },
                        "required": ["operator", "value"],
                        "additionalProperties": false
                    }
                }),
                &["items", "predicate"],
            ),
            items_output_schema(),
            &[],
            &[],
            &[],
        ),
        builtin(
            "collection.sort",
            BuiltinKind::CollectionSort,
            object_schema(
                json!({
                    "items": { "type": "array" },
                    "path": { "type": "string", "pattern": "^(|/.*)$", "default": "" },
                    "order": { "enum": ["asc", "desc"], "default": "asc" }
                }),
                &["items"],
            ),
            items_output_schema(),
            &[
                error("path_not_found", "sort key path does not exist"),
                error(
                    "unsupported_sort_value",
                    "sort keys are incompatible or non-scalar",
                ),
            ],
            &[],
            &[],
        ),
        builtin(
            "data.transform",
            BuiltinKind::DataTransform,
            object_schema(
                json!({
                    "value": true,
                    "mapping": {
                        "type": "object",
                        "minProperties": 1,
                        "additionalProperties": { "type": "string", "pattern": "^(|/.*)$" }
                    }
                }),
                &["value", "mapping"],
            ),
            value_output_schema(),
            &[error(
                "path_not_found",
                "mapping source path does not exist",
            )],
            &[],
            &[],
        ),
        builtin(
            "http.request",
            BuiltinKind::HttpRequest,
            object_schema(
                json!({
                    "url": { "type": "string", "pattern": "^https?://" },
                    "method": { "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"], "default": "GET" },
                    "headers": { "type": "object", "additionalProperties": { "type": "string" } },
                    "body": true
                }),
                &["url"],
            ),
            object_schema(
                json!({
                    "status": { "type": "integer", "minimum": 100, "maximum": 599 },
                    "headers": { "type": "object", "additionalProperties": { "type": "string" } },
                    "body": true
                }),
                &["status", "headers", "body"],
            ),
            &[error(
                "adapter_unavailable",
                "HTTP host adapter is unavailable",
            )],
            &[SideEffect::NetworkRequest],
            &[Capability::NetworkRequest],
        ),
        builtin(
            "time.now",
            BuiltinKind::TimeNow,
            object_schema(json!({}), &[]),
            object_schema(
                json!({
                    "unixMs": { "type": "integer", "minimum": 0 }
                }),
                &["unixMs"],
            ),
            &[error("clock_error", "host clock is before the Unix epoch")],
            &[SideEffect::TimeRead],
            &[],
        ),
        builtin(
            "notification.show",
            BuiltinKind::NotificationShow,
            object_schema(
                json!({
                    "title": { "type": "string" },
                    "message": { "type": "string", "minLength": 1 }
                }),
                &["message"],
            ),
            object_schema(
                json!({ "delivered": { "type": "boolean" } }),
                &["delivered"],
            ),
            &[error(
                "adapter_unavailable",
                "notification host adapter is unavailable",
            )],
            &[SideEffect::NotificationShow],
            &[Capability::NotificationShow],
        ),
        builtin(
            "ai.generate",
            BuiltinKind::AiGenerate,
            object_schema(
                json!({
                    "prompt": { "type": "string", "minLength": 1 },
                    "model": { "type": "string", "minLength": 1 },
                    "responseSchema": { "type": "object" }
                }),
                &["prompt"],
            ),
            object_schema(
                json!({
                    "text": { "type": "string" },
                    "data": true
                }),
                &["text"],
            ),
            &[error(
                "adapter_unavailable",
                "AI provider adapter is unavailable",
            )],
            &[SideEffect::AiGenerate, SideEffect::NetworkRequest],
            &[Capability::AiGenerate],
        ),
    ]
}

fn builtin(
    id: &str,
    kind: BuiltinKind,
    input: Value,
    output: Value,
    errors: &[ErrorDefinition],
    side_effects: &[SideEffect],
    required_permissions: &[Capability],
) -> BuiltinOperation {
    BuiltinOperation {
        kind,
        definition: OperationDefinition {
            id: id.to_owned(),
            input,
            output,
            errors: errors.to_vec(),
            side_effects: side_effects.to_vec(),
            required_permissions: required_permissions.to_vec(),
        },
    }
}

fn error(code: &str, description: &str) -> ErrorDefinition {
    ErrorDefinition {
        code: code.to_owned(),
        description: description.to_owned(),
    }
}

fn object_schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "$schema": JSON_SCHEMA,
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    })
}

fn value_output_schema() -> Value {
    object_schema(json!({ "value": true }), &["value"])
}

fn items_output_schema() -> Value {
    object_schema(json!({ "items": { "type": "array" } }), &["items"])
}

fn state_get(input: &Value, context: &OperationContext) -> Result<Value, OperationError> {
    let path = input["path"].as_str().unwrap_or_default();
    context
        .state()
        .pointer(path)
        .cloned()
        .map(|value| json!({ "value": value }))
        .ok_or_else(|| {
            OperationError::new(
                "state.get",
                "path_not_found",
                format!("state path `{path}` does not exist"),
            )
        })
}

fn state_set(input: &Value, context: &mut OperationContext) -> Result<Value, OperationError> {
    let path = input["path"].as_str().unwrap_or_default();
    let value = input["value"].clone();
    set_json_pointer(context.state_mut(), path, value.clone())?;
    Ok(json!({ "value": value }))
}

fn set_json_pointer(root: &mut Value, path: &str, value: Value) -> Result<(), OperationError> {
    let Some((parent_path, token)) = path.rsplit_once('/') else {
        return Err(OperationError::new(
            "state.set",
            "invalid_path",
            "state.set path must be a non-root JSON Pointer",
        ));
    };
    let token = decode_pointer_token(token).ok_or_else(|| {
        OperationError::new(
            "state.set",
            "invalid_path",
            format!("state path `{path}` contains an invalid escape"),
        )
    })?;
    let parent = root.pointer_mut(parent_path).ok_or_else(|| {
        OperationError::new(
            "state.set",
            "path_not_found",
            format!("state path parent `{parent_path}` does not exist"),
        )
    })?;
    match parent {
        Value::Object(object) => {
            object.insert(token, value);
            Ok(())
        }
        Value::Array(array) => {
            let index = token.parse::<usize>().map_err(|_| {
                OperationError::new(
                    "state.set",
                    "invalid_path",
                    format!("array index `{token}` is invalid"),
                )
            })?;
            let Some(slot) = array.get_mut(index) else {
                return Err(OperationError::new(
                    "state.set",
                    "path_not_found",
                    format!("array index `{index}` does not exist"),
                ));
            };
            *slot = value;
            Ok(())
        }
        _ => Err(OperationError::new(
            "state.set",
            "type_mismatch",
            format!("state path parent `{parent_path}` is not a container"),
        )),
    }
}

fn decode_pointer_token(token: &str) -> Option<String> {
    let mut decoded = String::new();
    let mut characters = token.chars();
    while let Some(character) = characters.next() {
        if character != '~' {
            decoded.push(character);
            continue;
        }
        match characters.next()? {
            '0' => decoded.push('~'),
            '1' => decoded.push('/'),
            _ => return None,
        }
    }
    Some(decoded)
}

fn collection_filter(input: &Value) -> Result<Value, OperationError> {
    let items = input["items"].as_array().cloned().unwrap_or_default();
    let predicate = &input["predicate"];
    let path = predicate
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let operator = predicate["operator"].as_str().unwrap_or_default();
    let expected = &predicate["value"];
    let items = items
        .into_iter()
        .filter(|item| {
            item.pointer(path)
                .is_some_and(|actual| matches_predicate(actual, operator, expected))
        })
        .collect::<Vec<_>>();
    Ok(json!({ "items": items }))
}

fn matches_predicate(actual: &Value, operator: &str, expected: &Value) -> bool {
    match operator {
        "eq" => actual == expected,
        "ne" => actual != expected,
        "contains" => match (actual, expected) {
            (Value::String(actual), Value::String(expected)) => actual.contains(expected),
            (Value::Array(actual), expected) => actual.contains(expected),
            _ => false,
        },
        "gt" | "gte" | "lt" | "lte" => compare_scalars(actual, expected).is_some_and(|order| {
            matches!(
                (operator, order),
                ("gt", Ordering::Greater)
                    | ("gte", Ordering::Greater | Ordering::Equal)
                    | ("lt", Ordering::Less)
                    | ("lte", Ordering::Less | Ordering::Equal)
            )
        }),
        _ => false,
    }
}

fn collection_sort(input: &Value) -> Result<Value, OperationError> {
    let path = input
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let descending = input.get("order").and_then(Value::as_str) == Some("desc");
    let mut keyed = Vec::new();
    for item in input["items"].as_array().cloned().unwrap_or_default() {
        let Some(key) = item.pointer(path).cloned() else {
            return Err(OperationError::new(
                "collection.sort",
                "path_not_found",
                format!("sort key path `{path}` does not exist"),
            ));
        };
        if !is_sortable(&key) {
            return Err(OperationError::new(
                "collection.sort",
                "unsupported_sort_value",
                "sort keys must be null, boolean, number or string",
            ));
        }
        keyed.push((item, key));
    }
    let category = keyed.first().map(|(_, key)| scalar_category(key));
    if keyed
        .iter()
        .any(|(_, key)| Some(scalar_category(key)) != category)
    {
        return Err(OperationError::new(
            "collection.sort",
            "unsupported_sort_value",
            "all sort keys must have the same JSON scalar type",
        ));
    }
    keyed.sort_by(|(_, left), (_, right)| {
        let order = compare_scalars(left, right).unwrap_or(Ordering::Equal);
        if descending {
            order.reverse()
        } else {
            order
        }
    });
    Ok(json!({
        "items": keyed.into_iter().map(|(item, _)| item).collect::<Vec<_>>()
    }))
}

fn is_sortable(value: &Value) -> bool {
    matches!(
        value,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
    )
}

fn scalar_category(value: &Value) -> u8 {
    match value {
        Value::Null => 0,
        Value::Bool(_) => 1,
        Value::Number(_) => 2,
        Value::String(_) => 3,
        _ => 4,
    }
}

fn compare_scalars(left: &Value, right: &Value) -> Option<Ordering> {
    match (left, right) {
        (Value::Null, Value::Null) => Some(Ordering::Equal),
        (Value::Bool(left), Value::Bool(right)) => Some(left.cmp(right)),
        (Value::Number(left), Value::Number(right)) => left.as_f64()?.partial_cmp(&right.as_f64()?),
        (Value::String(left), Value::String(right)) => Some(left.cmp(right)),
        _ => None,
    }
}

fn data_transform(input: &Value) -> Result<Value, OperationError> {
    let source = &input["value"];
    let mapping = input["mapping"].as_object().cloned().unwrap_or_default();
    let mut transformed = Map::new();
    for (target, path) in mapping {
        let path = path.as_str().unwrap_or_default();
        let Some(value) = source.pointer(path) else {
            return Err(OperationError::new(
                "data.transform",
                "path_not_found",
                format!("mapping source path `{path}` does not exist"),
            ));
        };
        transformed.insert(target, value.clone());
    }
    Ok(json!({ "value": transformed }))
}

fn time_now() -> Result<Value, OperationError> {
    let unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| {
            OperationError::new(
                "time.now",
                "clock_error",
                "host clock is before the Unix epoch",
            )
        })?
        .as_millis();
    let unix_ms = u64::try_from(unix_ms).unwrap_or(u64::MAX);
    Ok(json!({ "unixMs": unix_ms }))
}

fn adapter_unavailable(operation: &str, message: &str) -> OperationError {
    OperationError::new(operation, "adapter_unavailable", message)
}
