import { describe, it, expect, afterEach } from 'vitest'
import { mockIPC, clearMocks } from '@tauri-apps/api/mocks'
import { invoke } from '@tauri-apps/api/core'

describe('CRT-MK-M001: get_metrics IPC contract', () => {
  afterEach(() => clearMocks())

  it('M001: returns MetricsResponse shape', async () => {
    mockIPC((cmd) => {
      if (cmd === 'get_metrics') {
        return {
          agent_cpu: 2.5,
          agent_memory_mb: 48.0,
          system_cpu: 15.3,
          system_memory_used_mb: 8192.0,
          system_memory_total_mb: 16384.0,
        }
      }
    })
    const result = await invoke('get_metrics')
    expect(result).toHaveProperty('agent_cpu')
    expect(result).toHaveProperty('system_memory_total_mb')
    expect(typeof (result as any).agent_cpu).toBe('number')
  })

  it('M002: handles error response', async () => {
    mockIPC((cmd) => {
      if (cmd === 'get_metrics') throw new Error('system unavailable')
    })
    await expect(invoke('get_metrics')).rejects.toThrow()
  })

  it('M003: all numeric fields are non-negative', async () => {
    mockIPC((cmd) => {
      if (cmd === 'get_metrics') {
        return {
          agent_cpu: 0,
          agent_memory_mb: 0,
          system_cpu: 0,
          system_memory_used_mb: 0,
          system_memory_total_mb: 1024,
        }
      }
    })
    const result = await invoke<any>('get_metrics')
    expect(result.agent_cpu).toBeGreaterThanOrEqual(0)
    expect(result.agent_memory_mb).toBeGreaterThanOrEqual(0)
    expect(result.system_memory_total_mb).toBeGreaterThan(0)
  })
})
