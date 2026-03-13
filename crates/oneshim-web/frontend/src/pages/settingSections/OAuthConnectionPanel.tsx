import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { OAuthConnectionStatus, OAuthFlowStatus } from '../../api/client'
import {
  oauthCancelFlow,
  oauthConnectionStatus,
  oauthFlowStatus,
  oauthRevoke,
  oauthStartFlow,
} from '../../api/client'
import { isStandaloneModeEnabled } from '../../api/standalone'
import { Button, Card } from '../../components/ui'

type PanelState =
  | { phase: 'loading' }
  | { phase: 'disconnected' }
  | { phase: 'connecting'; flowId: string }
  | { phase: 'connected'; status: OAuthConnectionStatus }
  | { phase: 'error'; message: string }

const POLL_INTERVAL_MS = 1500

interface OAuthConnectionPanelProps {
  providerId: string
  providerName: string
}

export default function OAuthConnectionPanel({ providerId, providerName }: OAuthConnectionPanelProps) {
  const { t } = useTranslation()
  const [state, setState] = useState<PanelState>({ phase: 'loading' })
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null)

  const clearPoll = useCallback(() => {
    if (pollRef.current) {
      clearInterval(pollRef.current)
      pollRef.current = null
    }
  }, [])

  const refreshStatus = useCallback(async () => {
    if (isStandaloneModeEnabled()) {
      setState({ phase: 'disconnected' })
      return
    }
    try {
      const status = await oauthConnectionStatus(providerId)
      if (status.connected) {
        setState({ phase: 'connected', status })
      } else {
        setState({ phase: 'disconnected' })
      }
    } catch (err) {
      setState({ phase: 'error', message: String(err) })
    }
  }, [providerId])

  useEffect(() => {
    refreshStatus()
    return clearPoll
  }, [refreshStatus, clearPoll])

  const handleConnect = useCallback(async () => {
    try {
      const handle = await oauthStartFlow(providerId)
      setState({ phase: 'connecting', flowId: handle.flow_id })

      // Open auth URL in system browser
      window.open(handle.auth_url, '_blank', 'noopener')

      // Poll for completion
      pollRef.current = setInterval(async () => {
        try {
          const flowState: OAuthFlowStatus = await oauthFlowStatus(handle.flow_id)
          if (flowState.status === 'completed') {
            clearPoll()
            await refreshStatus()
          } else if (flowState.status === 'failed') {
            clearPoll()
            setState({ phase: 'error', message: flowState.error })
          } else if (flowState.status === 'cancelled') {
            clearPoll()
            setState({ phase: 'disconnected' })
          }
        } catch {
          clearPoll()
          setState({ phase: 'error', message: 'Failed to check flow status' })
        }
      }, POLL_INTERVAL_MS)
    } catch (err) {
      const msg = String(err)
      if (msg.includes('1455') || msg.includes('already in use')) {
        setState({ phase: 'error', message: t('settingsOAuth.portConflict') })
      } else {
        setState({ phase: 'error', message: msg })
      }
    }
  }, [providerId, clearPoll, refreshStatus, t])

  const handleCancel = useCallback(async () => {
    if (state.phase === 'connecting') {
      clearPoll()
      try {
        await oauthCancelFlow(state.flowId)
      } catch {
        // ignore cancel errors
      }
      setState({ phase: 'disconnected' })
    }
  }, [state, clearPoll])

  const handleDisconnect = useCallback(async () => {
    try {
      await oauthRevoke(providerId)
      setState({ phase: 'disconnected' })
    } catch (err) {
      setState({ phase: 'error', message: String(err) })
    }
  }, [providerId])

  return (
    <Card variant="default" padding="md" className="space-y-3">
      <div className="flex items-center gap-2">
        <h4 className="font-medium text-content-strong text-sm">
          {providerName} {t('settingsOAuth.title')}
        </h4>
        <span className="rounded-full bg-amber-100 px-2 py-0.5 text-amber-800 text-xs dark:bg-amber-900/30 dark:text-amber-300">
          {t('settingsOAuth.experimental')}
        </span>
      </div>

      {state.phase === 'loading' && (
        <p className="text-content-secondary text-sm">{t('settingsOAuth.loading')}</p>
      )}

      {state.phase === 'disconnected' && (
        <div className="space-y-2">
          <p className="text-content-secondary text-sm">{t('settingsOAuth.disconnectedDescription')}</p>
          <Button type="button" variant="primary" size="sm" onClick={handleConnect}>
            {t('settingsOAuth.connect')}
          </Button>
        </div>
      )}

      {state.phase === 'connecting' && (
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            <div className="h-4 w-4 animate-spin rounded-full border-2 border-accent border-t-transparent" />
            <p className="text-content-secondary text-sm">{t('settingsOAuth.connecting')}</p>
          </div>
          <Button type="button" variant="secondary" size="sm" onClick={handleCancel}>
            {t('settingsOAuth.cancel')}
          </Button>
        </div>
      )}

      {state.phase === 'connected' && (
        <div className="space-y-2">
          <div className="flex items-center gap-2">
            <span className="h-2 w-2 rounded-full bg-emerald-500" />
            <p className="text-content-secondary text-sm">{t('settingsOAuth.connected')}</p>
          </div>
          {state.status.expires_at && (
            <p className="text-content-secondary text-xs">
              {t('settingsOAuth.expiresAt')}: {new Date(state.status.expires_at).toLocaleString()}
            </p>
          )}
          <Button type="button" variant="secondary" size="sm" onClick={handleDisconnect}>
            {t('settingsOAuth.disconnect')}
          </Button>
        </div>
      )}

      {state.phase === 'error' && (
        <div className="space-y-2">
          <p className="text-sm text-red-600 dark:text-red-400">{state.message}</p>
          <Button type="button" variant="secondary" size="sm" onClick={refreshStatus}>
            {t('settingsOAuth.retry')}
          </Button>
        </div>
      )}

      <p className="text-content-muted text-xs">{t('settingsOAuth.disclaimer')}</p>
    </Card>
  )
}
