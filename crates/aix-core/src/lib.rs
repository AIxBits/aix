//! Shared protocol types. Independent of UI, host and AI providers.
use serde::{Deserialize, Serialize};
/// Version of the app definition supported by this build.
pub const SPEC_VERSION: &str = "0.1.0";
/// Validated application envelope. Full structural validation belongs to the schema boundary.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppDefinition {
    pub spec_version: String,
    pub metadata: serde_json::Value,
    pub state: serde_json::Value,
    pub resources: serde_json::Value,
    pub ui: serde_json::Value,
    pub workflows: Vec<serde_json::Value>,
    pub connectors: Vec<serde_json::Value>,
    pub permissions: Vec<serde_json::Value>,
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
    }
}
