import { describe, expect, it } from 'vitest'
import { DEFAULT_PROVIDER_SURFACE_CATALOG } from '../../api/defaultProviderSurfaceCatalog'
import type { FeatureCapabilitySnapshot } from '../../api/contracts'
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

  it('keeps direct OCR surfaces available in subscription cli mode', () => {
    expect(
      deriveDefaultProviderSurfaceId(DEFAULT_PROVIDER_SURFACE_CATALOG, 'ProviderSubscriptionCli', 'ocr_api', 'OpenAi'),
    ).toBe('provider_surface.openai.direct_api')
  })

  it('lists OCR-compatible direct surfaces even when llm access mode uses provider CLI', () => {
    const surfaces = getCompatibleProviderSurfaces(
      DEFAULT_PROVIDER_SURFACE_CATALOG,
      'ProviderSubscriptionCli',
      'ocr_api',
    )
    expect(surfaces.some((surface) => surface.surface_id === 'provider_surface.openai.direct_api')).toBe(true)
    expect(surfaces.some((surface) => surface.surface_id === 'provider_surface.anthropic.direct_api')).toBe(true)
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

  it('uses capability availability when deriving default CLI surface', () => {
    const snapshot: FeatureCapabilitySnapshot = {
      features: [
        {
          feature_id: 'provider_surface.openai.subprocess_cli',
          maturity: 'beta',
          availability: 'available',
          preferred: true,
          requires: ['cli:codex'],
          status_reason: null,
          status_copy_key: null,
        },
        {
          feature_id: 'provider_surface.anthropic.subprocess_cli',
          maturity: 'beta',
          availability: 'unavailable',
          preferred: true,
          requires: ['cli:claude-code'],
          status_reason: null,
          status_copy_key: null,
        },
      ],
    }

    expect(
      deriveDefaultProviderSurfaceId(
        DEFAULT_PROVIDER_SURFACE_CATALOG,
        'ProviderSubscriptionCli',
        'llm_api',
        'Generic',
        snapshot,
      ),
    ).toBe('provider_surface.openai.subprocess_cli')
  })
})
