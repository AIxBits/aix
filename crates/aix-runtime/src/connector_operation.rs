//! Conversion of App Definition connectors into registered Operations.

use aix_connector::{build_connector_request, validate_connector};
use aix_core::{Capability, Connector, ConnectorOperation};
use aix_permission::PermissionTarget;
use serde_json::{json, Value};

use crate::builtins::http_errors;
use crate::{
    Operation, OperationContext, OperationDefinition, OperationError, OperationRegistry,
    RegisterError, SideEffect,
};

const JSON_SCHEMA: &str = "https://json-schema.org/draft/2020-12/schema";

pub(crate) fn register_connectors(
    registry: &mut OperationRegistry,
    connectors: &[Connector],
) -> Result<(), RegisterError> {
    for connector in connectors {
        validate_connector(connector).map_err(|error| RegisterError::InvalidConnector {
            connector: connector.id.clone(),
            message: error.message,
        })?;
        for operation in &connector.operations {
            registry.register(HttpConnectorOperation::new(
                connector.clone(),
                operation.clone(),
            ))?;
        }
    }
    Ok(())
}

struct HttpConnectorOperation {
    connector: Connector,
    operation: ConnectorOperation,
    definition: OperationDefinition,
}

impl HttpConnectorOperation {
    fn new(connector: Connector, operation: ConnectorOperation) -> Self {
        let definition = OperationDefinition {
            id: operation.id.clone(),
            input: json!({
                "$schema": JSON_SCHEMA,
                "type": "object",
                "properties": {
                    "pathParams": {
                        "type": "object",
                        "additionalProperties": { "type": ["string", "number", "boolean"] }
                    },
                    "query": {
                        "type": "object",
                        "additionalProperties": {
                            "oneOf": [
                                { "type": ["string", "number", "boolean"] },
                                { "type": "array", "items": { "type": ["string", "number", "boolean"] } }
                            ]
                        }
                    },
                    "headers": { "type": "object", "additionalProperties": { "type": "string" } },
                    "body": true
                },
                "additionalProperties": false
            }),
            output: json!({
                "$schema": JSON_SCHEMA,
                "type": "object",
                "properties": {
                    "status": { "type": "integer", "minimum": 100, "maximum": 599 },
                    "headers": { "type": "object", "additionalProperties": { "type": "string" } },
                    "body": true
                },
                "required": ["status", "headers", "body"],
                "additionalProperties": false
            }),
            errors: http_errors(),
            side_effects: vec![SideEffect::NetworkRequest],
            required_permissions: vec![Capability::NetworkRequest],
        };
        Self {
            connector,
            operation,
            definition,
        }
    }
}

impl Operation for HttpConnectorOperation {
    fn definition(&self) -> &OperationDefinition {
        &self.definition
    }

    fn permission_target(
        &self,
        capability: &Capability,
        input: &Value,
    ) -> Result<PermissionTarget, OperationError> {
        if capability != &Capability::NetworkRequest {
            return Err(OperationError::new(
                &self.definition.id,
                "permission_target_unavailable",
                "connector capability target is unavailable",
            ));
        }
        build_connector_request(&self.connector, &self.operation, input)
            .map_err(|error| OperationError::new(&self.definition.id, error.code, error.message))
            .map(|request| PermissionTarget::NetworkUrl(request.url))
    }

    fn execute(
        &self,
        input: &Value,
        context: &mut OperationContext,
    ) -> Result<Value, OperationError> {
        let request = build_connector_request(&self.connector, &self.operation, input)
            .map_err(|error| OperationError::new(&self.definition.id, error.code, error.message))?;
        context
            .execute_http(&request)
            .map(|response| {
                json!({
                    "status": response.status,
                    "headers": response.headers,
                    "body": response.body
                })
            })
            .map_err(|error| OperationError::new(&self.definition.id, error.code, error.message))
    }
}
