use std::collections::HashSet;
use std::sync::OnceLock;

use oneshim_core::config::AiProviderType;
use oneshim_core::provider_surface::canonical_provider_surface_id;

use crate::ai_providers::{
    ProviderModelCatalogTransportSpec, ProviderParameterProfile, ProviderParameterSet,
    ProviderTransportSpec,
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
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ProviderSurfaceSpec {
    pub surface_id: String,
    pub vendor_id: String,
    pub provider_type: String,
    pub display_name: String,
    pub execution_kind: String,
    pub credential_kind: String,
    pub stability: String,
    #[serde(default)]
    pub preferred_for_product_auth: bool,
    #[serde(default)]
    pub related_surface_ids: Vec<String>,
    #[serde(default)]
    pub catalog_strategy: String,
    pub supports: ProviderSurfaceSupports,
    pub default_models: SurfaceDefaultModels,
    pub parameter_profiles: ProviderParameterSet,
    #[serde(default)]
    pub llm_transport: Option<ProviderTransportSpec>,
    #[serde(default)]
    pub ocr_transport: Option<ProviderTransportSpec>,
    #[serde(default)]
    pub model_catalog_transport: Option<ProviderModelCatalogTransportSpec>,
    #[serde(default)]
    pub subprocess_transport: Option<SubprocessTransportSpec>,
    #[serde(default)]
    pub references: Vec<String>,
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

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SurfaceDefaultModels {
    #[serde(default)]
    pub llm_models: Vec<String>,
    #[serde(default)]
    pub ocr_models: Vec<String>,
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
pub enum SurfaceExecutionKind {
    DirectHttp,
    ManagedHttp,
    SubprocessCli,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceStability {
    Ga,
    Preview,
    Experimental,
    Deprecated,
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
        let expected = provider_type_label(provider_type);
        if !surface.provider_type.eq_ignore_ascii_case(expected) {
            return Err(format!(
                "Surface '{}' does not match provider_type '{}'.",
                surface_id, expected
            ));
        }
        return Ok(surface);
    }

    compatibility_surface_for_provider_type(provider_type)
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
            provider_type_label(provider_type)
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

pub fn parse_surface_stability(raw: &str) -> Result<SurfaceStability, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "ga" => Ok(SurfaceStability::Ga),
        "preview" => Ok(SurfaceStability::Preview),
        "experimental" => Ok(SurfaceStability::Experimental),
        "deprecated" => Ok(SurfaceStability::Deprecated),
        other => Err(format!("Unsupported surface stability '{other}'.")),
    }
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

pub fn subprocess_invocation_mode(surface_id: &str) -> Result<SubprocessInvocationMode, String> {
    parse_subprocess_invocation_mode(&subprocess_transport(surface_id)?.invocation_mode)
}

pub fn subprocess_auth_probe_mode(surface_id: &str) -> Result<SubprocessAuthProbeMode, String> {
    parse_subprocess_auth_probe_mode(&subprocess_transport(surface_id)?.auth_probe_mode)
}

fn surface_catalog() -> Result<&'static ProviderSurfaceCatalog, String> {
    match SURFACE_CATALOG.get_or_init(load_surface_catalog) {
        Ok(catalog) => Ok(catalog),
        Err(message) => Err(message.clone()),
    }
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
        parse_surface_stability(&surface.stability)?;
        if surface.references.is_empty() {
            return Err(format!(
                "Surface '{}' must include at least one reference URL.",
                surface.surface_id
            ));
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

        if surface.supports.llm && surface.default_models.llm_models.is_empty() {
            return Err(format!(
                "Surface '{}' supports LLM but has no default llm_models.",
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
            }
            SurfaceExecutionKind::SubprocessCli => {
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
    if url.trim().is_empty() {
        return Err(format!(
            "Transport owner '{}' transport '{}' is missing a URL.",
            transport_owner, transport_name
        ));
    }
    if !url.trim().starts_with("https://") {
        return Err(format!(
            "Transport owner '{}' transport '{}' must use an https URL.",
            transport_owner, transport_name
        ));
    }
    parse_auth_scheme(auth_scheme)?;
    if let Some(shape) = request_shape {
        parse_request_shape(shape)?;
    }
    Ok(())
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

fn compatibility_surface_from_vendor<'a>(
    catalog: &'a ProviderSurfaceCatalog,
    vendor_id: &str,
) -> Option<&'a ProviderSurfaceSpec> {
    catalog.surfaces.iter().find(|surface| {
        surface.vendor_id.eq_ignore_ascii_case(vendor_id)
            && matches!(
                parse_surface_execution_kind(&surface.execution_kind),
                Ok(SurfaceExecutionKind::DirectHttp)
            )
    })
}

fn compatibility_surface_for_provider_type(
    provider_type: AiProviderType,
) -> Result<&'static ProviderSurfaceSpec, String> {
    let label = provider_type_label(provider_type);
    let catalog = surface_catalog()?;
    let vendor = catalog
        .vendors
        .iter()
        .find(|vendor| vendor.provider_type.eq_ignore_ascii_case(label))
        .ok_or_else(|| {
            format!("Provider vendor for {label} is missing from the surface catalog.")
        })?;
    compatibility_surface_from_vendor(catalog, &vendor.vendor_id).ok_or_else(|| {
        format!(
            "Provider type '{}' does not define a direct_http compatibility surface.",
            label
        )
    })
}

fn parse_auth_scheme(raw: &str) -> Result<ProviderAuthScheme, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
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

fn provider_type_label(provider_type: AiProviderType) -> &'static str {
    match provider_type {
        AiProviderType::Anthropic => "Anthropic",
        AiProviderType::OpenAi => "OpenAi",
        AiProviderType::Google => "Google",
        AiProviderType::Generic => "Generic",
    }
}

fn parse_provider_type_name(raw: &str) -> Option<AiProviderType> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "anthropic" => Some(AiProviderType::Anthropic),
        "openai" | "open_ai" | "open-ai" | "openai-compatible" => Some(AiProviderType::OpenAi),
        "google" | "gemini" => Some(AiProviderType::Google),
        "generic" => Some(AiProviderType::Generic),
        _ => None,
    }
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
            Some(AiProviderType::OpenAi)
        );
        assert_eq!(
            resolve_provider_type("gemini"),
            Some(AiProviderType::Google)
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
    fn resolves_openai_direct_transport_without_compatibility_projection() {
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
            subprocess_auth_probe_mode("provider_surface.anthropic.subprocess_cli")
                .expect("probe mode should resolve"),
            SubprocessAuthProbeMode::ClaudeAuthStatusJson
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
}
