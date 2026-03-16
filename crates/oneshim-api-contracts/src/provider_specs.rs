use std::collections::HashSet;
use std::sync::OnceLock;

use oneshim_core::config::{AiAccessMode, AiProviderType};
use oneshim_core::provider_surface::{
    canonical_provider_surface_id, provider_type_from_vendor_id, provider_vendor_id_or_default,
};

use crate::ai_providers::{
    ProviderModelCapabilityRules, ProviderModelCatalogTransportSpec, ProviderModelSupportStatus,
    ProviderParameterProfile, ProviderParameterSet, ProviderTransportSpec,
};

const PROVIDER_SURFACE_SPECS_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/providers/provider-surface-catalog.json"
));

static SURFACE_CATALOG: OnceLock<Result<ProviderSurfaceCatalog, String>> = OnceLock::new();

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ProviderSurfaceCatalog {
    pub version: u32,
    #[serde(default)]
    pub updated_at: String,
    pub vendors: Vec<ProviderVendorSpec>,
    pub surfaces: Vec<ProviderSurfaceSpec>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ProviderVendorSpec {
    pub vendor_id: String,
    pub provider_type: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub display_name: String,
    #[serde(default)]
    pub projection: Option<ProviderVendorProjectionSpec>,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct ProviderVendorProjectionSpec {
    #[serde(default)]
    pub api_key_env_vars: Vec<String>,
    #[serde(default)]
    pub api_key_temp_file_prefix: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ProviderSurfaceSpec {
    pub surface_id: String,
    pub vendor_id: String,
    pub provider_type: String,
    pub display_name: String,
    pub execution_kind: String,
    pub placement_kind: String,
    pub credential_kind: String,
    pub stability: String,
    #[serde(default)]
    pub preferred_for_product_auth: bool,
    #[serde(default)]
    pub related_surface_ids: Vec<String>,
    #[serde(default)]
    pub catalog_strategy: String,
    pub supports: ProviderSurfaceSupports,
    #[serde(default)]
    pub llm_capabilities: ProviderLlmCapabilities,
    #[serde(default)]
    pub ocr_capabilities: ProviderOcrCapabilities,
    pub default_models: SurfaceDefaultModels,
    #[serde(default)]
    pub capability_rules: ProviderModelCapabilityRules,
    pub parameter_profiles: ProviderParameterSet,
    #[serde(default)]
    pub unknown_model_policy: ProviderUnknownModelPolicySet,
    #[serde(default)]
    pub known_models: Vec<ProviderKnownModelSpec>,
    #[serde(default)]
    pub llm_transport: Option<ProviderTransportSpec>,
    #[serde(default)]
    pub ocr_transport: Option<ProviderTransportSpec>,
    #[serde(default)]
    pub model_catalog_transport: Option<ProviderModelCatalogTransportSpec>,
    #[serde(default)]
    pub availability_probe: Option<ProviderAvailabilityProbeSpec>,
    #[serde(default)]
    pub subprocess_transport: Option<SubprocessTransportSpec>,
    #[serde(default)]
    pub provisioning: Option<ProviderSurfaceProvisioningSpec>,
    #[serde(default)]
    pub references: Vec<String>,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct ProviderSurfaceProvisioningSpec {
    #[serde(default)]
    pub configuration_env_vars: Vec<String>,
    #[serde(default)]
    pub setup_copy_key: Option<String>,
    #[serde(default)]
    pub docs_url: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ProviderSurfaceSupports {
    #[serde(default)]
    pub llm: bool,
    #[serde(default)]
    pub ocr: bool,
    #[serde(default)]
    pub model_catalog: bool,
    #[serde(default)]
    pub context_bridge: bool,
}

#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub struct ProviderLlmCapabilities {
    #[serde(default)]
    pub structured_output: bool,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ProviderOcrCapabilities {
    #[serde(default = "default_ocr_strategy")]
    pub strategy: String,
    #[serde(default)]
    pub supports_geometry: bool,
    #[serde(default)]
    pub supports_confidence: bool,
    #[serde(default)]
    pub requires_image_input_model: bool,
    #[serde(default)]
    pub requires_structured_output_model: bool,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SurfaceDefaultModels {
    #[serde(default)]
    pub llm_models: Vec<String>,
    #[serde(default)]
    pub ocr_models: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ProviderKnownModelSpec {
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub id_prefixes: Vec<String>,
    pub capabilities: ProviderKnownModelCapabilities,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ProviderKnownModelCapabilities {
    #[serde(default = "default_true")]
    pub llm: bool,
    #[serde(default)]
    pub ocr: bool,
    #[serde(default)]
    pub image_input: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderUnknownModelPolicy {
    Allow,
    Warn,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ProviderUnknownModelPolicySet {
    #[serde(default = "default_unknown_model_policy")]
    pub llm: ProviderUnknownModelPolicy,
    #[serde(default = "default_unknown_model_policy")]
    pub ocr: ProviderUnknownModelPolicy,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ProviderAvailabilityProbeSpec {
    pub method: String,
    pub url: String,
    pub auth_scheme: String,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SubprocessTransportSpec {
    pub tool_id: String,
    #[serde(default)]
    pub executable_candidates: Vec<String>,
    #[serde(default)]
    pub auth_probe_command: Vec<String>,
    pub auth_probe_mode: String,
    pub invocation_mode: String,
    #[serde(default)]
    pub model_flag: Option<String>,
    #[serde(default)]
    pub json_output_supported: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderTransportKind {
    Llm,
    Ocr,
    ModelCatalog,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderAuthScheme {
    None,
    Bearer,
    XApiKey,
    XGoogApiKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderRequestShape {
    AnthropicMessages,
    AnthropicVisionMessages,
    OpenAiChatCompletions,
    OpenAiVisionChatCompletions,
    OpenAiResponses,
    GoogleGenerateContent,
    GoogleVisionAnnotate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCatalogResponseShape {
    StandardDataOrModels,
    GoogleModels,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceCapabilityKind {
    Llm,
    Ocr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceModelCapabilityKind {
    Llm,
    Ocr,
    ImageInput,
    StructuredOutput,
}

impl From<SurfaceCapabilityKind> for SurfaceModelCapabilityKind {
    fn from(value: SurfaceCapabilityKind) -> Self {
        match value {
            SurfaceCapabilityKind::Llm => SurfaceModelCapabilityKind::Llm,
            SurfaceCapabilityKind::Ocr => SurfaceModelCapabilityKind::Ocr,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceExecutionKind {
    DirectHttp,
    ManagedHttp,
    SubprocessCli,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePlacementKind {
    ProviderHosted,
    SelfHosted,
    InstalledCli,
    CustomHosted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceStability {
    Ga,
    Preview,
    Experimental,
    Deprecated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCatalogStrategy {
    None,
    HttpModelsEndpoint,
    SubprocessProbe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubprocessInvocationMode {
    CodexExecJson,
    ClaudePrintJson,
    GeminiCliPrompt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubprocessAuthProbeMode {
    None,
    CodexLoginStatusText,
    ClaudeAuthStatusJson,
}

fn default_true() -> bool {
    true
}

fn default_unknown_model_policy() -> ProviderUnknownModelPolicy {
    ProviderUnknownModelPolicy::Warn
}

fn default_model_support_status(raw: &str) -> ProviderModelSupportStatus {
    match raw.trim().to_ascii_lowercase().as_str() {
        "supported" => ProviderModelSupportStatus::Supported,
        "unsupported" => ProviderModelSupportStatus::Unsupported,
        _ => ProviderModelSupportStatus::Unknown,
    }
}

fn default_ocr_strategy() -> String {
    "none".to_string()
}

impl Default for ProviderUnknownModelPolicySet {
    fn default() -> Self {
        Self {
            llm: default_unknown_model_policy(),
            ocr: default_unknown_model_policy(),
        }
    }
}

impl Default for ProviderOcrCapabilities {
    fn default() -> Self {
        Self {
            strategy: default_ocr_strategy(),
            supports_geometry: false,
            supports_confidence: false,
            requires_image_input_model: false,
            requires_structured_output_model: false,
        }
    }
}

pub fn list_provider_surface_specs() -> Result<ProviderSurfaceCatalog, String> {
    Ok(surface_catalog()?.clone())
}

pub fn provider_surface_catalog() -> Result<&'static ProviderSurfaceCatalog, String> {
    surface_catalog()
}

pub fn resolve_provider_type(raw: &str) -> Option<AiProviderType> {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }

    let catalog = surface_catalog().ok()?;
    for vendor in &catalog.vendors {
        let canonical = vendor.provider_type.to_ascii_lowercase();
        if canonical == normalized
            || vendor
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(&normalized))
        {
            if let Some(parsed) = parse_provider_type_name(&vendor.provider_type) {
                return Some(parsed);
            }
        }
    }

    parse_provider_type_name(&normalized)
}

pub fn provider_surface_spec(surface_id: &str) -> Result<&'static ProviderSurfaceSpec, String> {
    let normalized = surface_id.trim().to_ascii_lowercase();
    surface_catalog()?
        .surfaces
        .iter()
        .find(|surface| surface.surface_id.eq_ignore_ascii_case(&normalized))
        .ok_or_else(|| format!("Provider surface spec for {surface_id} is missing."))
}

pub fn resolved_surface_spec(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
) -> Result<&'static ProviderSurfaceSpec, String> {
    if let Some(surface_id) = surface_id.map(str::trim).filter(|value| !value.is_empty()) {
        let surface = provider_surface_spec(surface_id)?;
        let expected = provider_vendor_id_or_default(provider_type);
        if provider_type_from_vendor_id(&surface.provider_type) != Some(provider_type) {
            return Err(format!(
                "Surface '{}' does not match provider_type '{}'.",
                surface_id, expected
            ));
        }
        return Ok(surface);
    }

    default_direct_surface_spec(provider_type)
}

pub fn default_surface_id_for_access_mode(
    provider_type: AiProviderType,
    access_mode: AiAccessMode,
    capability: SurfaceCapabilityKind,
) -> Result<Option<&'static str>, String> {
    let execution_kind = match access_mode.normalized_for_ai_surfaces() {
        AiAccessMode::ProviderOAuth => SurfaceExecutionKind::ManagedHttp,
        AiAccessMode::ProviderSubscriptionCli => SurfaceExecutionKind::SubprocessCli,
        AiAccessMode::ProviderApiKey | AiAccessMode::LocalModel => SurfaceExecutionKind::DirectHttp,
        AiAccessMode::PlatformConnected => unreachable!("legacy access mode should normalize"),
    };

    let mut candidates = surface_catalog()?
        .surfaces
        .iter()
        .filter(|surface| {
            provider_type_from_vendor_id(&surface.provider_type) == Some(provider_type)
        })
        .filter(|surface| {
            parse_surface_execution_kind(&surface.execution_kind).ok() == Some(execution_kind)
        })
        .filter(|surface| match capability {
            SurfaceCapabilityKind::Llm => surface.supports.llm,
            SurfaceCapabilityKind::Ocr => surface.supports.ocr,
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        right
            .preferred_for_product_auth
            .cmp(&left.preferred_for_product_auth)
            .then_with(|| {
                stability_sort_key(&right.stability).cmp(&stability_sort_key(&left.stability))
            })
            .then_with(|| left.display_name.cmp(&right.display_name))
    });

    Ok(candidates
        .first()
        .map(|surface| surface.surface_id.as_str()))
}

pub fn transport_spec(
    provider_type: AiProviderType,
    kind: ProviderTransportKind,
) -> Result<&'static ProviderTransportSpec, String> {
    resolved_transport_spec(provider_type, None, kind)
}

pub fn resolved_transport_spec(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
    kind: ProviderTransportKind,
) -> Result<&'static ProviderTransportSpec, String> {
    let surface = resolved_surface_spec(provider_type, surface_id)?;
    match kind {
        ProviderTransportKind::Llm => surface.llm_transport.as_ref().ok_or_else(|| {
            format!(
                "Surface '{}' does not provide an llm_transport.",
                surface.surface_id
            )
        }),
        ProviderTransportKind::Ocr => surface.ocr_transport.as_ref().ok_or_else(|| {
            format!(
                "Surface '{}' does not provide an ocr_transport.",
                surface.surface_id
            )
        }),
        ProviderTransportKind::ModelCatalog => Err(
            "Model catalog transport uses a dedicated shape and must be resolved separately."
                .to_string(),
        ),
    }
}

pub fn auth_scheme(
    provider_type: AiProviderType,
    kind: ProviderTransportKind,
) -> Result<ProviderAuthScheme, String> {
    resolved_auth_scheme(provider_type, None, kind)
}

pub fn resolved_auth_scheme(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
    kind: ProviderTransportKind,
) -> Result<ProviderAuthScheme, String> {
    let raw = match kind {
        ProviderTransportKind::Llm | ProviderTransportKind::Ocr => {
            resolved_transport_spec(provider_type, surface_id, kind)?
                .auth_scheme
                .as_str()
        }
        ProviderTransportKind::ModelCatalog => {
            resolved_model_catalog_transport(provider_type, surface_id)?
                .auth_scheme
                .as_str()
        }
    };
    parse_auth_scheme(raw)
}

pub fn request_shape(
    provider_type: AiProviderType,
    kind: ProviderTransportKind,
) -> Result<ProviderRequestShape, String> {
    resolved_request_shape(provider_type, None, kind)
}

pub fn resolved_request_shape(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
    kind: ProviderTransportKind,
) -> Result<ProviderRequestShape, String> {
    parse_request_shape(&resolved_transport_spec(provider_type, surface_id, kind)?.request_shape)
}

pub fn model_catalog_response_shape(
    provider_type: AiProviderType,
) -> Result<ModelCatalogResponseShape, String> {
    resolved_model_catalog_response_shape(provider_type, None)
}

pub fn resolved_model_catalog_transport(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
) -> Result<&'static ProviderModelCatalogTransportSpec, String> {
    let surface = resolved_surface_spec(provider_type, surface_id)?;
    surface.model_catalog_transport.as_ref().ok_or_else(|| {
        format!(
            "Surface '{}' does not provide a model_catalog_transport.",
            surface.surface_id
        )
    })
}

pub fn resolved_model_catalog_response_shape(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
) -> Result<ModelCatalogResponseShape, String> {
    let raw = resolved_model_catalog_transport(provider_type, surface_id)?
        .response_shape
        .trim()
        .to_ascii_lowercase();
    match raw.as_str() {
        "standard_data_or_models" => Ok(ModelCatalogResponseShape::StandardDataOrModels),
        "google_models" => Ok(ModelCatalogResponseShape::GoogleModels),
        _ => Err(format!(
            "Unsupported model catalog response shape '{raw}' for {}",
            provider_vendor_id_or_default(provider_type)
        )),
    }
}

pub fn default_llm_model(provider_type: AiProviderType) -> Result<Option<String>, String> {
    resolved_default_model(provider_type, None, SurfaceCapabilityKind::Llm)
}

pub fn default_ocr_model(provider_type: AiProviderType) -> Result<Option<String>, String> {
    resolved_default_model(provider_type, None, SurfaceCapabilityKind::Ocr)
}

pub fn default_surface_model(
    surface_id: &str,
    capability: SurfaceCapabilityKind,
) -> Result<Option<String>, String> {
    let surface = provider_surface_spec(surface_id)?;
    Ok(match capability {
        SurfaceCapabilityKind::Llm => surface.default_models.llm_models.first().cloned(),
        SurfaceCapabilityKind::Ocr => surface.default_models.ocr_models.first().cloned(),
    })
}

pub fn resolved_default_model(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
    capability: SurfaceCapabilityKind,
) -> Result<Option<String>, String> {
    let surface = resolved_surface_spec(provider_type, surface_id)?;
    Ok(match capability {
        SurfaceCapabilityKind::Llm => surface.default_models.llm_models.first().cloned(),
        SurfaceCapabilityKind::Ocr => surface.default_models.ocr_models.first().cloned(),
    })
}

pub fn parse_surface_execution_kind(raw: &str) -> Result<SurfaceExecutionKind, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "direct_http" => Ok(SurfaceExecutionKind::DirectHttp),
        "managed_http" => Ok(SurfaceExecutionKind::ManagedHttp),
        "subprocess_cli" => Ok(SurfaceExecutionKind::SubprocessCli),
        other => Err(format!("Unsupported surface execution kind '{other}'.")),
    }
}

pub fn parse_surface_placement_kind(raw: &str) -> Result<SurfacePlacementKind, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "provider_hosted" => Ok(SurfacePlacementKind::ProviderHosted),
        "self_hosted" => Ok(SurfacePlacementKind::SelfHosted),
        "installed_cli" => Ok(SurfacePlacementKind::InstalledCli),
        "custom_hosted" => Ok(SurfacePlacementKind::CustomHosted),
        other => Err(format!(
            "Unsupported provider surface placement_kind '{other}'."
        )),
    }
}

pub fn parse_surface_stability(raw: &str) -> Result<SurfaceStability, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "ga" => Ok(SurfaceStability::Ga),
        "preview" => Ok(SurfaceStability::Preview),
        "experimental" => Ok(SurfaceStability::Experimental),
        "deprecated" => Ok(SurfaceStability::Deprecated),
        other => Err(format!("Unsupported surface stability '{other}'.")),
    }
}

pub fn parse_model_catalog_strategy(raw: &str) -> Result<ModelCatalogStrategy, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "none" => Ok(ModelCatalogStrategy::None),
        "http_models_endpoint" => Ok(ModelCatalogStrategy::HttpModelsEndpoint),
        "subprocess_probe" => Ok(ModelCatalogStrategy::SubprocessProbe),
        other => Err(format!("Unsupported model catalog strategy '{other}'.")),
    }
}

pub fn model_catalog_strategy(surface_id: &str) -> Result<ModelCatalogStrategy, String> {
    let surface = provider_surface_spec(surface_id)?;
    parse_model_catalog_strategy(&surface.catalog_strategy)
}

pub fn resolved_model_catalog_strategy(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
) -> Result<ModelCatalogStrategy, String> {
    let surface = resolved_surface_spec(provider_type, surface_id)?;
    parse_model_catalog_strategy(&surface.catalog_strategy)
}

pub fn parse_subprocess_invocation_mode(raw: &str) -> Result<SubprocessInvocationMode, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "codex_exec_json" => Ok(SubprocessInvocationMode::CodexExecJson),
        "claude_print_json" => Ok(SubprocessInvocationMode::ClaudePrintJson),
        "gemini_cli_prompt" => Ok(SubprocessInvocationMode::GeminiCliPrompt),
        other => Err(format!("Unsupported subprocess invocation mode '{other}'.")),
    }
}

pub fn parse_subprocess_auth_probe_mode(raw: &str) -> Result<SubprocessAuthProbeMode, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "none" => Ok(SubprocessAuthProbeMode::None),
        "codex_login_status_text" => Ok(SubprocessAuthProbeMode::CodexLoginStatusText),
        "claude_auth_status_json" => Ok(SubprocessAuthProbeMode::ClaudeAuthStatusJson),
        other => Err(format!("Unsupported subprocess auth probe mode '{other}'.")),
    }
}

pub fn surface_stability(surface_id: &str) -> Result<SurfaceStability, String> {
    let surface = provider_surface_spec(surface_id)?;
    parse_surface_stability(&surface.stability)
}

pub fn subprocess_transport(surface_id: &str) -> Result<&'static SubprocessTransportSpec, String> {
    let surface = provider_surface_spec(surface_id)?;
    match parse_surface_execution_kind(&surface.execution_kind)? {
        SurfaceExecutionKind::SubprocessCli => {
            surface.subprocess_transport.as_ref().ok_or_else(|| {
                format!(
                    "Surface '{}' uses subprocess_cli but is missing subprocess_transport.",
                    surface.surface_id
                )
            })
        }
        _ => Err(format!(
            "Surface '{}' is not a subprocess_cli surface.",
            surface.surface_id
        )),
    }
}

pub fn subprocess_supports_json_output(surface_id: &str) -> Result<bool, String> {
    Ok(subprocess_transport(surface_id)?.json_output_supported)
}

pub fn subprocess_invocation_mode(surface_id: &str) -> Result<SubprocessInvocationMode, String> {
    parse_subprocess_invocation_mode(&subprocess_transport(surface_id)?.invocation_mode)
}

pub fn subprocess_auth_probe_mode(surface_id: &str) -> Result<SubprocessAuthProbeMode, String> {
    parse_subprocess_auth_probe_mode(&subprocess_transport(surface_id)?.auth_probe_mode)
}

pub fn surface_supports_capability(
    surface_id: &str,
    capability: SurfaceCapabilityKind,
) -> Result<bool, String> {
    let surface = provider_surface_spec(surface_id)?;
    Ok(match capability {
        SurfaceCapabilityKind::Llm => surface.supports.llm,
        SurfaceCapabilityKind::Ocr => surface.supports.ocr,
    })
}

pub fn list_subprocess_surface_specs() -> Result<Vec<&'static ProviderSurfaceSpec>, String> {
    Ok(surface_catalog()?
        .surfaces
        .iter()
        .filter(|surface| {
            matches!(
                parse_surface_execution_kind(&surface.execution_kind),
                Ok(SurfaceExecutionKind::SubprocessCli)
            )
        })
        .collect())
}

pub fn resolved_parameter_profile(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
    capability: SurfaceCapabilityKind,
) -> Result<&'static ProviderParameterProfile, String> {
    let surface = resolved_surface_spec(provider_type, surface_id)?;
    Ok(match capability {
        SurfaceCapabilityKind::Llm => &surface.parameter_profiles.llm,
        SurfaceCapabilityKind::Ocr => &surface.parameter_profiles.ocr,
    })
}

pub fn parameter_profile_for_surface(
    surface_id: &str,
    capability: SurfaceCapabilityKind,
) -> Result<&'static ProviderParameterProfile, String> {
    let surface = provider_surface_spec(surface_id)?;
    Ok(match capability {
        SurfaceCapabilityKind::Llm => &surface.parameter_profiles.llm,
        SurfaceCapabilityKind::Ocr => &surface.parameter_profiles.ocr,
    })
}

pub fn validate_supported_parameters(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
    capability: SurfaceCapabilityKind,
    parameters: &[&str],
) -> Result<(), String> {
    let profile = resolved_parameter_profile(provider_type, surface_id, capability)?;
    validate_parameter_usage(profile, parameters)
}

pub fn validate_supported_surface_parameters(
    surface_id: &str,
    capability: SurfaceCapabilityKind,
    parameters: &[&str],
) -> Result<(), String> {
    let profile = parameter_profile_for_surface(surface_id, capability)?;
    validate_parameter_usage(profile, parameters)
}

pub fn surface_supports_parameter(
    surface_id: &str,
    capability: SurfaceCapabilityKind,
    parameter: &str,
) -> Result<bool, String> {
    let normalized = parameter.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok(false);
    }

    let profile = parameter_profile_for_surface(surface_id, capability)?;
    Ok(profile
        .supported
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(&normalized)))
}

pub fn surface_supports_model_selection(
    surface_id: &str,
    capability: SurfaceCapabilityKind,
) -> Result<bool, String> {
    let surface = provider_surface_spec(surface_id)?;
    Ok(surface_declares_model_selection(surface, capability))
}

pub fn resolved_surface_supports_model_selection(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
    capability: SurfaceCapabilityKind,
) -> Result<bool, String> {
    let surface = resolved_surface_spec(provider_type, surface_id)?;
    surface_supports_model_selection(&surface.surface_id, capability)
}

pub fn resolved_surface_supports_parameter(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
    capability: SurfaceCapabilityKind,
    parameter: &str,
) -> Result<bool, String> {
    let surface = resolved_surface_spec(provider_type, surface_id)?;
    surface_supports_parameter(&surface.surface_id, capability, parameter)
}

pub fn availability_probe(
    surface_id: &str,
) -> Result<Option<&'static ProviderAvailabilityProbeSpec>, String> {
    Ok(provider_surface_spec(surface_id)?
        .availability_probe
        .as_ref())
}

pub fn known_model_spec_for_surface(
    surface_id: &str,
    model_id: &str,
) -> Result<Option<&'static ProviderKnownModelSpec>, String> {
    let normalized = model_id.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok(None);
    }

    let surface = provider_surface_spec(surface_id)?;
    Ok(surface
        .known_models
        .iter()
        .find(|model| known_model_matches(model, &normalized)))
}

pub fn model_capability_status_for_surface(
    surface_id: &str,
    capability: SurfaceModelCapabilityKind,
    model_id: &str,
) -> Result<ProviderModelSupportStatus, String> {
    let normalized = model_id.trim();
    if normalized.is_empty() {
        return Ok(ProviderModelSupportStatus::Unknown);
    }

    let surface = provider_surface_spec(surface_id)?;
    if let Some(model) = known_model_spec_for_surface(&surface.surface_id, normalized)? {
        let explicit = match capability {
            SurfaceModelCapabilityKind::Llm => Some(model.capabilities.llm),
            SurfaceModelCapabilityKind::Ocr => Some(model.capabilities.ocr),
            SurfaceModelCapabilityKind::ImageInput => Some(model.capabilities.image_input),
            SurfaceModelCapabilityKind::StructuredOutput => None,
        };
        if let Some(supported) = explicit {
            return Ok(if supported {
                ProviderModelSupportStatus::Supported
            } else {
                ProviderModelSupportStatus::Unsupported
            });
        }
    }

    let profile = match capability {
        SurfaceModelCapabilityKind::Llm => &surface.capability_rules.llm,
        SurfaceModelCapabilityKind::Ocr => &surface.capability_rules.ocr,
        SurfaceModelCapabilityKind::ImageInput => &surface.capability_rules.image_input,
        SurfaceModelCapabilityKind::StructuredOutput => &surface.capability_rules.structured_output,
    };

    capability_status_from_profile(profile, normalized)
}

pub fn resolved_model_capability_status(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
    capability: SurfaceModelCapabilityKind,
    model_id: &str,
) -> Result<ProviderModelSupportStatus, String> {
    let surface = resolved_surface_spec(provider_type, surface_id)?;
    model_capability_status_for_surface(&surface.surface_id, capability, model_id)
}

pub fn surface_requires_explicit_model_selection(
    surface_id: &str,
    capability: SurfaceCapabilityKind,
) -> Result<bool, String> {
    Ok(surface_supports_model_selection(surface_id, capability)?
        && default_surface_model(surface_id, capability)?
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .is_none())
}

pub fn resolved_surface_requires_explicit_model_selection(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
    capability: SurfaceCapabilityKind,
) -> Result<bool, String> {
    let surface = resolved_surface_spec(provider_type, surface_id)?;
    surface_requires_explicit_model_selection(&surface.surface_id, capability)
}

pub fn ocr_requires_structured_output_model(surface_id: &str) -> Result<bool, String> {
    Ok(provider_surface_spec(surface_id)?
        .ocr_capabilities
        .requires_structured_output_model)
}

pub fn resolved_ocr_requires_structured_output_model(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
) -> Result<bool, String> {
    let surface = resolved_surface_spec(provider_type, surface_id)?;
    ocr_requires_structured_output_model(&surface.surface_id)
}

pub fn validate_known_model_capability(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
    capability: SurfaceCapabilityKind,
    model_id: &str,
) -> Result<(), String> {
    let normalized = model_id.trim();
    if normalized.is_empty() {
        return Ok(());
    }

    let surface = resolved_surface_spec(provider_type, surface_id)?;
    let support =
        model_capability_status_for_surface(&surface.surface_id, capability.into(), normalized)?;
    if support == ProviderModelSupportStatus::Supported {
        return Ok(());
    }
    if support == ProviderModelSupportStatus::Unknown {
        let policy = unknown_model_policy_for_surface(&surface.surface_id, capability)?;
        return match policy {
            ProviderUnknownModelPolicy::Allow | ProviderUnknownModelPolicy::Warn => Ok(()),
            ProviderUnknownModelPolicy::Reject => {
                let capability_label = match capability {
                    SurfaceCapabilityKind::Llm => "LLM",
                    SurfaceCapabilityKind::Ocr => "OCR",
                };
                let replacement = default_surface_model(&surface.surface_id, capability)?
                    .unwrap_or_else(|| "a compatible default model".to_string());
                Err(format!(
                    "Model '{}' is not catalogued for {} surface '{}'. Choose a known compatible model such as '{}'.",
                    normalized, capability_label, surface.surface_id, replacement
                ))
            }
        };
    }

    let capability_label = match capability {
        SurfaceCapabilityKind::Llm => "LLM",
        SurfaceCapabilityKind::Ocr => "OCR",
    };
    let replacement = default_surface_model(&surface.surface_id, capability)?
        .unwrap_or_else(|| "a compatible default model".to_string());

    Err(format!(
        "Model '{}' is not marked as {}-capable for surface '{}'. Choose a compatible model such as '{}'.",
        normalized, capability_label, surface.surface_id, replacement
    ))
}

pub fn known_model_capability_warning(
    provider_type: AiProviderType,
    surface_id: Option<&str>,
    capability: SurfaceCapabilityKind,
    model_id: &str,
) -> Result<Option<String>, String> {
    let normalized = model_id.trim();
    if normalized.is_empty() {
        return Ok(None);
    }

    let surface = resolved_surface_spec(provider_type, surface_id)?;
    if model_capability_status_for_surface(&surface.surface_id, capability.into(), normalized)?
        != ProviderModelSupportStatus::Unknown
    {
        return Ok(None);
    }

    if unknown_model_policy_for_surface(&surface.surface_id, capability)?
        != ProviderUnknownModelPolicy::Warn
    {
        return Ok(None);
    }

    let capability_label = match capability {
        SurfaceCapabilityKind::Llm => "LLM",
        SurfaceCapabilityKind::Ocr => "OCR",
    };

    Ok(Some(format!(
        "Model '{}' is not catalogued for {} surface '{}'. Continuing because this surface allows unknown models with a warning.",
        normalized, capability_label, surface.surface_id
    )))
}

pub fn unknown_model_policy_for_surface(
    surface_id: &str,
    capability: SurfaceCapabilityKind,
) -> Result<ProviderUnknownModelPolicy, String> {
    let surface = provider_surface_spec(surface_id)?;
    Ok(match capability {
        SurfaceCapabilityKind::Llm => surface.unknown_model_policy.llm,
        SurfaceCapabilityKind::Ocr => surface.unknown_model_policy.ocr,
    })
}

fn surface_catalog() -> Result<&'static ProviderSurfaceCatalog, String> {
    match SURFACE_CATALOG.get_or_init(load_surface_catalog) {
        Ok(catalog) => Ok(catalog),
        Err(message) => Err(message.clone()),
    }
}

fn known_model_matches(model: &ProviderKnownModelSpec, normalized_model_id: &str) -> bool {
    if model.id.eq_ignore_ascii_case(normalized_model_id) {
        return true;
    }

    if model
        .aliases
        .iter()
        .any(|alias| alias.eq_ignore_ascii_case(normalized_model_id))
    {
        return true;
    }

    model.id_prefixes.iter().any(|prefix| {
        let normalized_prefix = prefix.trim().to_ascii_lowercase();
        !normalized_prefix.is_empty()
            && (normalized_model_id == normalized_prefix
                || normalized_model_id.starts_with(&normalized_prefix))
    })
}

fn capability_status_from_profile(
    profile: &crate::ai_providers::ProviderModelCapabilityProfile,
    model_id: &str,
) -> Result<ProviderModelSupportStatus, String> {
    let normalized = model_id.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok(ProviderModelSupportStatus::Unknown);
    }

    for pattern in &profile.deny_patterns {
        if model_pattern_matches(pattern, &normalized)? {
            return Ok(ProviderModelSupportStatus::Unsupported);
        }
    }

    for pattern in &profile.allow_patterns {
        if model_pattern_matches(pattern, &normalized)? {
            return Ok(ProviderModelSupportStatus::Supported);
        }
    }

    Ok(default_model_support_status(&profile.default_support))
}

fn model_pattern_matches(pattern: &str, normalized_model_id: &str) -> Result<bool, String> {
    let normalized_pattern = pattern.trim().to_ascii_lowercase();
    if normalized_pattern.is_empty() {
        return Ok(false);
    }

    if normalized_pattern == "*" {
        return Ok(true);
    }

    if !normalized_pattern.contains('*') {
        return Ok(normalized_model_id == normalized_pattern);
    }

    let parts = normalized_pattern.split('*').collect::<Vec<_>>();
    let starts_with_wildcard = normalized_pattern.starts_with('*');
    let ends_with_wildcard = normalized_pattern.ends_with('*');
    let mut search_start = 0usize;

    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }

        if index == 0 && !starts_with_wildcard {
            if !normalized_model_id[search_start..].starts_with(part) {
                return Ok(false);
            }
            search_start += part.len();
            continue;
        }

        let haystack = &normalized_model_id[search_start..];
        let Some(found_at) = haystack.find(part) else {
            return Ok(false);
        };
        search_start += found_at + part.len();
    }

    if !ends_with_wildcard {
        if let Some(last_part) = parts.iter().rev().find(|part| !part.is_empty()) {
            return Ok(normalized_model_id.ends_with(last_part));
        }
    }

    Ok(true)
}

fn load_surface_catalog() -> Result<ProviderSurfaceCatalog, String> {
    let catalog = serde_json::from_str::<ProviderSurfaceCatalog>(PROVIDER_SURFACE_SPECS_JSON)
        .map_err(|e| format!("Failed to parse provider surface catalog: {e}"))?;
    validate_surface_catalog(&catalog)?;
    Ok(catalog)
}

fn validate_surface_catalog(catalog: &ProviderSurfaceCatalog) -> Result<(), String> {
    if catalog.vendors.is_empty() {
        return Err("Provider surface catalog must contain at least one vendor.".to_string());
    }
    if catalog.surfaces.is_empty() {
        return Err("Provider surface catalog must contain at least one surface.".to_string());
    }

    let mut vendor_ids = HashSet::new();
    let mut surface_ids = HashSet::new();
    let mut aliases = HashSet::new();

    for vendor in &catalog.vendors {
        let vendor_id = vendor.vendor_id.trim().to_ascii_lowercase();
        if vendor_id.is_empty() {
            return Err("Provider surface catalog contains an empty vendor_id.".to_string());
        }
        if !vendor_ids.insert(vendor_id.clone()) {
            return Err(format!(
                "Duplicate vendor_id '{vendor_id}' in provider surface catalog."
            ));
        }
        if vendor.display_name.trim().is_empty() {
            return Err(format!(
                "Vendor '{}' is missing a display_name.",
                vendor.vendor_id
            ));
        }

        parse_provider_type(&vendor.provider_type)?;
        if let Some(projection) = vendor.projection.as_ref() {
            if projection
                .api_key_env_vars
                .iter()
                .any(|value| value.trim().is_empty())
            {
                return Err(format!(
                    "Vendor '{}' projection contains an empty api_key_env_vars entry.",
                    vendor.vendor_id
                ));
            }
            if projection
                .api_key_temp_file_prefix
                .as_deref()
                .map(str::trim)
                .is_some_and(str::is_empty)
            {
                return Err(format!(
                    "Vendor '{}' projection api_key_temp_file_prefix cannot be empty.",
                    vendor.vendor_id
                ));
            }
        }

        let provider_key = vendor.provider_type.trim().to_ascii_lowercase();
        for alias in &vendor.aliases {
            let normalized = alias.trim().to_ascii_lowercase();
            if normalized.is_empty() {
                return Err(format!(
                    "Vendor '{}' contains an empty alias.",
                    vendor.vendor_id
                ));
            }
            if normalized == provider_key {
                continue;
            }
            if !aliases.insert(normalized.clone()) {
                return Err(format!(
                    "Provider surface alias '{}' is defined more than once.",
                    alias
                ));
            }
        }
    }

    for surface in &catalog.surfaces {
        let surface_id = surface.surface_id.trim().to_ascii_lowercase();
        if surface_id.is_empty() {
            return Err("Provider surface catalog contains an empty surface_id.".to_string());
        }
        if !surface_ids.insert(surface_id.clone()) {
            return Err(format!(
                "Duplicate surface_id '{}' in provider surface catalog.",
                surface.surface_id
            ));
        }
        if canonical_provider_surface_id(&surface.surface_id).is_none() {
            return Err(format!(
                "Unknown provider surface id '{}' is not registered in oneshim-core.",
                surface.surface_id
            ));
        }
        if !vendor_ids.contains(&surface.vendor_id.trim().to_ascii_lowercase()) {
            return Err(format!(
                "Surface '{}' references unknown vendor_id '{}'.",
                surface.surface_id, surface.vendor_id
            ));
        }
        if surface.display_name.trim().is_empty() {
            return Err(format!(
                "Surface '{}' is missing a display_name.",
                surface.surface_id
            ));
        }
        parse_surface_placement_kind(&surface.placement_kind)?;
        parse_surface_stability(&surface.stability)?;
        if surface.references.is_empty() {
            return Err(format!(
                "Surface '{}' must include at least one reference URL.",
                surface.surface_id
            ));
        }
        for model in &surface.known_models {
            if model.id.trim().is_empty() {
                return Err(format!(
                    "Surface '{}' contains a known model with an empty id.",
                    surface.surface_id
                ));
            }
            if model
                .display_name
                .as_deref()
                .is_some_and(|value| value.trim().is_empty())
            {
                return Err(format!(
                    "Surface '{}' known model '{}' has an empty display_name.",
                    surface.surface_id, model.id
                ));
            }
            if model.aliases.iter().any(|alias| alias.trim().is_empty()) {
                return Err(format!(
                    "Surface '{}' known model '{}' contains an empty alias.",
                    surface.surface_id, model.id
                ));
            }
            if model
                .id_prefixes
                .iter()
                .any(|prefix| prefix.trim().is_empty())
            {
                return Err(format!(
                    "Surface '{}' known model '{}' contains an empty id_prefix.",
                    surface.surface_id, model.id
                ));
            }
        }
        if surface
            .related_surface_ids
            .iter()
            .any(|value| value.trim().is_empty())
        {
            return Err(format!(
                "Surface '{}' contains an empty related_surface_id.",
                surface.surface_id
            ));
        }
        if surface
            .related_surface_ids
            .iter()
            .any(|value| value.eq_ignore_ascii_case(&surface.surface_id))
        {
            return Err(format!(
                "Surface '{}' cannot reference itself in related_surface_ids.",
                surface.surface_id
            ));
        }

        let surface_provider_type = parse_provider_type(&surface.provider_type)?;
        let vendor_provider_type = catalog
            .vendors
            .iter()
            .find(|vendor| vendor.vendor_id.eq_ignore_ascii_case(&surface.vendor_id))
            .ok_or_else(|| {
                format!(
                    "Surface '{}' references unknown vendor_id '{}'.",
                    surface.surface_id, surface.vendor_id
                )
            })
            .and_then(|vendor| parse_provider_type(&vendor.provider_type))?;

        if surface_provider_type != vendor_provider_type {
            return Err(format!(
                "Surface '{}' provider_type '{}' does not match vendor '{}'.",
                surface.surface_id, surface.provider_type, surface.vendor_id
            ));
        }

        validate_parameter_profile(&surface.parameter_profiles.llm)?;
        validate_parameter_profile(&surface.parameter_profiles.ocr)?;
        validate_model_capability_profile(&surface.capability_rules.llm)?;
        validate_model_capability_profile(&surface.capability_rules.ocr)?;
        validate_model_capability_profile(&surface.capability_rules.image_input)?;
        validate_model_capability_profile(&surface.capability_rules.structured_output)?;
        let catalog_strategy = parse_model_catalog_strategy(&surface.catalog_strategy)?;

        if let Some(provisioning) = surface.provisioning.as_ref() {
            if provisioning
                .configuration_env_vars
                .iter()
                .any(|value| value.trim().is_empty())
            {
                return Err(format!(
                    "Surface '{}' provisioning contains an empty configuration_env_var entry.",
                    surface.surface_id
                ));
            }
            if provisioning
                .setup_copy_key
                .as_deref()
                .map(str::trim)
                .is_some_and(str::is_empty)
            {
                return Err(format!(
                    "Surface '{}' provisioning setup_copy_key cannot be empty.",
                    surface.surface_id
                ));
            }
            if provisioning
                .docs_url
                .as_deref()
                .map(str::trim)
                .is_some_and(str::is_empty)
            {
                return Err(format!(
                    "Surface '{}' provisioning docs_url cannot be empty.",
                    surface.surface_id
                ));
            }
        }

        if surface.supports.llm
            && surface.default_models.llm_models.is_empty()
            && !surface_declares_model_selection(surface, SurfaceCapabilityKind::Llm)
        {
            return Err(format!(
                "Surface '{}' supports LLM but defines neither default llm_models nor any LLM model-selection strategy.",
                surface.surface_id
            ));
        }

        match parse_surface_execution_kind(&surface.execution_kind)? {
            SurfaceExecutionKind::DirectHttp | SurfaceExecutionKind::ManagedHttp => {
                if surface.supports.llm {
                    let transport = surface.llm_transport.as_ref().ok_or_else(|| {
                        format!(
                            "Surface '{}' supports LLM but is missing llm_transport.",
                            surface.surface_id
                        )
                    })?;
                    validate_transport_spec(
                        &surface.surface_id,
                        "llm",
                        &transport.url,
                        &transport.auth_scheme,
                        Some(&transport.request_shape),
                    )?;
                }
                if surface.supports.ocr {
                    let transport = surface.ocr_transport.as_ref().ok_or_else(|| {
                        format!(
                            "Surface '{}' supports OCR but is missing ocr_transport.",
                            surface.surface_id
                        )
                    })?;
                    validate_transport_spec(
                        &surface.surface_id,
                        "ocr",
                        &transport.url,
                        &transport.auth_scheme,
                        Some(&transport.request_shape),
                    )?;
                }
                if surface.supports.model_catalog {
                    if catalog_strategy != ModelCatalogStrategy::HttpModelsEndpoint {
                        return Err(format!(
                            "Surface '{}' must use catalog_strategy='http_models_endpoint' for direct or managed HTTP model discovery.",
                            surface.surface_id
                        ));
                    }
                    let transport = surface.model_catalog_transport.as_ref().ok_or_else(|| {
                        format!(
                            "Surface '{}' supports model_catalog but is missing model_catalog_transport.",
                            surface.surface_id
                        )
                    })?;
                    validate_transport_spec(
                        &surface.surface_id,
                        "model_catalog",
                        &transport.url,
                        &transport.auth_scheme,
                        None,
                    )?;

                    let response_shape = transport.response_shape.trim().to_ascii_lowercase();
                    match response_shape.as_str() {
                        "standard_data_or_models" | "google_models" => {}
                        _ => {
                            return Err(format!(
                                "Surface '{}' has unsupported model catalog response shape '{}'.",
                                surface.surface_id, transport.response_shape
                            ))
                        }
                    }

                    if !transport.ocr_supported
                        && transport
                            .ocr_notice
                            .as_deref()
                            .map(str::trim)
                            .unwrap_or("")
                            .is_empty()
                    {
                        return Err(format!(
                            "Surface '{}' must include an OCR notice when model catalog OCR is unsupported.",
                            surface.surface_id
                        ));
                    }
                }
                if let Some(probe) = surface.availability_probe.as_ref() {
                    validate_transport_spec(
                        &surface.surface_id,
                        "availability_probe",
                        &probe.url,
                        &probe.auth_scheme,
                        None,
                    )?;
                    let method = probe.method.trim().to_ascii_uppercase();
                    if method != "GET" && method != "HEAD" {
                        return Err(format!(
                            "Surface '{}' availability_probe method '{}' is unsupported.",
                            surface.surface_id, probe.method
                        ));
                    }
                }
            }
            SurfaceExecutionKind::SubprocessCli => {
                if surface.supports.model_catalog
                    && catalog_strategy != ModelCatalogStrategy::SubprocessProbe
                {
                    return Err(format!(
                        "Subprocess surface '{}' must use catalog_strategy='subprocess_probe' when model_catalog is enabled.",
                        surface.surface_id
                    ));
                }
                let subprocess = surface.subprocess_transport.as_ref().ok_or_else(|| {
                    format!(
                        "Surface '{}' uses subprocess_cli but is missing subprocess_transport.",
                        surface.surface_id
                    )
                })?;
                if subprocess.tool_id.trim().is_empty() {
                    return Err(format!(
                        "Subprocess surface '{}' must declare a non-empty tool_id.",
                        surface.surface_id
                    ));
                }
                if subprocess.executable_candidates.is_empty() {
                    return Err(format!(
                        "Subprocess surface '{}' must declare executable_candidates.",
                        surface.surface_id
                    ));
                }
                let auth_probe_mode =
                    parse_subprocess_auth_probe_mode(&subprocess.auth_probe_mode)?;
                parse_subprocess_invocation_mode(&subprocess.invocation_mode)?;
                if auth_probe_mode != SubprocessAuthProbeMode::None
                    && subprocess.auth_probe_command.is_empty()
                {
                    return Err(format!(
                        "Subprocess surface '{}' must declare auth_probe_command when auth_probe_mode is enabled.",
                        surface.surface_id
                    ));
                }
            }
        }
    }

    for surface in &catalog.surfaces {
        for related_surface_id in &surface.related_surface_ids {
            let related_surface = catalog
                .surfaces
                .iter()
                .find(|candidate| {
                    candidate
                        .surface_id
                        .eq_ignore_ascii_case(related_surface_id)
                })
                .ok_or_else(|| {
                    format!(
                        "Surface '{}' references unknown related_surface_id '{}'.",
                        surface.surface_id, related_surface_id
                    )
                })?;
            if !related_surface
                .vendor_id
                .eq_ignore_ascii_case(&surface.vendor_id)
            {
                return Err(format!(
                    "Surface '{}' related_surface_id '{}' must share the same vendor.",
                    surface.surface_id, related_surface_id
                ));
            }
        }
    }

    Ok(())
}

fn validate_transport_spec(
    transport_owner: &str,
    transport_name: &str,
    url: &str,
    auth_scheme: &str,
    request_shape: Option<&str>,
) -> Result<(), String> {
    let auth_scheme = parse_auth_scheme(auth_scheme)?;
    if url.trim().is_empty() {
        return Err(format!(
            "Transport owner '{}' transport '{}' is missing a URL.",
            transport_owner, transport_name
        ));
    }
    if !transport_url_is_allowed(url, auth_scheme) {
        return Err(format!(
            "Transport owner '{}' transport '{}' must use an https URL or an allowed local no-auth URL.",
            transport_owner, transport_name
        ));
    }
    if let Some(shape) = request_shape {
        parse_request_shape(shape)?;
    }
    Ok(())
}

fn transport_url_is_allowed(url: &str, auth_scheme: ProviderAuthScheme) -> bool {
    let trimmed = url.trim();
    if trimmed.starts_with("https://") {
        return true;
    }

    if auth_scheme != ProviderAuthScheme::None || !trimmed.starts_with("http://") {
        return false;
    }

    let Some(rest) = trimmed.strip_prefix("http://") else {
        return false;
    };
    let host_port = rest.split('/').next().unwrap_or_default();
    if host_port.starts_with("[::1]") {
        return true;
    }
    let host = host_port.split(':').next().unwrap_or_default();

    matches!(host, "localhost" | "127.0.0.1")
}

fn stability_sort_key(raw: &str) -> i32 {
    match parse_surface_stability(raw).unwrap_or(SurfaceStability::Experimental) {
        SurfaceStability::Ga => 3,
        SurfaceStability::Preview => 2,
        SurfaceStability::Experimental => 1,
        SurfaceStability::Deprecated => 0,
    }
}

fn default_direct_surface_placement_sort_key(raw: &str) -> i32 {
    match parse_surface_placement_kind(raw).unwrap_or(SurfacePlacementKind::CustomHosted) {
        SurfacePlacementKind::ProviderHosted => 4,
        SurfacePlacementKind::CustomHosted => 3,
        SurfacePlacementKind::SelfHosted => 2,
        SurfacePlacementKind::InstalledCli => 1,
    }
}

fn validate_parameter_profile(profile: &ProviderParameterProfile) -> Result<(), String> {
    let supported = profile
        .supported
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let unsupported = profile
        .unsupported
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect::<HashSet<_>>();

    if supported.iter().any(|value| value.is_empty())
        || unsupported.iter().any(|value| value.is_empty())
    {
        return Err("Parameter profile contains an empty parameter entry.".to_string());
    }

    if let Some(overlap) = supported.intersection(&unsupported).next() {
        return Err(format!(
            "Parameter profile contains overlapping supported/unsupported field '{}'.",
            overlap
        ));
    }

    Ok(())
}

fn validate_model_capability_profile(
    profile: &crate::ai_providers::ProviderModelCapabilityProfile,
) -> Result<(), String> {
    let default_support = profile.default_support.trim();
    if !default_support.is_empty() {
        match default_support.to_ascii_lowercase().as_str() {
            "supported" | "unsupported" | "unknown" => {}
            other => {
                return Err(format!(
                    "Model capability profile has unsupported default_support '{}'.",
                    other
                ));
            }
        }
    }

    if profile
        .allow_patterns
        .iter()
        .chain(profile.deny_patterns.iter())
        .any(|value| value.trim().is_empty())
    {
        return Err("Model capability profile contains an empty pattern.".to_string());
    }

    Ok(())
}

fn surface_declares_model_selection(
    surface: &ProviderSurfaceSpec,
    capability: SurfaceCapabilityKind,
) -> bool {
    let has_defaults = match capability {
        SurfaceCapabilityKind::Llm => !surface.default_models.llm_models.is_empty(),
        SurfaceCapabilityKind::Ocr => !surface.default_models.ocr_models.is_empty(),
    };
    if has_defaults {
        return true;
    }

    let model_catalog_support = surface
        .model_catalog_transport
        .as_ref()
        .map(|transport| match capability {
            SurfaceCapabilityKind::Llm => transport.llm_supported,
            SurfaceCapabilityKind::Ocr => transport.ocr_supported,
        })
        .unwrap_or(false);
    if model_catalog_support {
        return true;
    }

    if surface.known_models.iter().any(|model| match capability {
        SurfaceCapabilityKind::Llm => model.capabilities.llm,
        SurfaceCapabilityKind::Ocr => model.capabilities.ocr,
    }) {
        return true;
    }

    let rules = match capability {
        SurfaceCapabilityKind::Llm => &surface.capability_rules.llm,
        SurfaceCapabilityKind::Ocr => &surface.capability_rules.ocr,
    };
    !rules.allow_patterns.is_empty() || rules.default_support.eq_ignore_ascii_case("supported")
}

fn validate_parameter_usage(
    profile: &ProviderParameterProfile,
    parameters: &[&str],
) -> Result<(), String> {
    let supported = profile
        .supported
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let unsupported = profile
        .unsupported
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect::<HashSet<_>>();

    for parameter in parameters {
        let normalized = parameter.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return Err("Parameter usage contains an empty field name.".to_string());
        }
        if unsupported.contains(&normalized) {
            return Err(format!(
                "Parameter '{parameter}' is explicitly unsupported by this provider surface."
            ));
        }
        if !supported.is_empty() && !supported.contains(&normalized) {
            return Err(format!(
                "Parameter '{parameter}' is not declared as supported by this provider surface."
            ));
        }
    }

    Ok(())
}

fn preferred_direct_surface_from_vendor<'a>(
    catalog: &'a ProviderSurfaceCatalog,
    vendor_id: &str,
) -> Option<&'a ProviderSurfaceSpec> {
    catalog
        .surfaces
        .iter()
        .filter(|surface| {
            surface.vendor_id.eq_ignore_ascii_case(vendor_id)
                && matches!(
                    parse_surface_execution_kind(&surface.execution_kind),
                    Ok(SurfaceExecutionKind::DirectHttp)
                )
        })
        .max_by(|left, right| {
            left.preferred_for_product_auth
                .cmp(&right.preferred_for_product_auth)
                .then_with(|| {
                    stability_sort_key(&left.stability).cmp(&stability_sort_key(&right.stability))
                })
                .then_with(|| {
                    default_direct_surface_placement_sort_key(&left.placement_kind).cmp(
                        &default_direct_surface_placement_sort_key(&right.placement_kind),
                    )
                })
                .then_with(|| right.display_name.cmp(&left.display_name))
        })
}

pub fn default_direct_surface_spec(
    provider_type: AiProviderType,
) -> Result<&'static ProviderSurfaceSpec, String> {
    let vendor_id = provider_vendor_id_or_default(provider_type);
    let catalog = surface_catalog()?;
    let vendor = catalog
        .vendors
        .iter()
        .find(|vendor| vendor.vendor_id.eq_ignore_ascii_case(vendor_id))
        .ok_or_else(|| {
            format!("Provider vendor for {vendor_id} is missing from the surface catalog.")
        })?;
    preferred_direct_surface_from_vendor(catalog, &vendor.vendor_id).ok_or_else(|| {
        format!(
            "Provider type '{}' does not define a default direct_http surface.",
            vendor_id
        )
    })
}

fn parse_auth_scheme(raw: &str) -> Result<ProviderAuthScheme, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "none" => Ok(ProviderAuthScheme::None),
        "bearer" => Ok(ProviderAuthScheme::Bearer),
        "x_api_key" => Ok(ProviderAuthScheme::XApiKey),
        "x_goog_api_key" => Ok(ProviderAuthScheme::XGoogApiKey),
        _ => Err(format!("Unsupported provider auth scheme '{raw}'")),
    }
}

fn parse_request_shape(raw: &str) -> Result<ProviderRequestShape, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "anthropic_messages" => Ok(ProviderRequestShape::AnthropicMessages),
        "anthropic_vision_messages" => Ok(ProviderRequestShape::AnthropicVisionMessages),
        "openai_chat_completions" => Ok(ProviderRequestShape::OpenAiChatCompletions),
        "openai_vision_chat_completions" => Ok(ProviderRequestShape::OpenAiVisionChatCompletions),
        "openai_responses" => Ok(ProviderRequestShape::OpenAiResponses),
        "google_generate_content" => Ok(ProviderRequestShape::GoogleGenerateContent),
        "google_vision_annotate" => Ok(ProviderRequestShape::GoogleVisionAnnotate),
        _ => Err(format!("Unsupported provider request shape '{raw}'")),
    }
}

fn parse_provider_type_name(raw: &str) -> Option<AiProviderType> {
    provider_type_from_vendor_id(raw)
}

fn parse_provider_type(raw: &str) -> Result<AiProviderType, String> {
    parse_provider_type_name(raw).ok_or_else(|| {
        format!(
            "Unsupported provider_type '{}'.",
            raw.trim().to_ascii_lowercase()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_provider_surface_catalog() {
        let catalog = list_provider_surface_specs().expect("surface catalog should load");
        assert!(catalog.vendors.len() >= 4);
        assert!(catalog.surfaces.len() >= 6);
    }

    #[test]
    fn resolves_aliases() {
        assert_eq!(
            resolve_provider_type("openai-compatible"),
            Some(AiProviderType::Generic)
        );
        assert_eq!(
            resolve_provider_type("gemini"),
            Some(AiProviderType::Google)
        );
        assert_eq!(
            resolve_provider_type("ollama"),
            Some(AiProviderType::Ollama)
        );
    }

    #[test]
    fn returns_openai_llm_shape() {
        let shape = request_shape(AiProviderType::OpenAi, ProviderTransportKind::Llm)
            .expect("llm shape should resolve");
        assert_eq!(shape, ProviderRequestShape::OpenAiResponses);
    }

    #[test]
    fn returns_google_catalog_shape() {
        let shape = model_catalog_response_shape(AiProviderType::Google)
            .expect("catalog shape should resolve");
        assert_eq!(shape, ModelCatalogResponseShape::GoogleModels);
    }

    #[test]
    fn returns_default_model_for_surface() {
        let model = default_surface_model(
            "provider_surface.openai.subprocess_cli",
            SurfaceCapabilityKind::Llm,
        )
        .expect("model should resolve");
        assert_eq!(model.as_deref(), Some("gpt-5.4"));
    }

    #[test]
    fn derives_catalog_default_surface_by_access_mode_and_capability() {
        assert_eq!(
            default_surface_id_for_access_mode(
                AiProviderType::OpenAi,
                AiAccessMode::ProviderOAuth,
                SurfaceCapabilityKind::Llm,
            )
            .expect("managed oauth default should resolve"),
            Some("provider_surface.openai.managed_oauth")
        );
        assert_eq!(
            default_surface_id_for_access_mode(
                AiProviderType::Google,
                AiAccessMode::ProviderSubscriptionCli,
                SurfaceCapabilityKind::Llm,
            )
            .expect("google subprocess default should resolve"),
            Some("provider_surface.google.subprocess_cli")
        );
        assert_eq!(
            default_surface_id_for_access_mode(
                AiProviderType::OpenAi,
                AiAccessMode::ProviderSubscriptionCli,
                SurfaceCapabilityKind::Ocr,
            )
            .expect("ocr subprocess default should resolve"),
            Some("provider_surface.openai.subprocess_cli")
        );
    }

    #[test]
    fn resolves_openai_direct_transport_without_surface_projection() {
        let transport = transport_spec(AiProviderType::OpenAi, ProviderTransportKind::Llm)
            .expect("transport should resolve");
        assert_eq!(transport.url, "https://api.openai.com/v1/responses");
    }

    #[test]
    fn rejects_duplicate_aliases() {
        let mut catalog = list_provider_surface_specs().expect("surface catalog should load");
        catalog.vendors[1].aliases = vec!["shared".to_string()];
        catalog.vendors[2].aliases = vec!["shared".to_string()];

        let err = validate_surface_catalog(&catalog).expect_err("duplicate aliases should fail");
        assert!(err.contains("defined more than once"));
    }

    #[test]
    fn rejects_overlapping_supported_and_unsupported_parameters() {
        let err = validate_parameter_profile(&ProviderParameterProfile {
            supported: vec!["temperature".to_string()],
            unsupported: vec!["temperature".to_string()],
            notes: Vec::new(),
        })
        .expect_err("overlapping parameters should fail");
        assert!(err.contains("overlapping supported/unsupported"));
    }

    #[test]
    fn resolves_openai_managed_surface_shape() {
        let shape = resolved_request_shape(
            AiProviderType::OpenAi,
            Some("provider_surface.openai.managed_oauth"),
            ProviderTransportKind::Llm,
        )
        .expect("managed surface should resolve");
        assert_eq!(shape, ProviderRequestShape::OpenAiResponses);
    }

    #[test]
    fn resolves_subprocess_surface_modes() {
        assert_eq!(
            subprocess_invocation_mode("provider_surface.openai.subprocess_cli")
                .expect("invocation mode should resolve"),
            SubprocessInvocationMode::CodexExecJson
        );
        assert_eq!(
            model_catalog_strategy("provider_surface.openai.direct_api")
                .expect("catalog strategy should resolve"),
            ModelCatalogStrategy::HttpModelsEndpoint
        );
        assert_eq!(
            subprocess_auth_probe_mode("provider_surface.anthropic.subprocess_cli")
                .expect("probe mode should resolve"),
            SubprocessAuthProbeMode::ClaudeAuthStatusJson
        );
    }

    #[test]
    fn loads_managed_oauth_provisioning_from_catalog() {
        let surface = provider_surface_spec("provider_surface.google.managed_oauth")
            .expect("google managed surface should load");
        let provisioning = surface
            .provisioning
            .as_ref()
            .expect("google managed surface should declare provisioning");
        assert_eq!(
            provisioning.configuration_env_vars,
            vec!["ONESHIM_GOOGLE_OAUTH_CLIENT_ID".to_string()]
        );
        assert_eq!(
            provisioning.setup_copy_key.as_deref(),
            Some("featureCapability.surface.provider_surface.google.managed_oauth.setup")
        );
        assert_eq!(
            provisioning.docs_url.as_deref(),
            Some("https://developers.google.com/identity/protocols/oauth2/native-app")
        );
    }

    #[test]
    fn loads_vendor_projection_metadata_from_catalog() {
        let catalog = surface_catalog().expect("catalog should load");
        let openai = catalog
            .vendors
            .iter()
            .find(|vendor| vendor.vendor_id == "openai")
            .expect("openai vendor should exist");
        let projection = openai
            .projection
            .as_ref()
            .expect("openai vendor should declare projection metadata");
        assert_eq!(
            projection.api_key_env_vars,
            vec!["OPENAI_API_KEY".to_string()]
        );
        assert_eq!(
            projection.api_key_temp_file_prefix.as_deref(),
            Some("openai")
        );
    }

    #[test]
    fn lists_subprocess_surface_specs_from_catalog() {
        let surfaces = list_subprocess_surface_specs().expect("subprocess surfaces should load");
        let ids: Vec<&str> = surfaces
            .iter()
            .map(|surface| surface.surface_id.as_str())
            .collect();
        assert!(ids.contains(&"provider_surface.openai.subprocess_cli"));
        assert!(ids.contains(&"provider_surface.anthropic.subprocess_cli"));
        assert!(ids.contains(&"provider_surface.google.subprocess_cli"));
    }

    #[test]
    fn reports_json_output_support_for_gemini_subprocess() {
        assert!(
            subprocess_supports_json_output("provider_surface.google.subprocess_cli")
                .expect("json output support should resolve")
        );
    }

    #[test]
    fn validates_supported_parameters_for_openai_managed_surface() {
        validate_supported_parameters(
            AiProviderType::OpenAi,
            Some("provider_surface.openai.managed_oauth"),
            SurfaceCapabilityKind::Llm,
            &["model", "instructions", "input", "max_output_tokens"],
        )
        .expect("managed OpenAI parameters should be allowed");
    }

    #[test]
    fn rejects_undeclared_parameter_usage_for_surface() {
        let err = validate_supported_surface_parameters(
            "provider_surface.anthropic.subprocess_cli",
            SurfaceCapabilityKind::Llm,
            &["response_format"],
        )
        .expect_err("unsupported subprocess parameter should fail");
        assert!(
            err.contains("not declared as supported") || err.contains("explicitly unsupported")
        );
    }

    #[test]
    fn loads_related_surface_ids_from_catalog() {
        let surface = provider_surface_spec("provider_surface.openai.managed_oauth")
            .expect("managed oauth surface should exist");
        assert_eq!(
            surface.related_surface_ids,
            vec!["provider_surface.openai.subprocess_cli".to_string()]
        );
    }

    #[test]
    fn rejects_unknown_related_surface_id() {
        let mut catalog = list_provider_surface_specs().expect("surface catalog should load");
        catalog.surfaces[0].related_surface_ids = vec!["provider_surface.missing".to_string()];

        let err =
            validate_surface_catalog(&catalog).expect_err("unknown related surface should fail");
        assert!(err.contains("unknown related_surface_id"));
    }

    #[test]
    fn rejects_cross_vendor_related_surface_id() {
        let mut catalog = list_provider_surface_specs().expect("surface catalog should load");
        let managed = catalog
            .surfaces
            .iter_mut()
            .find(|surface| surface.surface_id == "provider_surface.openai.managed_oauth")
            .expect("managed oauth surface should exist");
        managed.related_surface_ids = vec!["provider_surface.anthropic.subprocess_cli".to_string()];

        let err = validate_surface_catalog(&catalog)
            .expect_err("cross-vendor related surface should fail");
        assert!(err.contains("must share the same vendor"));
    }

    #[test]
    fn resolves_ollama_no_auth_surface() {
        let auth_scheme = resolved_auth_scheme(
            AiProviderType::Ollama,
            Some("provider_surface.ollama.local_http"),
            ProviderTransportKind::Llm,
        )
        .expect("ollama auth scheme should resolve");
        assert_eq!(auth_scheme, ProviderAuthScheme::None);
    }

    #[test]
    fn resolves_ollama_self_hosted_placement() {
        let surface = provider_surface_spec("provider_surface.ollama.local_http")
            .expect("ollama surface should exist");
        let placement =
            parse_surface_placement_kind(&surface.placement_kind).expect("placement should parse");
        assert_eq!(placement, SurfacePlacementKind::SelfHosted);
    }

    #[test]
    fn matches_known_ollama_vision_model_by_prefix() {
        let known = known_model_spec_for_surface(
            "provider_surface.ollama.local_http",
            "qwen3-vl:8b-instruct-q4_K_M",
        )
        .expect("known model lookup should succeed")
        .expect("vision model should match by prefix");
        assert!(known.capabilities.ocr);
    }

    #[test]
    fn rejects_known_non_vision_ocr_model() {
        let err = validate_known_model_capability(
            AiProviderType::Ollama,
            Some("provider_surface.ollama.local_http"),
            SurfaceCapabilityKind::Ocr,
            "qwen3:8b",
        )
        .expect_err("text-only model should be rejected for OCR");
        assert!(err.contains("not marked as OCR-capable"));
    }

    #[test]
    fn resolves_ollama_availability_probe() {
        let probe = availability_probe("provider_surface.ollama.local_http")
            .expect("availability probe should resolve")
            .expect("ollama probe should exist");
        assert_eq!(probe.url, "http://localhost:11434/api/version");
        assert_eq!(probe.auth_scheme, "none");
    }

    #[test]
    fn resolves_unknown_model_policy_from_catalog() {
        assert_eq!(
            unknown_model_policy_for_surface(
                "provider_surface.openai.direct_api",
                SurfaceCapabilityKind::Llm
            )
            .expect("llm policy should resolve"),
            ProviderUnknownModelPolicy::Allow
        );
        assert_eq!(
            unknown_model_policy_for_surface(
                "provider_surface.openai.direct_api",
                SurfaceCapabilityKind::Ocr
            )
            .expect("ocr policy should resolve"),
            ProviderUnknownModelPolicy::Reject
        );
    }

    #[test]
    fn rejects_unknown_ocr_models_when_surface_policy_requires_known_models() {
        let error = validate_known_model_capability(
            AiProviderType::OpenAi,
            Some("provider_surface.openai.direct_api"),
            SurfaceCapabilityKind::Ocr,
            "custom-text-model",
        )
        .expect_err("unknown OCR model should be rejected");
        assert!(error.contains("not catalogued") || error.contains("not marked"));
    }

    #[test]
    fn surfaces_with_warn_policy_emit_unknown_model_warning() {
        let warning = known_model_capability_warning(
            AiProviderType::Generic,
            Some("provider_surface.generic.direct_api"),
            SurfaceCapabilityKind::Ocr,
            "totally-new-model",
        )
        .expect("warning lookup should succeed");
        assert!(warning.is_some());
    }

    #[test]
    fn resolves_capability_rules_for_local_openai_compatible_surface() {
        assert_eq!(
            model_capability_status_for_surface(
                "provider_surface.generic.local_openai_compatible",
                SurfaceModelCapabilityKind::Ocr,
                "qwen2.5-vl-7b-instruct"
            )
            .expect("ocr support should resolve"),
            ProviderModelSupportStatus::Supported
        );
        assert_eq!(
            model_capability_status_for_surface(
                "provider_surface.generic.local_openai_compatible",
                SurfaceModelCapabilityKind::StructuredOutput,
                "text-embedding-3-small"
            )
            .expect("structured output support should resolve"),
            ProviderModelSupportStatus::Unsupported
        );
    }

    #[test]
    fn explicit_model_selection_is_required_for_local_openai_compatible_surface() {
        assert!(surface_requires_explicit_model_selection(
            "provider_surface.generic.local_openai_compatible",
            SurfaceCapabilityKind::Llm
        )
        .expect("selection requirement should resolve"));
    }

    #[test]
    fn loads_surface_execution_capabilities_from_catalog() {
        let catalog = provider_surface_catalog().expect("catalog should load");
        let openai = catalog
            .surfaces
            .iter()
            .find(|surface| surface.surface_id == "provider_surface.openai.direct_api")
            .expect("openai direct surface should exist");
        assert!(openai.llm_capabilities.structured_output);

        let google = catalog
            .surfaces
            .iter()
            .find(|surface| surface.surface_id == "provider_surface.google.direct_api")
            .expect("google direct surface should exist");
        assert_eq!(google.ocr_capabilities.strategy, "vision_api");
        assert!(google.ocr_capabilities.supports_geometry);
        assert!(google.ocr_capabilities.supports_confidence);
        assert!(openai.ocr_capabilities.requires_structured_output_model);
    }
}
