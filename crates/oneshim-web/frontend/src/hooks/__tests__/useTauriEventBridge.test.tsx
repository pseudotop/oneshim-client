import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, render, screen, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { MemoryRouter, useLocation } from 'react-router-dom'
import { afterEach, describe, expect, it, vi } from 'vitest'

type EventCallback = (event: { payload?: unknown }) => void

type RenderBridgeHarnessOptions = {
  expectedListenCalls?: number
  listenImpl?: (eventName: string, callback: EventCallback) => Promise<() => void>
}

async function renderBridgeHarness({ expectedListenCalls = 4, listenImpl }: RenderBridgeHarnessOptions = {}) {
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
        <MemoryRouter initialEntries={['/']}>{children}</MemoryRouter>
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

    expect(unlistenCallbacks).toHaveLength(4)
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
})
