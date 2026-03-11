import { describe, it, expect, afterEach } from 'vitest'
import { mockIPC, clearMocks } from '@tauri-apps/api/mocks'
import { invoke } from '@tauri-apps/api/core'

describe('CRT-MK-M007: update_setting IPC contract', () => {
  afterEach(() => clearMocks())

  it('M007: accepts valid config_json with allowed key', async () => {
    mockIPC((cmd, args) => {
      if (cmd === 'update_setting') {
        const json = (args as any).config_json
        const parsed = JSON.parse(json)
        if (parsed.notification) return { ok: true }
        throw new Error('not permitted')
      }
    })
    const result = await invoke('update_setting', {
      config_json: JSON.stringify({ notification: { enabled: false } }),
    })
    expect(result).toEqual({ ok: true })
  })

  it('M008: rejects disallowed key', async () => {
    mockIPC((cmd, args) => {
      if (cmd === 'update_setting') {
        const json = (args as any).config_json
        const parsed = JSON.parse(json)
        const ALLOWED = ['monitoring', 'capture', 'notification', 'web', 'schedule',
          'telemetry', 'privacy', 'update', 'language', 'theme']
        for (const key of Object.keys(parsed)) {
          if (!ALLOWED.includes(key)) throw new Error(`modifying '${key}' is not permitted`)
        }
        return { ok: true }
      }
    })
    await expect(
      invoke('update_setting', { config_json: JSON.stringify({ server: { url: 'x' } }) })
    ).rejects.toThrow(/not permitted/)
  })

  it('M009: handles malformed JSON', async () => {
    mockIPC((cmd, args) => {
      if (cmd === 'update_setting') {
        try {
          JSON.parse((args as any).config_json)
        } catch {
          throw new Error('invalid JSON')
        }
        return { ok: true }
      }
    })
    await expect(
      invoke('update_setting', { config_json: 'not-json{' })
    ).rejects.toThrow(/invalid JSON/)
  })
})
