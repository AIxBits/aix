//! Operation contracts, registry and checked execution.

use std::collections::{BTreeMap, BTreeSet};

use aix_core::Capability;
use jsonschema::Validator;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// A host-observable effect produced by an operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SideEffect {
    #[serde(rename = "state.write")]
    StateWrite,
    #[serde(rename = "network.request")]
    NetworkRequest,
    #[serde(rename = "notification.show")]
    NotificationShow,
    #[serde(rename = "ai.generate")]
    AiGenerate,
    #[serde(rename = "time.read")]
    TimeRead,
}

/// A stable error code advertised by an operation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ErrorDefinition {
    pub code: String,
    pub description: String,
}

/// Complete machine-readable definition of one registered operation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationDefinition {
    pub id: String,
    pub input: Value,
    pub output: Value,
    pub errors: Vec<ErrorDefinition>,
    pub side_effects: Vec<SideEffect>,
    pub required_permissions: Vec<Capability>,
}

/// Structured failure returned by the operation boundary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationError {
    pub operation_id: String,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl OperationError {
    /// Construct an operation-owned error with a stable code.
    pub fn new(
        operation_id: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            operation_id: operation_id.into(),
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    /// Attach structured diagnostic data safe for callers to inspect.
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }
}

/// Mutable data available to an operation invocation.
///
/// Host adapters are intentionally absent in Phase 2. Operations that require
/// them return `adapter_unavailable` without performing a side effect.
#[derive(Clone, Debug)]
pub struct OperationContext {
    state: Value,
}

impl OperationContext {
    /// Create a context with object-valued app-local state.
    pub fn new(state: Value) -> Result<Self, OperationError> {
        if !state.is_object() {
            return Err(OperationError::new(
                "runtime",
                "invalid_state",
                "application state must be a JSON object",
            ));
        }
        Ok(Self { state })
    }

    /// Create an empty app-local state object.
    pub fn empty() -> Self {
        Self { state: json!({}) }
    }

    /// Read the current state snapshot.
    pub fn state(&self) -> &Value {
        &self.state
    }

    pub(crate) fn state_mut(&mut self) -> &mut Value {
        &mut self.state
    }
}

/// Implementation behind a registered operation definition.
pub trait Operation: Send + Sync {
    /// Return the immutable public contract for this operation.
    fn definition(&self) -> &OperationDefinition;

    /// Execute against validated JSON input.
    fn execute(
        &self,
        input: &Value,
        context: &mut OperationContext,
    ) -> Result<Value, OperationError>;
}

struct RegisteredOperation {
    implementation: Box<dyn Operation>,
    input_validator: Validator,
    output_validator: Validator,
}

/// Errors encountered while building an operation registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegisterError {
    InvalidId(String),
    DuplicateId(String),
    DuplicateErrorCode { operation: String, code: String },
    InvalidInputSchema { operation: String, message: String },
    InvalidOutputSchema { operation: String, message: String },
}

impl std::fmt::Display for RegisterError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for RegisterError {}

/// Registry that validates every input and output around operation execution.
#[derive(Default)]
pub struct OperationRegistry {
    operations: BTreeMap<String, RegisteredOperation>,
}

impl OperationRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an implementation after compiling and checking its contract.
    pub fn register(&mut self, operation: impl Operation + 'static) -> Result<(), RegisterError> {
        let definition = operation.definition();
        if !is_operation_id(&definition.id) {
            return Err(RegisterError::InvalidId(definition.id.clone()));
        }
        if self.operations.contains_key(&definition.id) {
            return Err(RegisterError::DuplicateId(definition.id.clone()));
        }
        let mut error_codes = BTreeSet::new();
        for error in &definition.errors {
            if !error_codes.insert(error.code.as_str()) {
                return Err(RegisterError::DuplicateErrorCode {
                    operation: definition.id.clone(),
                    code: error.code.clone(),
                });
            }
        }
        let input_validator = jsonschema::validator_for(&definition.input).map_err(|error| {
            RegisterError::InvalidInputSchema {
                operation: definition.id.clone(),
                message: error.to_string(),
            }
        })?;
        let output_validator = jsonschema::validator_for(&definition.output).map_err(|error| {
            RegisterError::InvalidOutputSchema {
                operation: definition.id.clone(),
                message: error.to_string(),
            }
        })?;
        self.operations.insert(
            definition.id.clone(),
            RegisteredOperation {
                implementation: Box::new(operation),
                input_validator,
                output_validator,
            },
        );
        Ok(())
    }

    /// Return public definitions in deterministic ID order.
    pub fn definitions(&self) -> Vec<&OperationDefinition> {
        self.operations
            .values()
            .map(|registered| registered.implementation.definition())
            .collect()
    }

    /// Look up one public operation definition.
    pub fn definition(&self, id: &str) -> Option<&OperationDefinition> {
        self.operations
            .get(id)
            .map(|registered| registered.implementation.definition())
    }

    /// Validate input without executing the operation.
    pub fn validate_input(&self, id: &str, input: &Value) -> Result<(), OperationError> {
        let Some(registered) = self.operations.get(id) else {
            return Err(OperationError::new(
                id,
                "unknown_operation",
                format!("operation `{id}` is not registered"),
            ));
        };
        let input_errors = validation_messages(&registered.input_validator, input);
        if input_errors.is_empty() {
            Ok(())
        } else {
            Err(OperationError::new(
                id,
                "invalid_input",
                "operation input does not match its schema",
            )
            .with_details(json!({ "errors": input_errors })))
        }
    }

    /// Validate input, execute the operation and validate its output.
    pub fn execute(
        &self,
        id: &str,
        input: &Value,
        context: &mut OperationContext,
    ) -> Result<Value, OperationError> {
        self.validate_input(id, input)?;
        let registered = self
            .operations
            .get(id)
            .expect("validated operation must remain registered");

        let state_before_execution = context.state.clone();
        let output = match registered.implementation.execute(input, context) {
            Ok(output) => output,
            Err(error) => {
                context.state = state_before_execution;
                let declared = registered
                    .implementation
                    .definition()
                    .errors
                    .iter()
                    .any(|definition| definition.code == error.code);
                if declared {
                    return Err(error);
                }
                return Err(OperationError::new(
                    id,
                    "operation_contract_violation",
                    format!("operation returned undeclared error `{}`", error.code),
                ));
            }
        };

        let output_errors = validation_messages(&registered.output_validator, &output);
        if !output_errors.is_empty() {
            context.state = state_before_execution;
            return Err(OperationError::new(
                id,
                "invalid_output",
                "operation output does not match its schema",
            )
            .with_details(json!({ "errors": output_errors })));
        }
        Ok(output)
    }
}

fn validation_messages(validator: &Validator, value: &Value) -> Vec<String> {
    validator
        .iter_errors(value)
        .map(|error| error.to_string())
        .collect()
}

fn is_operation_id(id: &str) -> bool {
    let mut segments = id.split('.');
    let valid_segment = |segment: &str| {
        !segment.is_empty()
            && segment.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
            })
            && segment
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_lowercase())
    };
    let first = segments.next().is_some_and(valid_segment);
    let remaining: Vec<_> = segments.collect();
    first && !remaining.is_empty() && remaining.into_iter().all(valid_segment)
}
