import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { IS_TAURI } from '../utils/platform'

type TauriEventPayload = {
  payload?: unknown
}

function isRoutePath(payload: unknown): payload is string {
  return typeof payload === 'string' && payload.startsWith('/')
}

export function useTauriEventBridge() {
  const navigate = useNavigate()
  const location = useLocation()
  const queryClient = useQueryClient()
  const navigateRef = useRef(navigate)
  const pathnameRef = useRef(location.pathname)
  const queryClientRef = useRef(queryClient)

  navigateRef.current = navigate
  pathnameRef.current = location.pathname
  queryClientRef.current = queryClient

  useEffect(() => {
    if (!IS_TAURI) {
      return
    }

    let disposed = false
    let unlistenCallbacks: Array<() => void> = []

    const navigateTo = (path: string) => {
      if (pathnameRef.current === path) {
        return
      }
      navigateRef.current(path)
    }

    const refreshUpdateStatus = () => {
      void queryClientRef.current.invalidateQueries({ queryKey: ['update-status'] })
    }

    const refreshAutomationStatus = () => {
      void queryClientRef.current.invalidateQueries({ queryKey: ['automationStatus'] })
    }

    const registerListeners = async () => {
      try {
        const { listen } = await import('@tauri-apps/api/event')
        const listeners = await Promise.all([
          listen('navigate', (event: TauriEventPayload) => {
            if (isRoutePath(event.payload)) {
              navigateTo(event.payload)
            }
          }),
          listen('tray-toggle-automation', () => {
            refreshAutomationStatus()
            navigateTo('/automation')
          }),
          listen('tray-approve-update', () => {
            refreshUpdateStatus()
            navigateTo('/updates')
          }),
          listen('tray-defer-update', () => {
            refreshUpdateStatus()
            navigateTo('/updates')
          }),
        ])

        if (disposed) {
          listeners.forEach((unlisten) => unlisten())
          return
        }

        unlistenCallbacks = listeners
      } catch {
        // Browser mode or unavailable Tauri event bridge.
      }
    }

    void registerListeners()

    return () => {
      disposed = true
      unlistenCallbacks.forEach((unlisten) => unlisten())
      unlistenCallbacks = []
    }
  }, [])
}
