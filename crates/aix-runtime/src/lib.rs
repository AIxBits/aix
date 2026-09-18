//! AIX operation runtime.
//!
//! The runtime executes only registered Rust operations. Every invocation is
//! surrounded by JSON Schema validation; app input is decoded into strict data
//! types and checked again at this trust boundary.

mod app_validation;
mod binding;
mod builtins;
mod operation;
mod state_store;
mod workflow;

pub use app_validation::{
    validate_app, validate_app_json, AppValidationError, ValidationIssue, MAX_APP_DEFINITION_BYTES,
    MAX_WORKFLOW_STEPS,
};
pub use binding::{
    contains_bindings, resolve_bindings, validate_binding_template, BindingError, BindingSources,
    MAX_BINDING_DEPTH,
};
pub use builtins::builtin_registry;
pub use operation::{
    ErrorDefinition, Operation, OperationContext, OperationDefinition, OperationError,
    OperationRegistry, RegisterError, SideEffect,
};
pub use state_store::{MemoryStateStore, SqliteStateStore, StateStore, StateStoreError};
pub use workflow::{
    AppSession, CancellationToken, DispatchResult, ExecutionLimits, RuntimeEvent, SessionError,
    StepExecution, TimerSubscription, WorkflowError, WorkflowErrorCode, WorkflowExecution,
};

use aix_core::AppDefinition;
use serde_json::Value;

/// Runtime facade for app validation and checked operation execution.
pub struct Runtime {
    operations: OperationRegistry,
}

impl Runtime {
    /// Construct a runtime with all Phase 2 built-in operations.
    pub fn new() -> Result<Self, RegisterError> {
        Ok(Self {
            operations: builtin_registry()?,
        })
    }

    /// Access the registered operation contracts.
    pub fn operations(&self) -> &OperationRegistry {
        &self.operations
    }

    /// Parse and validate a bounded JSON app definition.
    pub fn load_json(&self, source: &str) -> Result<AppDefinition, AppValidationError> {
        validate_app_json(source, &self.operations)
    }

    /// Execute one registered operation through input/output validation.
    pub fn execute(
        &self,
        id: &str,
        input: &Value,
        context: &mut OperationContext,
    ) -> Result<Value, OperationError> {
        self.operations.execute(id, input, context)
    }
}

/// Current supported specification version.
pub fn spec_version() -> &'static str {
    aix_core::SPEC_VERSION
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod workflow_tests;
