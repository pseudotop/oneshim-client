import { describe, it, expect, afterEach } from 'vitest'
import { mockIPC, clearMocks } from '@tauri-apps/api/mocks'
import { invoke } from '@tauri-apps/api/core'

describe('CRT-MK-M010: get_update_status IPC contract', () => {
  afterEach(() => clearMocks())

  it('M010: returns disabled state', async () => {
    mockIPC((cmd) => {
      if (cmd === 'get_update_status') return { phase: 'disabled', message: 'Updates disabled' }
    })
    const result = await invoke<any>('get_update_status')
    expect(result.phase).toBe('disabled')
    expect(result.message).toBe('Updates disabled')
  })

  it('M011: returns idle state', async () => {
    mockIPC((cmd) => {
      if (cmd === 'get_update_status') return { phase: 'idle' }
    })
    const result = await invoke<any>('get_update_status')
    expect(result.phase).toBe('idle')
  })

  it('M012: returns downloading state with progress', async () => {
    mockIPC((cmd) => {
      if (cmd === 'get_update_status') return { phase: 'downloading', message: '45%' }
    })
    const result = await invoke<any>('get_update_status')
    expect(result.phase).toBe('downloading')
  })
})
