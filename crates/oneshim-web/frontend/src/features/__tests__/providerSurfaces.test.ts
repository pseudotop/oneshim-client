import { describe, expect, it } from 'vitest'
import { DEFAULT_PROVIDER_SURFACE_CATALOG } from '../../api/defaultProviderSurfaceCatalog'
import {
  deriveDefaultProviderSurfaceId,
  getCompatibleProviderSurfaces,
  preferredRelatedProviderSurface,
  sortProviderSurfaces,
} from '../providerSurfaces'

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

  it('prefers higher-stability preferred surfaces within the same compatibility set', () => {
    const subprocess = DEFAULT_PROVIDER_SURFACE_CATALOG.surfaces.find(
      (surface) => surface.surface_id === 'provider_surface.openai.subprocess_cli',
    )
    expect(subprocess).toBeDefined()

    const legacySubprocess = {
      ...subprocess!,
      surface_id: 'provider_surface.test.legacy_cli',
      display_name: 'Legacy CLI',
      preferred_for_product_auth: false,
      stability: 'experimental',
    }

    expect(sortProviderSurfaces([legacySubprocess, subprocess!]).map((surface) => surface.surface_id)).toEqual([
      'provider_surface.openai.subprocess_cli',
      'provider_surface.test.legacy_cli',
    ])
  })

  it('resolves explicit related subprocess surface for managed oauth', () => {
    const oauthSurface = DEFAULT_PROVIDER_SURFACE_CATALOG.surfaces.find(
      (surface) => surface.surface_id === 'provider_surface.openai.managed_oauth',
    )

    expect(
      preferredRelatedProviderSurface(DEFAULT_PROVIDER_SURFACE_CATALOG, oauthSurface, 'subprocess_cli')?.surface_id,
    ).toBe('provider_surface.openai.subprocess_cli')
  })
})
