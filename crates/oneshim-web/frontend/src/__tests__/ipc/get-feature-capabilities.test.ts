import { afterEach, describe, expect, it } from 'vitest'
import { mockIPC, clearMocks } from '@tauri-apps/api/mocks'
import { invoke } from '@tauri-apps/api/core'

describe('CRT-MK-M051: get_feature_capabilities IPC contract', () => {
  afterEach(() => clearMocks())

  it('M051: returns feature capability snapshot', async () => {
    mockIPC((cmd) => {
      if (cmd === 'get_feature_capabilities') {
        return {
          features: [
            {
              feature_id: 'provider_surface.openai.managed_oauth',
              maturity: 'experimental',
              availability: 'available',
              preferred: false,
              requires: ['os_secret_store'],
              status_reason: null,
              status_copy_key: 'featureCapability.providerSurface.openaiManagedOAuth.available',
            },
          ],
        }
      }
    })

    const result = await invoke<{
      features: Array<{
        feature_id: string
        maturity: string
        availability: string
        preferred: boolean
        requires: string[]
        status_reason: string | null
        status_copy_key: string | null
      }>
    }>('get_feature_capabilities')

    expect(result.features).toHaveLength(1)
    expect(result.features[0]?.feature_id).toBe('provider_surface.openai.managed_oauth')
    expect(result.features[0]?.maturity).toBe('experimental')
  })
})
