use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::executor::block_on;

use crate::{
    AppBuilder, AppGenerationProvider, BuilderConfig, BuilderConstructionError, BuilderErrorCode,
    GenerateAppRequest, ProviderError, ProviderRequest, ProviderRequestPurpose, ProviderResponse,
    MAX_PROVIDER_OUTPUT_BYTES,
};

struct MockProvider {
    responses: Mutex<VecDeque<Result<ProviderResponse, ProviderError>>>,
    requests: Mutex<Vec<ProviderRequest>>,
}

impl MockProvider {
    fn new(responses: Vec<Result<ProviderResponse, ProviderError>>) -> Arc<Self> {
        Arc::new(Self {
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(Vec::new()),
        })
    }

    fn requests(&self) -> Vec<ProviderRequest> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait]
impl AppGenerationProvider for MockProvider {
    async fn generate(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.requests.lock().unwrap().push(request);
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("mock provider response")
    }
}

fn response(content: impl Into<String>) -> Result<ProviderResponse, ProviderError> {
    Ok(ProviderResponse {
        content: content.into(),
    })
}

fn request(requirement: &str) -> GenerateAppRequest {
    GenerateAppRequest {
        requirement: requirement.to_owned(),
        existing_definition: None,
    }
}

fn valid_source() -> &'static str {
    include_str!("../../../examples/hello.aix.json")
}

#[test]
fn generates_and_normalizes_a_valid_definition() {
    let provider = MockProvider::new(vec![response(valid_source())]);
    let builder = AppBuilder::new(provider.clone()).unwrap();
    let generated = block_on(builder.generate(request("Create a greeting app"))).unwrap();

    assert_eq!(generated.definition.metadata.id, "hello");
    assert_eq!(generated.attempts, 1);
    assert!(generated.source.contains("\"specVersion\": \"0.1.0\""));
    assert!(!generated.source.contains("null"));
    aix_runtime::Runtime::new()
        .unwrap()
        .load_json(&generated.source)
        .unwrap();
    let requests = provider.requests();
    assert_eq!(requests[0].purpose, ProviderRequestPurpose::Generate);
    assert!(requests[0].system.contains("Never emit JavaScript"));
    assert!(requests[0].system.contains("state.get"));
}

#[test]
fn accepts_a_single_json_markdown_fence() {
    let provider = MockProvider::new(vec![response(format!("```json\n{}\n```", valid_source()))]);
    let builder = AppBuilder::new(provider).unwrap();
    let generated = block_on(builder.generate(request("Create a greeting app"))).unwrap();
    assert_eq!(generated.definition.metadata.name, "AIX Hello");
}

#[test]
fn repairs_a_candidate_using_validator_issues() {
    let provider = MockProvider::new(vec![response("{\"bad\":true}"), response(valid_source())]);
    let builder = AppBuilder::new(provider.clone()).unwrap();
    let generated = block_on(builder.generate(request("Create a greeting app"))).unwrap();

    assert_eq!(generated.attempts, 2);
    let requests = provider.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].purpose, ProviderRequestPurpose::Repair);
    assert!(requests[1].user.contains("invalid_structure"));
    assert!(requests[1].user.contains("Rejected candidate"));
}

#[test]
fn returns_validation_issues_after_the_repair_limit() {
    let provider = MockProvider::new(vec![response("{}"), response("{}")]);
    let builder = AppBuilder::with_config(
        provider,
        BuilderConfig {
            max_repair_attempts: 1,
        },
    )
    .unwrap();
    let error = block_on(builder.generate(request("Create an app"))).unwrap_err();

    assert_eq!(error.code, BuilderErrorCode::ValidationFailed);
    assert_eq!(error.attempts, 2);
    assert!(!error.issues.is_empty());
}

#[test]
fn rejects_empty_requests_before_calling_the_provider() {
    let provider = MockProvider::new(vec![]);
    let builder = AppBuilder::new(provider.clone()).unwrap();
    let error = block_on(builder.generate(request("  "))).unwrap_err();

    assert_eq!(error.code, BuilderErrorCode::InvalidRequest);
    assert_eq!(error.attempts, 0);
    assert!(provider.requests().is_empty());
}

#[test]
fn does_not_echo_oversized_provider_output_into_a_repair_prompt() {
    let provider = MockProvider::new(vec![response("x".repeat(MAX_PROVIDER_OUTPUT_BYTES + 1))]);
    let builder = AppBuilder::new(provider.clone()).unwrap();
    let error = block_on(builder.generate(request("Create an app"))).unwrap_err();

    assert_eq!(error.code, BuilderErrorCode::OutputTooLarge);
    assert_eq!(provider.requests().len(), 1);
}

#[test]
fn returns_sanitized_provider_failures() {
    let provider = MockProvider::new(vec![Err(ProviderError::new(
        "temporarily_unavailable",
        "provider is unavailable",
    ))]);
    let builder = AppBuilder::new(provider).unwrap();
    let error = block_on(builder.generate(request("Create an app"))).unwrap_err();

    assert_eq!(error.code, BuilderErrorCode::ProviderFailure);
    assert!(error.message.contains("temporarily_unavailable"));
    assert!(error.issues.is_empty());
}

#[test]
fn revision_requests_include_a_validated_definition() {
    let runtime = aix_runtime::Runtime::new().unwrap();
    let existing = runtime.load_json(valid_source()).unwrap();
    let provider = MockProvider::new(vec![response(valid_source())]);
    let builder = AppBuilder::new(provider.clone()).unwrap();
    let generated = block_on(builder.generate(GenerateAppRequest {
        requirement: "Change the greeting".to_owned(),
        existing_definition: Some(existing),
    }))
    .unwrap();

    assert_eq!(generated.attempts, 1);
    let requests = provider.requests();
    assert_eq!(requests[0].purpose, ProviderRequestPurpose::Revise);
    assert!(requests[0].user.contains("existing validated definition"));
}

#[test]
fn rejects_an_invalid_existing_definition_before_provider_access() {
    let runtime = aix_runtime::Runtime::new().unwrap();
    let mut existing = runtime.load_json(valid_source()).unwrap();
    existing.spec_version = "9.0.0".to_owned();
    let provider = MockProvider::new(vec![]);
    let builder = AppBuilder::new(provider.clone()).unwrap();
    let error = block_on(builder.generate(GenerateAppRequest {
        requirement: "Change the greeting".to_owned(),
        existing_definition: Some(existing),
    }))
    .unwrap_err();

    assert_eq!(error.code, BuilderErrorCode::InvalidRequest);
    assert_eq!(error.attempts, 0);
    assert!(provider.requests().is_empty());
}

#[test]
fn provider_protocol_has_no_credential_field() {
    let request = ProviderRequest {
        purpose: ProviderRequestPurpose::Generate,
        system: "system".to_owned(),
        user: "user".to_owned(),
    };
    let json = serde_json::to_value(request).unwrap();
    assert_eq!(
        json.as_object().unwrap().keys().collect::<Vec<_>>(),
        vec!["purpose", "system", "user"]
    );
}

#[test]
fn caps_repair_configuration() {
    let provider = MockProvider::new(vec![]);
    let error = AppBuilder::with_config(
        provider,
        BuilderConfig {
            max_repair_attempts: 4,
        },
    )
    .err()
    .expect("configuration should fail");
    assert!(matches!(error, BuilderConstructionError::Configuration(_)));
}
