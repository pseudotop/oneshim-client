use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderTransportSpec {
    pub method: String,
    pub url: String,
    pub auth_scheme: String,
    pub request_shape: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderModelCatalogTransportSpec {
    pub method: String,
    pub url: String,
    pub auth_scheme: String,
    pub response_shape: String,
    #[serde(default = "default_true")]
    pub llm_supported: bool,
    #[serde(default = "default_true")]
    pub ocr_supported: bool,
    #[serde(default)]
    pub ocr_notice: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderParameterSet {
    pub llm: ProviderParameterProfile,
    pub ocr: ProviderParameterProfile,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderParameterProfile {
    #[serde(default)]
    pub supported: Vec<String>,
    #[serde(default)]
    pub unsupported: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProviderModelsRequest {
    pub provider_type: String,
    pub api_key: String,
    pub endpoint: Option<String>,
    #[serde(default)]
    pub surface: Option<String>,
    #[serde(default)]
    pub surface_id: Option<String>,
    #[serde(default)]
    pub use_saved_secret: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProviderModelsResponse {
    pub models: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}
