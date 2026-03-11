import { describe, it, expect, afterEach } from 'vitest'
import { mockWindows, clearMocks } from '@tauri-apps/api/mocks'

describe('CRT-MK-M021: Window API contract', () => {
  afterEach(() => clearMocks())

  it('M021: mockWindows provides current window', async () => {
    mockWindows('main', 'settings')
    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    const win = getCurrentWindow()
    expect(win.label).toBe('main')
  })

  it('M022: multiple windows accessible', async () => {
    mockWindows('main', 'settings', 'about')
    const { getAll } = await import('@tauri-apps/api/window')
    const windows = getAll()
    expect(windows.length).toBeGreaterThanOrEqual(3)
  })
})
