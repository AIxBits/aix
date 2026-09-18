//! Provider-neutral AI authoring for AIX App Definitions.
//!
//! The authoring plane asks an injected model provider for JSON, treats the
//! response as untrusted data, and passes it through the same Rust validation
//! boundary used by the runtime. It never executes generated source code and
//! never receives provider credentials.

use std::sync::Arc;

use aix_core::AppDefinition;
use aix_runtime::{RegisterError, Runtime, ValidationIssue};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Largest natural-language request accepted by the builder.
pub const MAX_REQUIREMENT_BYTES: usize = 65_536;
/// Largest model response accepted before parsing.
pub const MAX_PROVIDER_OUTPUT_BYTES: usize = 1_048_576;
/// Hard ceiling for validation-driven repair attempts.
pub const MAX_REPAIR_ATTEMPTS: u8 = 3;

/// Why the authoring plane is calling the provider.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderRequestPurpose {
    /// Create a new definition from a requirement.
    Generate,
    /// Modify a previously validated definition.
    Revise,
    /// Correct a candidate rejected by the validator.
    Repair,
}

/// Credential-free request sent to an AI provider adapter.
///
/// The adapter owns the provider profile and secret lookup. API keys are not
/// represented in this protocol and therefore cannot enter prompts or logs.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderRequest {
    /// Generation stage represented by this call.
    pub purpose: ProviderRequestPurpose,
    /// Trusted contract assembled by AIX, including schemas and safety rules.
    pub system: String,
    /// User requirement or validator-driven repair request.
    pub user: String,
}

/// Text returned by a provider adapter.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderResponse {
    /// Candidate definition returned as text by the model.
    pub content: String,
}

/// Sanitized provider failure safe to return to the authoring UI.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderError {
    /// Stable, sanitized adapter error code.
    pub code: String,
    /// Human-readable message that contains no credential or raw header.
    pub message: String,
}

impl ProviderError {
    /// Construct a provider error without attaching credentials or raw headers.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// Adapter implemented by hosted or local model integrations.
#[async_trait]
pub trait AppGenerationProvider: Send + Sync {
    /// Generate one candidate response.
    async fn generate(&self, request: ProviderRequest) -> Result<ProviderResponse, ProviderError>;
}

/// User request to create or revise an AIX app.
#[derive(Clone, Debug)]
pub struct GenerateAppRequest {
    /// Natural-language behavior requested by the user.
    pub requirement: String,
    /// Existing validated definition when revising an app.
    pub existing_definition: Option<AppDefinition>,
}

/// Limits applied to one generation session.
#[derive(Clone, Copy, Debug)]
pub struct BuilderConfig {
    /// Number of validator-driven correction calls after the first response.
    pub max_repair_attempts: u8,
}

impl Default for BuilderConfig {
    fn default() -> Self {
        Self {
            max_repair_attempts: 2,
        }
    }
}

/// Successfully generated and validated app definition.
#[derive(Clone, Debug)]
pub struct GeneratedApp {
    /// Strict Rust representation accepted by the runtime.
    pub definition: AppDefinition,
    /// Normalized JSON source suitable for saving as an `.aix.json` file.
    pub source: String,
    /// Total provider calls used, including repairs.
    pub attempts: u8,
}

/// Stable authoring failure categories.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuilderErrorCode {
    InvalidRequest,
    InvalidConfiguration,
    ProviderFailure,
    OutputTooLarge,
    ValidationFailed,
}

/// Failure returned to an authoring client.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuilderError {
    /// Stable category suitable for UI behavior.
    pub code: BuilderErrorCode,
    /// Human-readable failure summary.
    pub message: String,
    /// Number of provider calls completed before failure.
    pub attempts: u8,
    /// Runtime validation issues when candidate validation failed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<ValidationIssue>,
}

impl std::fmt::Display for BuilderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl std::error::Error for BuilderError {}

/// Validating app-generation service used by a CLI or desktop host.
pub struct AppBuilder {
    provider: Arc<dyn AppGenerationProvider>,
    runtime: Runtime,
    config: BuilderConfig,
    system_prompt: String,
}

impl AppBuilder {
    /// Build an authoring service with default repair limits.
    pub fn new(provider: Arc<dyn AppGenerationProvider>) -> Result<Self, BuilderConstructionError> {
        Self::with_config(provider, BuilderConfig::default())
    }

    /// Build an authoring service with explicit limits.
    pub fn with_config(
        provider: Arc<dyn AppGenerationProvider>,
        config: BuilderConfig,
    ) -> Result<Self, BuilderConstructionError> {
        if config.max_repair_attempts > MAX_REPAIR_ATTEMPTS {
            return Err(BuilderConstructionError::Configuration(BuilderError {
                code: BuilderErrorCode::InvalidConfiguration,
                message: format!("max_repair_attempts cannot exceed {MAX_REPAIR_ATTEMPTS}"),
                attempts: 0,
                issues: vec![],
            }));
        }
        let runtime = Runtime::new().map_err(BuilderConstructionError::Runtime)?;
        let system_prompt = build_system_prompt(&runtime);
        Ok(Self {
            provider,
            runtime,
            config,
            system_prompt,
        })
    }

    /// Generate, validate and when necessary repair one complete definition.
    pub async fn generate(
        &self,
        request: GenerateAppRequest,
    ) -> Result<GeneratedApp, BuilderError> {
        validate_requirement(&request.requirement)?;
        if let Some(existing) = &request.existing_definition {
            if let Err(error) = aix_runtime::validate_app(existing, self.runtime.operations()) {
                return Err(BuilderError {
                    code: BuilderErrorCode::InvalidRequest,
                    message: "existing definition is not valid for this runtime".to_owned(),
                    attempts: 0,
                    issues: error.issues,
                });
            }
        }
        let purpose = if request.existing_definition.is_some() {
            ProviderRequestPurpose::Revise
        } else {
            ProviderRequestPurpose::Generate
        };
        let mut provider_request = ProviderRequest {
            purpose,
            system: self.system_prompt.clone(),
            user: initial_user_prompt(&request)?,
        };
        let total_attempts = self.config.max_repair_attempts + 1;
        let mut last_issues = Vec::new();

        for attempt in 1..=total_attempts {
            let response = self
                .provider
                .generate(provider_request)
                .await
                .map_err(|error| BuilderError {
                    code: BuilderErrorCode::ProviderFailure,
                    message: format!("provider `{}` failed: {}", error.code, error.message),
                    attempts: attempt,
                    issues: vec![],
                })?;
            if response.content.len() > MAX_PROVIDER_OUTPUT_BYTES {
                return Err(BuilderError {
                    code: BuilderErrorCode::OutputTooLarge,
                    message: "provider response exceeds 1 MiB".to_owned(),
                    attempts: attempt,
                    issues: vec![],
                });
            }
            let candidate = extract_json(&response.content);
            match self.runtime.load_json(&candidate) {
                Ok(definition) => {
                    let source = serde_json::to_string_pretty(&definition).map_err(|error| {
                        BuilderError {
                            code: BuilderErrorCode::ValidationFailed,
                            message: format!(
                                "validated definition could not be serialized: {error}"
                            ),
                            attempts: attempt,
                            issues: vec![],
                        }
                    })?;
                    return Ok(GeneratedApp {
                        definition,
                        source,
                        attempts: attempt,
                    });
                }
                Err(error) => {
                    last_issues = error.issues;
                    if attempt == total_attempts {
                        break;
                    }
                    provider_request =
                        repair_request(&self.system_prompt, &candidate, &last_issues)?;
                }
            }
        }

        Err(BuilderError {
            code: BuilderErrorCode::ValidationFailed,
            message: "provider did not produce a valid AIX App Definition".to_owned(),
            attempts: total_attempts,
            issues: last_issues,
        })
    }
}

/// Failure while constructing an [`AppBuilder`].
#[derive(Debug)]
pub enum BuilderConstructionError {
    /// Built-in runtime contracts could not be initialized.
    Runtime(RegisterError),
    /// A configured authoring limit is outside the supported range.
    Configuration(BuilderError),
}

impl std::fmt::Display for BuilderConstructionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Runtime(error) => write!(formatter, "runtime initialization failed: {error}"),
            Self::Configuration(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for BuilderConstructionError {}

fn validate_requirement(requirement: &str) -> Result<(), BuilderError> {
    if requirement.trim().is_empty() {
        return Err(BuilderError {
            code: BuilderErrorCode::InvalidRequest,
            message: "requirement cannot be empty".to_owned(),
            attempts: 0,
            issues: vec![],
        });
    }
    if requirement.len() > MAX_REQUIREMENT_BYTES {
        return Err(BuilderError {
            code: BuilderErrorCode::InvalidRequest,
            message: "requirement exceeds 64 KiB".to_owned(),
            attempts: 0,
            issues: vec![],
        });
    }
    Ok(())
}

fn build_system_prompt(runtime: &Runtime) -> String {
    let operation_definitions = serde_json::to_string(&runtime.operations().definitions())
        .expect("built-in operation definitions must serialize");
    format!(
        "You generate complete AIX App Definition JSON. Return exactly one JSON object and no prose. Never emit JavaScript, Python, shell commands, inline scripts, executable expressions, secrets, or credentials. Use only operations in the supplied registry. The output must satisfy this JSON Schema:\n{}\nRegistered operations:\n{}",
        include_str!("../../../packages/aix-schema/app.schema.json"),
        operation_definitions
    )
}

fn initial_user_prompt(request: &GenerateAppRequest) -> Result<String, BuilderError> {
    let mut prompt = format!(
        "Create an AIX app for this user requirement:\n{}",
        request.requirement
    );
    if let Some(existing) = &request.existing_definition {
        let source = serde_json::to_string(existing).map_err(|error| BuilderError {
            code: BuilderErrorCode::InvalidRequest,
            message: format!("existing definition could not be serialized: {error}"),
            attempts: 0,
            issues: vec![],
        })?;
        if source.len() > MAX_PROVIDER_OUTPUT_BYTES {
            return Err(BuilderError {
                code: BuilderErrorCode::InvalidRequest,
                message: "existing definition exceeds 1 MiB".to_owned(),
                attempts: 0,
                issues: vec![],
            });
        }
        prompt.push_str(
            "\nRevise this existing validated definition and return the complete result:\n",
        );
        prompt.push_str(&source);
    }
    Ok(prompt)
}

fn repair_request(
    system_prompt: &str,
    candidate: &str,
    issues: &[ValidationIssue],
) -> Result<ProviderRequest, BuilderError> {
    let issues = serde_json::to_string(issues).map_err(|error| BuilderError {
        code: BuilderErrorCode::ValidationFailed,
        message: format!("validation issues could not be serialized: {error}"),
        attempts: 0,
        issues: vec![],
    })?;
    Ok(ProviderRequest {
        purpose: ProviderRequestPurpose::Repair,
        system: system_prompt.to_owned(),
        user: format!(
            "The candidate below was rejected. Return a complete corrected JSON object.\nValidation issues:\n{issues}\nRejected candidate:\n{candidate}"
        ),
    })
}

fn extract_json(content: &str) -> String {
    let trimmed = content.trim();
    if let Some(after_fence) = trimmed.strip_prefix("```json") {
        if let Some(candidate) = after_fence.strip_suffix("```") {
            return candidate.trim().to_owned();
        }
    }
    trimmed.to_owned()
}

#[cfg(test)]
mod tests;
