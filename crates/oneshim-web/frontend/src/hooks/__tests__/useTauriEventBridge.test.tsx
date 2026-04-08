import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, render, screen, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { useLocation } from 'react-router-dom'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { AppMemoryRouter } from '../../router/future'

type EventCallback = (event: { payload?: unknown }) => void

type RenderBridgeHarnessOptions = {
  expectedListenCalls?: number
  listenImpl?: (eventName: string, callback: EventCallback) => Promise<() => void>
}

async function renderBridgeHarness({ expectedListenCalls = 9, listenImpl }: RenderBridgeHarnessOptions = {}) {
  const listeners = new Map<string, EventCallback>()
  const unlistenCallbacks: Array<ReturnType<typeof vi.fn>> = []
  const defaultListenImpl = async (eventName: string, callback: EventCallback) => {
    listeners.set(eventName, callback)
    const unlisten = vi.fn(() => {
      listeners.delete(eventName)
    })
    unlistenCallbacks.push(unlisten)
    return unlisten
  }
  const listen = vi.fn(listenImpl ?? defaultListenImpl)

  vi.doMock('../../utils/platform', () => ({
    IS_TAURI: true,
  }))
  vi.doMock('@tauri-apps/api/event', () => ({
    listen,
  }))

  const { useTauriEventBridge } = await import('../useTauriEventBridge')
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  })
  const invalidateQueries = vi.spyOn(queryClient, 'invalidateQueries')

  function LocationProbe() {
    useTauriEventBridge()
    const location = useLocation()
    return <div data-testid="path">{location.pathname}</div>
  }

  function wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>
        <AppMemoryRouter initialEntries={['/']}>{children}</AppMemoryRouter>
      </QueryClientProvider>
    )
  }

  const rendered = render(<LocationProbe />, { wrapper })
  await waitFor(() => expect(listen).toHaveBeenCalledTimes(expectedListenCalls))

  return {
    ...rendered,
    invalidateQueries,
    listeners,
    listen,
    unlistenCallbacks,
  }
}

describe('useTauriEventBridge', () => {
  afterEach(() => {
    vi.resetModules()
    vi.clearAllMocks()
  })

  it('routes navigate payloads and cleans up listeners on unmount', async () => {
    const { listeners, unlistenCallbacks, unmount } = await renderBridgeHarness()

    act(() => {
      listeners.get('navigate')?.({ payload: '/settings' })
    })

    expect(screen.getByTestId('path')).toHaveTextContent('/settings')

    unmount()

    expect(unlistenCallbacks).toHaveLength(9)
    unlistenCallbacks.forEach((unlisten) => {
      expect(unlisten).toHaveBeenCalledTimes(1)
    })
  })

  it('routes tray automation and update events to their target pages', async () => {
    const { invalidateQueries, listeners } = await renderBridgeHarness()

    act(() => {
      listeners.get('tray-toggle-automation')?.({})
    })

    expect(screen.getByTestId('path')).toHaveTextContent('/settings')
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ['automationStatus'] })

    act(() => {
      listeners.get('tray-approve-update')?.({})
    })

    expect(screen.getByTestId('path')).toHaveTextContent('/updates')
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ['update-status'] })

    act(() => {
      listeners.get('tray-defer-update')?.({})
    })

    expect(screen.getByTestId('path')).toHaveTextContent('/updates')
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ['update-status'] })
  })

  it('cleans up earlier listeners when a later registration fails', async () => {
    let callCount = 0
    const listenerMap = new Map<string, EventCallback>()
    const unlistenCallbacks: Array<ReturnType<typeof vi.fn>> = []

    await renderBridgeHarness({
      expectedListenCalls: 3,
      listenImpl: async (eventName, callback) => {
        callCount += 1
        if (callCount === 3) {
          throw new Error('listen failed')
        }

        listenerMap.set(eventName, callback)
        const unlisten = vi.fn(() => {
          listenerMap.delete(eventName)
        })
        unlistenCallbacks.push(unlisten)
        return unlisten
      },
    })

    await waitFor(() => expect(unlistenCallbacks).toHaveLength(2))
    expect(listenerMap.size).toBe(0)
    unlistenCallbacks.forEach((unlisten) => {
      expect(unlisten).toHaveBeenCalledTimes(1)
    })
  })

  it('calls notifyRouteRecovery on frontend-recovery reset-route signal', async () => {
    // Mock the recoverySignals module BEFORE importing useTauriEventBridge
    const notifySpy = vi.fn()
    vi.doMock('../../routes/recoverySignals', () => ({
      notifyRouteRecovery: notifySpy,
    }))

    const { listeners } = await renderBridgeHarness()

    act(() => {
      listeners.get('frontend-recovery')?.({
        payload: {
          strategy: 'reset-route',
          route: '/focus',
          reason: 'render crash',
        },
      })
    })

    expect(notifySpy).toHaveBeenCalledWith('/focus')
  })

  it('does NOT globally invalidate queries on reset-route (boundary handles it)', async () => {
    vi.doMock('../../routes/recoverySignals', () => ({
      notifyRouteRecovery: vi.fn(),
    }))

    const { invalidateQueries, listeners } = await renderBridgeHarness()

    act(() => {
      listeners.get('frontend-recovery')?.({
        payload: {
          strategy: 'reset-route',
          route: '/focus',
          reason: 'render crash',
        },
      })
    })

    // Bridge should NOT trigger a global invalidate — that's the boundary's job.
    // Earlier impl invalidated here AND in the boundary (IA-3 double invalidation).
    expect(invalidateQueries).not.toHaveBeenCalled()
  })

  it('triggers full reload on frontend-recovery full-reload signal', async () => {
    vi.doMock('../../routes/recoverySignals', () => ({
      notifyRouteRecovery: vi.fn(),
    }))
    const { listeners } = await renderBridgeHarness()

    // jsdom does not implement window.location.reload — stub it
    const reloadMock = vi.fn()
    const originalLocation = window.location
    Object.defineProperty(window, 'location', {
      configurable: true,
      writable: true,
      value: { ...originalLocation, reload: reloadMock },
    })

    // Spy on the before-full-reload event used by unsaved-data guards
    const dispatchSpy = vi.spyOn(window, 'dispatchEvent')

    act(() => {
      listeners.get('frontend-recovery')?.({
        payload: {
          strategy: 'full-reload',
          route: '/focus',
          reason: 'critical escalation',
        },
      })
    })

    expect(reloadMock).toHaveBeenCalledTimes(1)
    // Unsaved-data guards are notified via a separate event
    const guardEvent = dispatchSpy.mock.calls.find(([event]) => (event as Event).type === 'oneshim:before-full-reload')
    expect(guardEvent).toBeDefined()

    dispatchSpy.mockRestore()
    Object.defineProperty(window, 'location', {
      configurable: true,
      writable: true,
      value: originalLocation,
    })
  })

  it('ignores frontend-recovery payloads that fail the type guard', async () => {
    const notifySpy = vi.fn()
    vi.doMock('../../routes/recoverySignals', () => ({
      notifyRouteRecovery: notifySpy,
    }))

    const { invalidateQueries, listeners } = await renderBridgeHarness()

    act(() => {
      // Missing required fields
      listeners.get('frontend-recovery')?.({ payload: { strategy: 42 } })
    })

    expect(invalidateQueries).not.toHaveBeenCalled()
    expect(notifySpy).not.toHaveBeenCalled()
  })

  it('shows a toast for inbound integration prompts', async () => {
    const addToast = vi.fn()
    vi.doMock('../useToast', () => ({
      addToast,
    }))

    const { listeners } = await renderBridgeHarness()

    act(() => {
      listeners.get('integration-proactive-prompt')?.({
        payload: {
          title: 'Review teammate note',
          body: 'A follow-up prompt is waiting in the inbox',
        },
      })
    })

    expect(addToast).toHaveBeenCalledWith(
      'info',
      'Review teammate note: A follow-up prompt is waiting in the inbox',
      10000,
    )
  })
})
