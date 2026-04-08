import { renderHook } from '@testing-library/react'
import type { ReactNode } from 'react'
import { MemoryRouter } from 'react-router-dom'
import { describe, expect, it } from 'vitest'
import { useCurrentRoute } from '../useCurrentRoute'

function wrapperForPath(path: string) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return <MemoryRouter initialEntries={[path]}>{children}</MemoryRouter>
  }
}

describe('useCurrentRoute', () => {
  it('resolves root "/" to the root route node (no child when at default)', () => {
    const { result } = renderHook(() => useCurrentRoute(), { wrapper: wrapperForPath('/') })
    expect(result.current.node.path).toBe('/')
    // At the bare "/" path, before redirect to defaultChild, child is null
    expect(result.current.child).toBeNull()
  })

  it('resolves "/overview" to the root route with overview as active child', () => {
    const { result } = renderHook(() => useCurrentRoute(), { wrapper: wrapperForPath('/overview') })
    expect(result.current.node.path).toBe('/')
    expect(result.current.child?.path).toBe('overview')
  })

  it('resolves "/monitoring" to the root route with monitoring as active child', () => {
    const { result } = renderHook(() => useCurrentRoute(), { wrapper: wrapperForPath('/monitoring') })
    expect(result.current.node.path).toBe('/')
    expect(result.current.child?.path).toBe('monitoring')
  })

  it('resolves "/focus/score" to /focus with score as active child', () => {
    const { result } = renderHook(() => useCurrentRoute(), { wrapper: wrapperForPath('/focus/score') })
    expect(result.current.node.path).toBe('/focus')
    expect(result.current.child?.path).toBe('score')
  })

  it('resolves "/focus/sessions" to /focus with sessions as active child', () => {
    const { result } = renderHook(() => useCurrentRoute(), { wrapper: wrapperForPath('/focus/sessions') })
    expect(result.current.node.path).toBe('/focus')
    expect(result.current.child?.path).toBe('sessions')
  })

  it('resolves "/automation/policies" to /automation with policies as active child', () => {
    const { result } = renderHook(() => useCurrentRoute(), { wrapper: wrapperForPath('/automation/policies') })
    expect(result.current.node.path).toBe('/automation')
    expect(result.current.child?.path).toBe('policies')
  })

  it('resolves "/settings/ai-automation" to /settings with ai-automation as active child', () => {
    const { result } = renderHook(() => useCurrentRoute(), {
      wrapper: wrapperForPath('/settings/ai-automation'),
    })
    expect(result.current.node.path).toBe('/settings')
    expect(result.current.child?.path).toBe('ai-automation')
  })

  it('resolves a childless leaf route "/day" with no active child', () => {
    const { result } = renderHook(() => useCurrentRoute(), { wrapper: wrapperForPath('/day') })
    expect(result.current.node.path).toBe('/day')
    expect(result.current.child).toBeNull()
  })

  it('resolves a childless leaf route "/chat" with no active child', () => {
    const { result } = renderHook(() => useCurrentRoute(), { wrapper: wrapperForPath('/chat') })
    expect(result.current.node.path).toBe('/chat')
    expect(result.current.child).toBeNull()
  })

  it('resolves an unknown path to the root route as fallback', () => {
    const { result } = renderHook(() => useCurrentRoute(), { wrapper: wrapperForPath('/nonexistent-route') })
    expect(result.current.node.path).toBe('/')
    expect(result.current.child).toBeNull()
  })
})
