import { describe, it, expect, afterEach } from 'vitest'
import { mockIPC, clearMocks } from '@tauri-apps/api/mocks'
import { invoke } from '@tauri-apps/api/core'

describe('CRT-MK-M004: get_settings IPC contract', () => {
  afterEach(() => clearMocks())

  it('M004: returns settings object with standard sections', async () => {
    mockIPC((cmd) => {
      if (cmd === 'get_settings') {
        return {
          monitoring: { interval_seconds: 5 },
          capture: { enabled: true },
          notification: { enabled: true },
          server: { base_url: '[REDACTED]', api_key: '[REDACTED]' },
        }
      }
    })
    const result = await invoke<any>('get_settings')
    expect(result).toHaveProperty('monitoring')
    expect(result).toHaveProperty('capture')
    expect(result).toHaveProperty('notification')
  })

  it('M005: redacted fields contain [REDACTED]', async () => {
    mockIPC((cmd) => {
      if (cmd === 'get_settings') {
        return {
          server: { base_url: '[REDACTED]', api_key: '[REDACTED]', timeout: 30 },
        }
      }
    })
    const result = await invoke<any>('get_settings')
    expect(result.server.base_url).toBe('[REDACTED]')
    expect(result.server.api_key).toBe('[REDACTED]')
    expect(result.server.timeout).toBe(30)
  })

  it('M006: handles error response', async () => {
    mockIPC((cmd) => {
      if (cmd === 'get_settings') throw new Error('config not found')
    })
    await expect(invoke('get_settings')).rejects.toThrow()
  })
})
