use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderPresetCatalog {
    pub version: u32,
    #[serde(default)]
    pub updated_at: String,
    pub providers: Vec<ProviderPreset>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderPreset {
    pub provider_type: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub display_name: String,
    pub llm_endpoint: String,
    pub ocr_endpoint: String,
    pub model_catalog_endpoint: String,
    #[serde(default = "default_true")]
    pub ocr_model_catalog_supported: bool,
    #[serde(default)]
    pub ocr_model_catalog_notice: Option<String>,
    #[serde(default)]
    pub llm_models: Vec<String>,
    #[serde(default)]
    pub ocr_models: Vec<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProviderModelsRequest {
    pub provider_type: String,
    pub api_key: String,
    pub endpoint: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProviderModelsResponse {
    pub models: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}
