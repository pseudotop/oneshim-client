// AI 프로바이더 설정 — AiProviderConfig 및 외부 엔드포인트 검증 오케스트레이터
use super::super::enums::*;
use super::ai_validation::{
    ExternalApiEndpoint, OcrValidationConfig, SceneActionOverrideConfig, SceneIntelligenceConfig,
};
use crate::error::CoreError;
use serde::{Deserialize, Serialize};

// ── AiProviderConfig ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProviderConfig {
    #[serde(default)]
    pub access_mode: AiAccessMode,
    #[serde(default)]
    pub ocr_provider: OcrProviderType,
    #[serde(default)]
    pub llm_provider: LlmProviderType,
    #[serde(default)]
    pub ocr_api: Option<ExternalApiEndpoint>,
    #[serde(default)]
    pub llm_api: Option<ExternalApiEndpoint>,
    #[serde(default)]
    pub external_data_policy: ExternalDataPolicy,
    #[serde(default)]
    pub allow_unredacted_external_ocr: bool,
    #[serde(default)]
    pub ocr_validation: OcrValidationConfig,
    #[serde(default)]
    pub scene_action_override: SceneActionOverrideConfig,
    #[serde(default)]
    pub scene_intelligence: SceneIntelligenceConfig,
    #[serde(default = "default_true")]
    pub fallback_to_local: bool,
}

impl Default for AiProviderConfig {
    fn default() -> Self {
        Self {
            access_mode: AiAccessMode::default(),
            ocr_provider: OcrProviderType::default(),
            llm_provider: LlmProviderType::default(),
            ocr_api: None,
            llm_api: None,
            external_data_policy: ExternalDataPolicy::default(),
            allow_unredacted_external_ocr: false,
            ocr_validation: OcrValidationConfig::default(),
            scene_action_override: SceneActionOverrideConfig::default(),
            scene_intelligence: SceneIntelligenceConfig::default(),
            fallback_to_local: true,
        }
    }
}

impl AiProviderConfig {
    pub fn validate_selected_remote_endpoints(&self) -> Result<(), CoreError> {
        self.ocr_validation.validate()?;
        self.scene_action_override.validate()?;
        self.scene_intelligence.validate()?;

        match self.access_mode {
            AiAccessMode::ProviderApiKey | AiAccessMode::PlatformConnected => {
                if self.ocr_provider == OcrProviderType::Remote {
                    validate_remote_endpoint(self.ocr_api.as_ref(), "ocr_api")?;
                }
                if self.llm_provider == LlmProviderType::Remote {
                    validate_remote_endpoint(self.llm_api.as_ref(), "llm_api")?;
                }
            }
            AiAccessMode::LocalModel => {}
            AiAccessMode::ProviderSubscriptionCli => {
                if self.ocr_provider == OcrProviderType::Remote
                    || self.llm_provider == LlmProviderType::Remote
                {
                    return Err(CoreError::Config(
                        "Provider subscription (CLI) mode requires local OCR/LLM providers instead of remote providers."
                            .to_string(),
                    ));
                }
            }
            AiAccessMode::ProviderOAuth => {
                // OAuth mode: LLM uses managed OAuth credentials (no API key needed).
                // OCR still respects its own provider setting (local/remote with API key).
                if self.ocr_provider == OcrProviderType::Remote {
                    validate_remote_endpoint(self.ocr_api.as_ref(), "ocr_api")?;
                }
            }
        }
        Ok(())
    }
}

// ── validate_remote_endpoint (모듈-내부 헬퍼) ─────────────────────

fn validate_remote_endpoint(
    endpoint: Option<&ExternalApiEndpoint>,
    field_name: &str,
) -> Result<(), CoreError> {
    let endpoint = endpoint.ok_or_else(|| {
        CoreError::Config(format!(
            "`{field_name}` is required when a remote provider is selected."
        ))
    })?;

    let endpoint_url = endpoint.endpoint.trim();
    if endpoint_url.is_empty() {
        return Err(CoreError::Config(format!(
            "`{field_name}.endpoint` must not be empty."
        )));
    }
    if !(endpoint_url.starts_with("http://") || endpoint_url.starts_with("https://")) {
        return Err(CoreError::Config(format!(
            "`{field_name}.endpoint` must be an http:// or https:// URL."
        )));
    }

    if endpoint.api_key.trim().is_empty() {
        return Err(CoreError::Config(format!(
            "`{field_name}.api_key` must not be empty."
        )));
    }

    if endpoint.timeout_secs == 0 {
        return Err(CoreError::Config(format!(
            "`{field_name}.timeout_secs` must be >= 1."
        )));
    }

    if let Some(model) = endpoint
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let decision = crate::ai_model_lifecycle_policy::evaluate_model_lifecycle_now(
            endpoint.provider_type,
            model,
        )?;
        if let crate::ai_model_lifecycle_policy::ModelLifecycleDecision::Block { message, .. } =
            decision
        {
            return Err(CoreError::PolicyDenied(message));
        }
    }

    Ok(())
}

// ── Private default helpers ─────────────────────────────────────────

fn default_true() -> bool {
    true
}
