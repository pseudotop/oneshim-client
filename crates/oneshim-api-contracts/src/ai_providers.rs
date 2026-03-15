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
pub struct ProviderHealthTransportSpec {
    pub method: String,
    pub url: String,
    pub auth_scheme: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderModelSupportStatus {
    Supported,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ProviderModelCapabilityRules {
    #[serde(default)]
    pub llm: ProviderModelCapabilityProfile,
    #[serde(default)]
    pub ocr: ProviderModelCapabilityProfile,
    #[serde(default)]
    pub image_input: ProviderModelCapabilityProfile,
    #[serde(default)]
    pub structured_output: ProviderModelCapabilityProfile,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ProviderModelCapabilityProfile {
    #[serde(default)]
    pub default_support: String,
    #[serde(default)]
    pub allow_patterns: Vec<String>,
    #[serde(default)]
    pub deny_patterns: Vec<String>,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderDiscoveredModel {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_support: Option<ProviderModelSupportStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_ocr: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ocr_support: Option<ProviderModelSupportStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_input_support: Option<ProviderModelSupportStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_output_support: Option<ProviderModelSupportStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability_source: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProviderModelsResponse {
    pub models: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub model_details: Vec<ProviderDiscoveredModel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}
