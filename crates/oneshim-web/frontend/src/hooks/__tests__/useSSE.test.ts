import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook } from '@testing-library/react'

// Mock standalone mode to return true (skip connect)
vi.mock('../../api/standalone', () => ({
  isStandaloneModeEnabled: vi.fn(() => true),
}))

import { useSSE } from '../useSSE'

describe('useSSE', () => {
  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('returns expected shape', () => {
    const { result } = renderHook(() => useSSE())
    expect(result.current).toHaveProperty('status')
    expect(result.current).toHaveProperty('latestMetrics')
    expect(result.current).toHaveProperty('latestFrame')
    expect(result.current).toHaveProperty('idleState')
    expect(result.current).toHaveProperty('metricsHistory')
    expect(result.current).toHaveProperty('connect')
    expect(result.current).toHaveProperty('disconnect')
  })

  it('initial status is disconnected (standalone mode)', () => {
    const { result } = renderHook(() => useSSE())
    expect(result.current.status).toBe('disconnected')
  })

  it('initial metrics are null', () => {
    const { result } = renderHook(() => useSSE())
    expect(result.current.latestMetrics).toBeNull()
  })

  it('initial metricsHistory is empty', () => {
    const { result } = renderHook(() => useSSE())
    expect(result.current.metricsHistory).toEqual([])
  })

  it('connect and disconnect are functions', () => {
    const { result } = renderHook(() => useSSE())
    expect(typeof result.current.connect).toBe('function')
    expect(typeof result.current.disconnect).toBe('function')
  })
})
