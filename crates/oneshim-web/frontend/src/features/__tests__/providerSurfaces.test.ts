import { describe, expect, it } from 'vitest'
import { DEFAULT_PROVIDER_SURFACE_CATALOG } from '../../api/defaultProviderSurfaceCatalog'
import { deriveDefaultProviderSurfaceId, getCompatibleProviderSurfaces } from '../providerSurfaces'

describe('provider surface defaults', () => {
  it('uses managed oauth for OpenAI llm oauth mode', () => {
    expect(deriveDefaultProviderSurfaceId(DEFAULT_PROVIDER_SURFACE_CATALOG, 'ProviderOAuth', 'llm_api', 'OpenAi')).toBe(
      'provider_surface.openai.managed_oauth',
    )
  })

  it('uses subprocess surface for subscription cli mode', () => {
    expect(
      deriveDefaultProviderSurfaceId(DEFAULT_PROVIDER_SURFACE_CATALOG, 'ProviderSubscriptionCli', 'llm_api', 'Anthropic'),
    ).toBe('provider_surface.anthropic.subprocess_cli')
  })

  it('keeps ocr surface unset in subscription cli mode', () => {
    expect(
      deriveDefaultProviderSurfaceId(DEFAULT_PROVIDER_SURFACE_CATALOG, 'ProviderSubscriptionCli', 'ocr_api', 'OpenAi'),
    ).toBeNull()
  })

  it('falls back to direct api for generic provider types', () => {
    expect(
      deriveDefaultProviderSurfaceId(DEFAULT_PROVIDER_SURFACE_CATALOG, 'ProviderApiKey', 'llm_api', 'Generic'),
    ).toBe(
      'provider_surface.generic.direct_api',
    )
  })

  it('filters compatible surfaces for oauth llm mode', () => {
    const surfaces = getCompatibleProviderSurfaces(DEFAULT_PROVIDER_SURFACE_CATALOG, 'ProviderOAuth', 'llm_api')
    expect(surfaces.map((surface) => surface.surface_id)).toEqual(['provider_surface.openai.managed_oauth'])
  })
})
