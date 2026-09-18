//! Shared AIX protocol types.
//!
//! These types contain data only. They are independent of UI frameworks, host
//! containers and AI providers, and they never evaluate application input.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Version of the app definition supported by this build.
pub const SPEC_VERSION: &str = "0.1.0";

/// A structurally decoded AIX application.
///
/// Deserialization rejects unknown fields. Semantic checks such as unique IDs,
/// workflow graph integrity and registered operations are owned by the runtime.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppDefinition {
    pub spec_version: String,
    pub metadata: AppMetadata,
    pub state: BTreeMap<String, Value>,
    pub resources: BTreeMap<String, Resource>,
    pub ui: UiNode,
    pub workflows: Vec<Workflow>,
    pub connectors: Vec<Connector>,
    pub permissions: Vec<PermissionRequest>,
}

/// Application identity shown to users and tooling.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A typed resource whose value is resolved independently from the UI.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Resource {
    #[serde(rename = "type")]
    pub kind: ResourceKind,
    pub value: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Resource kinds supported by the Beta specification.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceKind {
    Text,
    Image,
    Audio,
    Video,
    Data,
}

/// A node in the declarative UI tree.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiNode {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: UiNodeKind,
    #[serde(default)]
    pub props: BTreeMap<String, Value>,
    #[serde(default)]
    pub children: Vec<UiNode>,
}

/// UI node kinds supported by the Beta renderer contract.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UiNodeKind {
    Page,
    Container,
    Text,
    Button,
    Input,
    List,
    Table,
    Image,
    Audio,
    Video,
}

/// A finite operation graph triggered by one event.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workflow {
    pub id: String,
    pub on: EventTrigger,
    pub entry: String,
    pub steps: Vec<WorkflowStep>,
}

/// Event that selects a workflow.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventTrigger {
    #[serde(rename = "type")]
    pub kind: EventKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_ms: Option<u64>,
}

/// Events accepted by the Beta workflow contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    #[serde(rename = "app.start")]
    AppStart,
    #[serde(rename = "ui.click")]
    UiClick,
    #[serde(rename = "ui.change")]
    UiChange,
    #[serde(rename = "timer")]
    Timer,
}

/// A call to one registered operation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowStep {
    pub id: String,
    pub operation: String,
    pub input: Value,
    #[serde(default)]
    pub next: Vec<String>,
}

/// An external connector declaration.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Connector {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: ConnectorKind,
    pub base_url: String,
    pub operations: Vec<ConnectorOperation>,
}

/// Connector families recognized by the Beta schema.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectorKind {
    Http,
}

/// One HTTP operation exposed by a connector.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorOperation {
    pub id: String,
    pub method: HttpMethod,
    pub path: String,
}

/// HTTP methods accepted by connector definitions.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

/// A capability and scopes requested by an app.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionRequest {
    pub capability: Capability,
    pub scopes: Vec<String>,
}

/// Capability names shared by app definitions and operation descriptors.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Capability {
    #[serde(rename = "ai.generate")]
    AiGenerate,
    #[serde(rename = "network.request")]
    NetworkRequest,
    #[serde(rename = "file.read")]
    FileRead,
    #[serde(rename = "file.write")]
    FileWrite,
    #[serde(rename = "notification.show")]
    NotificationShow,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_example_envelope() {
        let app: AppDefinition =
            serde_json::from_str(include_str!("../../../examples/hello.aix.json")).unwrap();
        assert_eq!(app.spec_version, SPEC_VERSION);
        assert_eq!(app.state["city"], "Shanghai");
        assert!(matches!(app.ui.kind, UiNodeKind::Page));
    }

    #[test]
    fn rejects_unknown_app_fields() {
        let source = include_str!("../../../examples/hello.aix.json").replace(
            "\"specVersion\":",
            "\"script\": \"echo unsafe\", \"specVersion\":",
        );
        assert!(serde_json::from_str::<AppDefinition>(&source).is_err());
    }
}
