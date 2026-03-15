use std::collections::HashSet;
use std::sync::OnceLock;

use oneshim_core::config::AiProviderType;
use oneshim_core::provider_surface::canonical_provider_surface_id;

use crate::ai_providers::{
    ProviderModelCatalogTransportSpec, ProviderParameterProfile, ProviderParameterSet,
    ProviderPreset, ProviderPresetCatalog, ProviderTransportSpec,
};

const PROVIDER_SURFACE_SPECS_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../specs/providers/provider-surface-catalog.v2.json"
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
    #[serde(default)]
    pub executable_candidates: Vec<String>,
    #[serde(default)]
    pub auth_probe_command: Vec<String>,
    pub invocation_mode: String,
    #[serde(default)]
    pub model_flag: Option<String>,
    #[serde(default)]
    pub json_output_supported: bool,
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

pub fn list_provider_surface_specs() -> Result<ProviderSurfaceCatalog, String> {
    Ok(catalog()?.clone())
}

pub fn provider_surface_catalog() -> Result<&'static ProviderSurfaceCatalog, String> {
    catalog()
}

pub fn provider_surface_spec(surface_id: &str) -> Result<&'static ProviderSurfaceSpec, String> {
    let normalized = surface_id.trim().to_ascii_lowercase();
    catalog()?
        .surfaces
        .iter()
        .find(|surface| surface.surface_id.eq_ignore_ascii_case(&normalized))
        .ok_or_else(|| format!("Provider surface spec for {surface_id} is missing."))
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

pub fn list_compatibility_provider_presets() -> Result<ProviderPresetCatalog, String> {
    let catalog = catalog()?;
    let providers = catalog
        .vendors
        .iter()
        .filter_map(|vendor| compatibility_preset_from_vendor(catalog, vendor))
        .collect::<Vec<_>>();

    Ok(ProviderPresetCatalog {
        version: catalog.version,
        updated_at: catalog.updated_at.clone(),
        providers,
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

fn catalog() -> Result<&'static ProviderSurfaceCatalog, String> {
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
        parse_provider_type(&surface.provider_type)?;
        validate_parameter_profile(&surface.parameter_profiles.llm)?;
        validate_parameter_profile(&surface.parameter_profiles.ocr)?;

        match parse_surface_execution_kind(&surface.execution_kind)? {
            SurfaceExecutionKind::DirectHttp | SurfaceExecutionKind::ManagedHttp => {
                if surface.supports.llm && surface.llm_transport.is_none() {
                    return Err(format!(
                        "Surface '{}' supports LLM but is missing llm_transport.",
                        surface.surface_id
                    ));
                }
                if surface.supports.ocr && surface.ocr_transport.is_none() {
                    return Err(format!(
                        "Surface '{}' supports OCR but is missing ocr_transport.",
                        surface.surface_id
                    ));
                }
                if surface.supports.model_catalog && surface.model_catalog_transport.is_none() {
                    return Err(format!(
                        "Surface '{}' supports model_catalog but is missing model_catalog_transport.",
                        surface.surface_id
                    ));
                }
            }
            SurfaceExecutionKind::SubprocessCli => {
                let subprocess = surface.subprocess_transport.as_ref().ok_or_else(|| {
                    format!(
                        "Surface '{}' uses subprocess_cli but is missing subprocess_transport.",
                        surface.surface_id
                    )
                })?;
                if subprocess.executable_candidates.is_empty() {
                    return Err(format!(
                        "Subprocess surface '{}' must declare executable_candidates.",
                        surface.surface_id
                    ));
                }
            }
        }
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

    if let Some(overlap) = supported.intersection(&unsupported).next() {
        return Err(format!(
            "Parameter profile contains overlapping supported/unsupported field '{}'.",
            overlap
        ));
    }

    Ok(())
}

fn compatibility_preset_from_vendor(
    catalog: &ProviderSurfaceCatalog,
    vendor: &ProviderVendorSpec,
) -> Option<ProviderPreset> {
    let surface = catalog.surfaces.iter().find(|surface| {
        surface.vendor_id.eq_ignore_ascii_case(&vendor.vendor_id)
            && matches!(
                parse_surface_execution_kind(&surface.execution_kind),
                Ok(SurfaceExecutionKind::DirectHttp)
            )
    })?;

    Some(ProviderPreset {
        provider_type: vendor.provider_type.clone(),
        aliases: vendor.aliases.clone(),
        display_name: vendor.display_name.clone(),
        llm_endpoint: surface
            .llm_transport
            .as_ref()
            .map(|value| value.url.clone())
            .unwrap_or_default(),
        ocr_endpoint: surface
            .ocr_transport
            .as_ref()
            .map(|value| value.url.clone())
            .unwrap_or_default(),
        model_catalog_endpoint: surface
            .model_catalog_transport
            .as_ref()
            .map(|value| value.url.clone())
            .unwrap_or_default(),
        ocr_model_catalog_supported: surface
            .model_catalog_transport
            .as_ref()
            .map(|value| value.ocr_supported)
            .unwrap_or(false),
        ocr_model_catalog_notice: surface
            .model_catalog_transport
            .as_ref()
            .and_then(|value| value.ocr_notice.clone()),
        llm_models: surface.default_models.llm_models.clone(),
        ocr_models: surface.default_models.ocr_models.clone(),
    })
}

fn parse_provider_type(raw: &str) -> Result<AiProviderType, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "anthropic" => Ok(AiProviderType::Anthropic),
        "openai" => Ok(AiProviderType::OpenAi),
        "google" => Ok(AiProviderType::Google),
        "generic" => Ok(AiProviderType::Generic),
        other => Err(format!("Unsupported provider_type '{other}'.")),
    }
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
    fn returns_default_model_for_surface() {
        let model = default_surface_model(
            "provider_surface.openai.subprocess_cli",
            SurfaceCapabilityKind::Llm,
        )
        .expect("model should resolve");
        assert_eq!(model.as_deref(), Some("gpt-5.4"));
    }

    #[test]
    fn derives_compatibility_presets_from_direct_surfaces() {
        let presets =
            list_compatibility_provider_presets().expect("compatibility presets should load");
        let generic = presets
            .providers
            .iter()
            .find(|provider| provider.provider_type == "Generic")
            .expect("generic preset should exist");
        assert_eq!(
            generic.llm_models.first().map(String::as_str),
            Some("gpt-5-mini")
        );
    }
}
