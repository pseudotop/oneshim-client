import { useCallback, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { OAuthConnectionStatus, OAuthFlowStatus } from '../../api/client'
import { oauthCancelFlow, oauthConnectionStatus, oauthFlowStatus, oauthRevoke, oauthStartFlow } from '../../api/client'
import { Button, Card } from '../../components/ui'
import { isOAuthPanelAvailable } from './oauth-panel-support'

type ExpiryLevel = 'ok' | 'warning' | 'critical' | 'none';

function getExpiryLevel(expiresAt: string | null | undefined): ExpiryLevel {
  if (!expiresAt) return 'none';
  const remaining = new Date(expiresAt).getTime() - Date.now();
  const minutes = remaining / 60_000;
  if (minutes < 1) return 'critical';
  if (minutes <= 5) return 'warning';
  return 'ok';
}

const EXPIRY_BADGE_STYLES: Record<ExpiryLevel, string> = {
  ok: 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200',
  warning: 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200',
  critical: 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200',
  none: 'bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-400',
};

type PanelState =
  | { phase: 'unavailable' }
  | { phase: 'loading' }
  | { phase: 'disconnected' }
  | { phase: 'connecting'; authUrl: string; flowId: string }
  | { phase: 'connected'; status: OAuthConnectionStatus }
  | { phase: 'error'; detail?: string; message: string }

const POLL_INTERVAL_MS = 1500
const POLL_TIMEOUT_MS = 120000

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

  const toErrorState = useCallback(
    (error: unknown): PanelState => {
      const detail = error instanceof Error ? error.message : String(error)
      if (detail.includes('1455') || detail.includes('already in use')) {
        return { phase: 'error', detail, message: t('settingsOAuth.portConflict') }
      }
      if (detail.includes('not available') || detail.includes('unavailable')) {
        return { phase: 'error', detail, message: t('settingsOAuth.unavailable') }
      }
      return { phase: 'error', detail, message: t('settingsOAuth.genericError') }
    },
    [t],
  )

  const openAuthorizationPage = useCallback(
    (authUrl: string) => {
      const opened = window.open(authUrl, '_blank', 'noopener,noreferrer')
      if (!opened) {
        setState({ phase: 'error', message: t('settingsOAuth.openBrowserFailed') })
      }
    },
    [t],
  )

  const refreshStatus = useCallback(async () => {
    if (!isOAuthPanelAvailable()) {
      setState({ phase: 'unavailable' })
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
      setState(toErrorState(err))
    }
  }, [providerId, toErrorState])

  useEffect(() => {
    refreshStatus()
    return clearPoll
  }, [refreshStatus, clearPoll])

  // Auto-refresh status every 60s to update expiry badge
  useEffect(() => {
    if (state.phase !== 'connected') return;
    const timer = setInterval(() => refreshStatus(), 60_000);
    return () => clearInterval(timer);
  }, [state.phase, refreshStatus]);

  const handleConnect = useCallback(async () => {
    try {
      const handle = await oauthStartFlow(providerId)
      setState({ phase: 'connecting', authUrl: handle.auth_url, flowId: handle.flow_id })

      const startedAt = Date.now()

      // Poll for completion
      pollRef.current = setInterval(async () => {
        if (Date.now() - startedAt >= POLL_TIMEOUT_MS) {
          clearPoll()
          try {
            await oauthCancelFlow(handle.flow_id)
          } catch {
            // ignore timeout cleanup errors
          }
          setState({ phase: 'error', message: t('settingsOAuth.timeout') })
          return
        }

        try {
          const flowState: OAuthFlowStatus = await oauthFlowStatus(handle.flow_id)
          if (flowState.status === 'completed') {
            clearPoll()
            await refreshStatus()
          } else if (flowState.status === 'failed') {
            clearPoll()
            setState(toErrorState(flowState.error))
          } else if (flowState.status === 'cancelled') {
            clearPoll()
            setState({ phase: 'disconnected' })
          }
        } catch (err) {
          clearPoll()
          setState(toErrorState(err))
        }
      }, POLL_INTERVAL_MS)
    } catch (err) {
      setState(toErrorState(err))
    }
  }, [providerId, clearPoll, refreshStatus, t, toErrorState])

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
      setState(toErrorState(err))
    }
  }, [providerId, toErrorState])

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

      {state.phase === 'loading' && <p className="text-content-secondary text-sm">{t('settingsOAuth.loading')}</p>}

      {state.phase === 'unavailable' && (
        <p className="text-content-secondary text-sm">{t('settingsOAuth.desktopOnly')}</p>
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
          <p className="text-content-muted text-xs">{t('settingsOAuth.openBrowserHint')}</p>
          <div className="flex flex-wrap gap-2">
            <Button type="button" variant="primary" size="sm" onClick={() => openAuthorizationPage(state.authUrl)}>
              {t('settingsOAuth.openBrowser')}
            </Button>
            <Button type="button" variant="secondary" size="sm" onClick={handleCancel}>
              {t('settingsOAuth.cancel')}
            </Button>
          </div>
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
          {state.status.expires_at && (() => {
            const level = getExpiryLevel(state.status.expires_at);
            // When no refresh token, treat warning-level expiry as critical
            const effectiveLevel = (!state.status.has_refresh_token && level === 'warning') ? 'critical' : level;
            if (effectiveLevel === 'ok') return null;
            const label = effectiveLevel === 'critical'
              ? t('settingsOAuth.statusExpired')
              : t('settingsOAuth.statusExpiringSoon');
            return (
              <span className={`inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium ${EXPIRY_BADGE_STYLES[effectiveLevel]}`}>
                {label}
              </span>
            );
          })()}
          {state.status.has_refresh_token === false && (
            <p className="flex items-center gap-1.5 rounded bg-amber-50 px-2 py-1.5 text-amber-800 text-xs dark:bg-amber-900/20 dark:text-amber-300">
              <span aria-hidden="true">⚠</span>
              {t('settingsOAuth.noRefreshToken')}
            </p>
          )}
          <Button type="button" variant="secondary" size="sm" onClick={handleDisconnect}>
            {t('settingsOAuth.disconnect')}
          </Button>
        </div>
      )}

      {state.phase === 'error' && (
        <div className="space-y-2">
          <p className="text-red-600 text-sm dark:text-red-400">{state.message}</p>
          {state.detail && <p className="text-content-muted text-xs">{state.detail}</p>}
          <Button type="button" variant="secondary" size="sm" onClick={refreshStatus}>
            {t('settingsOAuth.retry')}
          </Button>
        </div>
      )}

      <p className="text-content-muted text-xs">{t('settingsOAuth.disclaimer')}</p>
    </Card>
  )
}
