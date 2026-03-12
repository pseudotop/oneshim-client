import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, render, screen, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { MemoryRouter, useLocation } from 'react-router-dom'
import { afterEach, describe, expect, it, vi } from 'vitest'

type EventCallback = (event: { payload?: unknown }) => void

async function renderBridgeHarness() {
  const listeners = new Map<string, EventCallback>()
  const unlistenCallbacks: Array<ReturnType<typeof vi.fn>> = []
  const listen = vi.fn(async (eventName: string, callback: EventCallback) => {
    listeners.set(eventName, callback)
    const unlisten = vi.fn(() => {
      listeners.delete(eventName)
    })
    unlistenCallbacks.push(unlisten)
    return unlisten
  })

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
  await waitFor(() => expect(listen).toHaveBeenCalledTimes(4))

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

    expect(screen.getByTestId('path')).toHaveTextContent('/automation')
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
})
