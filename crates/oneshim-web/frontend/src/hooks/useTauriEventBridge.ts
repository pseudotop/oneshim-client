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
      let pendingUnlistenCallbacks: Array<() => void> = []

      try {
        const { listen } = await import('@tauri-apps/api/event')

        const registerListener = async (
          eventName: string,
          handler: (event: TauriEventPayload) => void,
        ): Promise<boolean> => {
          const unlisten = await listen(eventName, handler)
          if (disposed) {
            unlisten()
            return false
          }
          pendingUnlistenCallbacks.push(unlisten)
          return true
        }

        if (
          !(await registerListener('navigate', (event: TauriEventPayload) => {
            if (isRoutePath(event.payload)) {
              navigateTo(event.payload)
            }
          }))
        ) {
          return
        }

        if (
          !(await registerListener('tray-toggle-automation', () => {
            refreshAutomationStatus()
            navigateTo('/settings')
          }))
        ) {
          return
        }

        if (
          !(await registerListener('tray-approve-update', () => {
            refreshUpdateStatus()
            navigateTo('/updates')
          }))
        ) {
          return
        }

        if (
          !(await registerListener('tray-defer-update', () => {
            refreshUpdateStatus()
            navigateTo('/updates')
          }))
        ) {
          return
        }

        unlistenCallbacks = pendingUnlistenCallbacks
      } catch {
        pendingUnlistenCallbacks.forEach((unlisten) => unlisten())
        pendingUnlistenCallbacks = []
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
