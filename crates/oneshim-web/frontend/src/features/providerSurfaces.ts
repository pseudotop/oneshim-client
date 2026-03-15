const DIRECT_API_SURFACE_BY_PROVIDER: Record<string, string> = {
  Anthropic: 'provider_surface.anthropic.direct_api',
  OpenAi: 'provider_surface.openai.direct_api',
  Google: 'provider_surface.google.direct_api',
  Generic: 'provider_surface.generic.direct_api',
}

const SUBPROCESS_CLI_SURFACE_BY_PROVIDER: Record<string, string> = {
  Anthropic: 'provider_surface.anthropic.subprocess_cli',
  OpenAi: 'provider_surface.openai.subprocess_cli',
  Google: 'provider_surface.google.subprocess_cli',
}

export type EndpointSurfaceKind = 'ocr_api' | 'llm_api'

export function deriveDefaultProviderSurfaceId(
  accessMode: string | null | undefined,
  endpointKind: EndpointSurfaceKind,
  providerType: string | null | undefined,
): string | null {
  const normalizedProvider = (providerType ?? '').trim() || 'Generic'

  if (endpointKind === 'llm_api' && accessMode === 'ProviderOAuth' && normalizedProvider === 'OpenAi') {
    return 'provider_surface.openai.managed_oauth'
  }

  if (accessMode === 'ProviderSubscriptionCli') {
    if (endpointKind === 'ocr_api') {
      return null
    }
    return SUBPROCESS_CLI_SURFACE_BY_PROVIDER[normalizedProvider] ?? null
  }

  return DIRECT_API_SURFACE_BY_PROVIDER[normalizedProvider] ?? DIRECT_API_SURFACE_BY_PROVIDER.Generic
}
