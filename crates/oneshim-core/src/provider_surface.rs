use crate::config::{AiAccessMode, AiProviderType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderSurfaceTransport {
    DirectApi,
    ManagedOAuth,
    SubprocessCli,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderSurfaceSpec {
    pub id: &'static str,
    pub provider_type: AiProviderType,
    pub transport: ProviderSurfaceTransport,
    pub supports_llm: bool,
    pub supports_ocr: bool,
}

const KNOWN_PROVIDER_SURFACES: &[ProviderSurfaceSpec] = &[
    ProviderSurfaceSpec {
        id: "provider_surface.anthropic.direct_api",
        provider_type: AiProviderType::Anthropic,
        transport: ProviderSurfaceTransport::DirectApi,
        supports_llm: true,
        supports_ocr: true,
    },
    ProviderSurfaceSpec {
        id: "provider_surface.openai.direct_api",
        provider_type: AiProviderType::OpenAi,
        transport: ProviderSurfaceTransport::DirectApi,
        supports_llm: true,
        supports_ocr: true,
    },
    ProviderSurfaceSpec {
        id: "provider_surface.google.direct_api",
        provider_type: AiProviderType::Google,
        transport: ProviderSurfaceTransport::DirectApi,
        supports_llm: true,
        supports_ocr: true,
    },
    ProviderSurfaceSpec {
        id: "provider_surface.ollama.local_http",
        provider_type: AiProviderType::Ollama,
        transport: ProviderSurfaceTransport::DirectApi,
        supports_llm: true,
        supports_ocr: true,
    },
    ProviderSurfaceSpec {
        id: "provider_surface.generic.direct_api",
        provider_type: AiProviderType::Generic,
        transport: ProviderSurfaceTransport::DirectApi,
        supports_llm: true,
        supports_ocr: true,
    },
    ProviderSurfaceSpec {
        id: "provider_surface.openai.managed_oauth",
        provider_type: AiProviderType::OpenAi,
        transport: ProviderSurfaceTransport::ManagedOAuth,
        supports_llm: true,
        supports_ocr: false,
    },
    ProviderSurfaceSpec {
        id: "provider_surface.openai.subprocess_cli",
        provider_type: AiProviderType::OpenAi,
        transport: ProviderSurfaceTransport::SubprocessCli,
        supports_llm: true,
        supports_ocr: false,
    },
    ProviderSurfaceSpec {
        id: "provider_surface.anthropic.subprocess_cli",
        provider_type: AiProviderType::Anthropic,
        transport: ProviderSurfaceTransport::SubprocessCli,
        supports_llm: true,
        supports_ocr: false,
    },
    ProviderSurfaceSpec {
        id: "provider_surface.google.subprocess_cli",
        provider_type: AiProviderType::Google,
        transport: ProviderSurfaceTransport::SubprocessCli,
        supports_llm: true,
        supports_ocr: false,
    },
];

pub fn canonical_provider_surface_id(raw: &str) -> Option<&'static str> {
    provider_surface_spec(raw).map(|spec| spec.id)
}

pub fn provider_surface_spec(raw: &str) -> Option<ProviderSurfaceSpec> {
    let normalized = raw.trim().to_ascii_lowercase();
    KNOWN_PROVIDER_SURFACES
        .iter()
        .copied()
        .find(|spec| spec.id.eq_ignore_ascii_case(&normalized))
}

pub fn provider_surface_supports_llm(raw: &str) -> bool {
    provider_surface_spec(raw).is_some_and(|spec| spec.supports_llm)
}

pub fn provider_surface_supports_ocr(raw: &str) -> bool {
    provider_surface_spec(raw).is_some_and(|spec| spec.supports_ocr)
}

pub fn provider_surface_uses_no_auth(raw: &str) -> bool {
    matches!(
        provider_surface_spec(raw),
        Some(ProviderSurfaceSpec {
            id: "provider_surface.ollama.local_http",
            ..
        })
    )
}

pub fn default_provider_surface_id(
    provider_type: AiProviderType,
    access_mode: AiAccessMode,
) -> Option<&'static str> {
    match access_mode {
        AiAccessMode::ProviderOAuth => (provider_type == AiProviderType::OpenAi)
            .then_some("provider_surface.openai.managed_oauth"),
        AiAccessMode::ProviderSubscriptionCli => match provider_type {
            AiProviderType::OpenAi => Some("provider_surface.openai.subprocess_cli"),
            AiProviderType::Anthropic => Some("provider_surface.anthropic.subprocess_cli"),
            AiProviderType::Google => Some("provider_surface.google.subprocess_cli"),
            AiProviderType::Ollama => None,
            AiProviderType::Generic => None,
        },
        AiAccessMode::ProviderApiKey
        | AiAccessMode::PlatformConnected
        | AiAccessMode::LocalModel => Some(match provider_type {
            AiProviderType::Anthropic => "provider_surface.anthropic.direct_api",
            AiProviderType::OpenAi => "provider_surface.openai.direct_api",
            AiProviderType::Google => "provider_surface.google.direct_api",
            AiProviderType::Ollama => "provider_surface.ollama.local_http",
            AiProviderType::Generic => "provider_surface.generic.direct_api",
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_known_surface_ids() {
        assert_eq!(
            canonical_provider_surface_id("provider_surface.openai.subprocess_cli"),
            Some("provider_surface.openai.subprocess_cli")
        );
        assert_eq!(
            canonical_provider_surface_id("PROVIDER_SURFACE.OPENAI.DIRECT_API"),
            Some("provider_surface.openai.direct_api")
        );
    }

    #[test]
    fn derives_defaults_for_access_mode() {
        assert_eq!(
            default_provider_surface_id(AiProviderType::OpenAi, AiAccessMode::ProviderOAuth),
            Some("provider_surface.openai.managed_oauth")
        );
        assert_eq!(
            default_provider_surface_id(
                AiProviderType::Anthropic,
                AiAccessMode::ProviderSubscriptionCli
            ),
            Some("provider_surface.anthropic.subprocess_cli")
        );
        assert_eq!(
            default_provider_surface_id(AiProviderType::Generic, AiAccessMode::ProviderApiKey),
            Some("provider_surface.generic.direct_api")
        );
    }
}
