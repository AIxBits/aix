//! AIX operation runtime.
//!
//! The runtime executes only registered Rust operations. Every invocation is
//! surrounded by JSON Schema validation; app input is decoded into strict data
//! types and checked again at this trust boundary.

mod app_validation;
mod binding;
mod builtins;
mod connector_operation;
mod operation;
mod state_store;
mod workflow;

pub use aix_connector::{
    import_openapi, import_openapi_json, ConnectorError, HttpLimits, HttpRequest, HttpResponse,
    HttpTransport, ReqwestHttpTransport, MAX_IMPORTED_OPERATIONS, MAX_OPENAPI_DOCUMENT_BYTES,
};
pub use aix_permission::{
    CapabilityResolver, HostGrant, PermissionCheck, PermissionDenied, PermissionResolver,
    PermissionTarget, ResolverBuildError,
};
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

use aix_core::{AppDefinition, Connector};
use serde_json::Value;

/// Runtime facade for app validation and checked operation execution.
pub struct Runtime {
    operations: OperationRegistry,
}

impl Runtime {
    /// Construct a runtime with all built-in atomic operations.
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
        let app = app_validation::parse_app_json(source)?;
        let mut operations = builtin_registry().map_err(registration_validation_error)?;
        connector_operation::register_connectors(&mut operations, &app.connectors)
            .map_err(registration_validation_error)?;
        validate_app(&app, &operations)?;
        Ok(app)
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

    pub(crate) fn install_connectors(
        &mut self,
        connectors: &[Connector],
    ) -> Result<(), RegisterError> {
        connector_operation::register_connectors(&mut self.operations, connectors)
    }
}

fn registration_validation_error(error: RegisterError) -> AppValidationError {
    AppValidationError {
        issues: vec![ValidationIssue {
            path: "/connectors".to_owned(),
            code: "invalid_connector_operation".to_owned(),
            message: error.to_string(),
        }],
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
