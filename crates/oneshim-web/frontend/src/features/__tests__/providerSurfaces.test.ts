import { describe, expect, it } from 'vitest'
import { deriveDefaultProviderSurfaceId } from '../providerSurfaces'

describe('provider surface defaults', () => {
  it('uses managed oauth for OpenAI llm oauth mode', () => {
    expect(deriveDefaultProviderSurfaceId('ProviderOAuth', 'llm_api', 'OpenAi')).toBe(
      'provider_surface.openai.managed_oauth',
    )
  })

  it('uses subprocess surface for subscription cli mode', () => {
    expect(deriveDefaultProviderSurfaceId('ProviderSubscriptionCli', 'llm_api', 'Anthropic')).toBe(
      'provider_surface.anthropic.subprocess_cli',
    )
  })

  it('keeps ocr surface unset in subscription cli mode', () => {
    expect(deriveDefaultProviderSurfaceId('ProviderSubscriptionCli', 'ocr_api', 'OpenAi')).toBeNull()
  })

  it('falls back to direct api for generic provider types', () => {
    expect(deriveDefaultProviderSurfaceId('ProviderApiKey', 'llm_api', 'Generic')).toBe(
      'provider_surface.generic.direct_api',
    )
  })
})
