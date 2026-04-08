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

  it('dispatches route-error-reset CustomEvent on frontend-recovery reset-route signal', async () => {
    const { invalidateQueries, listeners } = await renderBridgeHarness()

    const dispatchSpy = vi.spyOn(window, 'dispatchEvent')

    act(() => {
      listeners.get('frontend-recovery')?.({
        payload: {
          strategy: 'reset-route',
          route: '/focus',
          reason: 'render crash',
        },
      })
    })

    // Queries are invalidated
    expect(invalidateQueries).toHaveBeenCalled()

    // CustomEvent dispatched with the right detail
    const customEventCall = dispatchSpy.mock.calls.find(([event]) => (event as Event).type === 'route-error-reset')
    expect(customEventCall).toBeDefined()
    const event = customEventCall?.[0] as CustomEvent
    expect(event.detail).toEqual({ route: '/focus' })

    dispatchSpy.mockRestore()
  })

  it('triggers full reload on frontend-recovery full-reload signal', async () => {
    const { listeners } = await renderBridgeHarness()

    // jsdom does not implement window.location.reload — stub it
    const reloadMock = vi.fn()
    const originalLocation = window.location
    Object.defineProperty(window, 'location', {
      configurable: true,
      writable: true,
      value: { ...originalLocation, reload: reloadMock },
    })

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

    Object.defineProperty(window, 'location', {
      configurable: true,
      writable: true,
      value: originalLocation,
    })
  })

  it('ignores frontend-recovery payloads that fail the type guard', async () => {
    const { invalidateQueries, listeners } = await renderBridgeHarness()
    const dispatchSpy = vi.spyOn(window, 'dispatchEvent')

    act(() => {
      // Missing required fields
      listeners.get('frontend-recovery')?.({ payload: { strategy: 42 } })
    })

    expect(invalidateQueries).not.toHaveBeenCalled()
    const customEventCall = dispatchSpy.mock.calls.find(([event]) => (event as Event).type === 'route-error-reset')
    expect(customEventCall).toBeUndefined()

    dispatchSpy.mockRestore()
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
