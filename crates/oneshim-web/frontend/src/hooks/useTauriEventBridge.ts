import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { useLocation, useNavigate } from 'react-router-dom'
import { IS_TAURI } from '../utils/platform'
import { addToast } from './useToast'

type TauriEventPayload = {
  payload?: unknown
}

type IntegrationPromptPayload = {
  title?: unknown
  body?: unknown
}

function isRoutePath(payload: unknown): payload is string {
  return typeof payload === 'string' && payload.startsWith('/')
}

function isIntegrationPromptPayload(payload: unknown): payload is IntegrationPromptPayload {
  return typeof payload === 'object' && payload !== null
}

export function useTauriEventBridge() {
  const navigate = useNavigate()
  const location = useLocation()
  const queryClient = useQueryClient()
  const { t } = useTranslation()
  const navigateRef = useRef(navigate)
  const pathnameRef = useRef(location.pathname)
  const queryClientRef = useRef(queryClient)
  const tRef = useRef(t)

  navigateRef.current = navigate
  pathnameRef.current = location.pathname
  queryClientRef.current = queryClient
  tRef.current = t

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
          !(await registerListener('automation:quick-access', () => {
            refreshAutomationStatus()
            navigateTo('/automation')
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

        if (
          !(await registerListener('oauth-reauth-required', () => {
            addToast('warning', tRef.current('settingsOAuth.reauthDescription'), 8000)
            navigateTo('/settings')
          }))
        ) {
          return
        }

        if (
          !(await registerListener('integration-proactive-prompt', (event: TauriEventPayload) => {
            if (!isIntegrationPromptPayload(event.payload)) {
              return
            }
            const title = typeof event.payload.title === 'string' ? event.payload.title.trim() : ''
            const body = typeof event.payload.body === 'string' ? event.payload.body.trim() : ''
            const message = [title, body].filter(Boolean).join(': ')
            if (message.length > 0) {
              addToast('info', message, 10000)
            }
          }))
        ) {
          return
        }

        unlistenCallbacks = pendingUnlistenCallbacks
      } catch {
        for (const unlisten of pendingUnlistenCallbacks) unlisten()
        pendingUnlistenCallbacks = []
        // Browser mode or unavailable Tauri event bridge.
      }
    }

    void registerListeners()

    return () => {
      disposed = true
      for (const unlisten of unlistenCallbacks) unlisten()
      unlistenCallbacks = []
    }
  }, [])
}
