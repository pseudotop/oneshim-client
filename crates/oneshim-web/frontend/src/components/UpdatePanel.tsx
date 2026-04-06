import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { fetchUpdateStatus, postUpdateAction, type UpdateAction, type UpdateStatus } from '../api/client'
import { useUpdateStream } from '../hooks/useUpdateStream'
import { typography } from '../styles/tokens'
import { cn } from '../utils/cn'
import { Badge, Button, Card, CardTitle, Spinner } from './ui'

type UpdatePanelProps = {
  compact?: boolean
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

export default function UpdatePanel({ compact = false }: UpdatePanelProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const stream = useUpdateStream()
  const [nowMs, setNowMs] = useState(() => Date.now())

  useEffect(() => {
    const timer = window.setInterval(() => setNowMs(Date.now()), 5000)
    return () => window.clearInterval(timer)
  }, [])

  const { data, isLoading } = useQuery<UpdateStatus>({
    queryKey: ['update-status'],
    queryFn: fetchUpdateStatus,
    refetchInterval: stream.status === 'connected' ? false : 15000,
    retry: 1,
  })

  const status = useMemo(() => stream.latest ?? data, [stream.latest, data])

  const freshness = useMemo(() => {
    if (!status?.updated_at) {
      return { ageSec: null, stale: true, severelyStale: true }
    }

    const timestamp = Date.parse(status.updated_at)
    if (!Number.isFinite(timestamp)) {
      return { ageSec: null, stale: true, severelyStale: true }
    }

    const ageSec = Math.max(0, Math.floor((nowMs - timestamp) / 1000))
    return {
      ageSec,
      stale: ageSec > 15,
      severelyStale: ageSec > 60,
    }
  }, [nowMs, status?.updated_at])

  const actionMutation = useMutation({
    mutationFn: (action: UpdateAction) => postUpdateAction(action),
    onSuccess: (response) => {
      queryClient.setQueryData(['update-status'], response.status)
      queryClient.invalidateQueries({ queryKey: ['update-status'] })
    },
  })

  const canApproveOrDefer = status?.phase === 'PendingApproval' && !freshness.severelyStale

  const phaseLabel = useMemo(() => {
    const phase = status?.phase
    if (phase === 'PendingApproval') return t('updates.pendingApproval')
    if (phase === 'Installing') return t('updates.installing')
    if (phase === 'Updated') return t('updates.updated')
    if (phase === 'Deferred') return t('updates.deferred')
    if (phase === 'Error') return t('updates.error')
    if (phase === 'Checking') return t('updates.checking')
    return t('updates.idle')
  }, [status?.phase, t])

  if (isLoading && !status) {
    return (
      <Card variant="default" padding="lg">
        <div className="flex items-center space-x-2">
          <Spinner size="sm" />
          <span className="text-content-secondary">{t('updates.loading')}</span>
        </div>
      </Card>
    )
  }

  return (
    <Card variant="default" padding="lg">
      <div className="mb-4 flex items-center justify-between">
        <CardTitle>{t('updates.title')}</CardTitle>
        <div className="flex items-center space-x-2">
          <Badge color={stream.status === 'connected' ? 'success' : 'warning'} size="sm">
            {stream.status === 'connected' ? t('updates.live') : t('updates.polling')}
          </Badge>
          <Badge color={freshness.stale ? 'warning' : 'success'} size="sm">
            {freshness.stale ? t('updates.stale') : t('updates.fresh')}
          </Badge>
          <Badge
            color={status?.phase === 'Error' ? 'error' : status?.phase === 'PendingApproval' ? 'warning' : 'info'}
            size="sm"
          >
            {phaseLabel}
          </Badge>
        </div>
      </div>

      <p className={cn('text-content-strong', typography.body)}>{status?.message ?? t('updates.statusUnavailable')}</p>

      <p className={cn('mt-2 text-content-secondary', typography.caption)}>
        {freshness.ageSec === null
          ? t('updates.lastUpdateUnknown')
          : t('updates.lastUpdateAge', { seconds: freshness.ageSec })}
      </p>

      {freshness.severelyStale && (
        <div className={cn('mt-2 text-semantic-warning', typography.caption)}>{t('updates.staleActionBlocked')}</div>
      )}

      {(stream.lastError || stream.retryCount > 0) && (
        <div className={cn('mt-2 text-semantic-warning', typography.caption)}>
          {stream.lastError ? t('updates.streamIssue') : t('updates.reconnecting', { count: stream.retryCount })}
        </div>
      )}

      {status?.pending && (
        <div
          className={cn('mt-3 space-y-1 rounded-lg border border-muted p-3 text-content-secondary', typography.caption)}
        >
          <div>
            {t('updates.currentVersion')}: {status.pending.current_version}
          </div>
          <div>
            {t('updates.latestVersion')}: {status.pending.latest_version}
          </div>
          <a href={status.pending.release_url} target="_blank" rel="noreferrer" className="text-brand-text underline">
            {t('updates.openRelease')}
          </a>
          {status.pending.download_size_bytes != null && status.pending.download_size_bytes > 0 && (
            <p className={cn('mt-1', typography.caption, 'text-content-tertiary')}>
              {t('updates.downloadSize', { size: formatBytes(status.pending.download_size_bytes) })}
            </p>
          )}
          {status.pending.release_notes && (
            <details className="mt-3">
              <summary className={cn('cursor-pointer select-none', typography.caption, 'text-content-secondary')}>
                {t('updates.releaseNotes')}
              </summary>
              <pre
                className={cn(
                  'mt-2 max-h-64 overflow-y-auto whitespace-pre-wrap rounded-md bg-surface-secondary p-3',
                  typography.caption,
                )}
              >
                {status.pending.release_notes}
              </pre>
            </details>
          )}
        </div>
      )}

      {!compact && (
        <div className={cn('mt-3 text-content-secondary', typography.caption)}>
          {status?.auto_install ? t('updates.autoInstallOn') : t('updates.autoInstallOff')}
        </div>
      )}

      <div className="mt-4 flex flex-wrap gap-2">
        <Button
          type="button"
          variant="secondary"
          size="sm"
          isLoading={actionMutation.isPending}
          onClick={() => actionMutation.mutate('CheckNow')}
        >
          {t('updates.checkNow')}
        </Button>
        {status?.phase === 'PendingApproval' && (
          <>
            <Button
              type="button"
              variant="primary"
              size="sm"
              isLoading={actionMutation.isPending}
              onClick={() => actionMutation.mutate('Approve')}
              disabled={!canApproveOrDefer}
            >
              {t('updates.approve')}
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              isLoading={actionMutation.isPending}
              onClick={() => actionMutation.mutate('Defer')}
              disabled={!canApproveOrDefer}
            >
              {t('updates.defer')}
            </Button>
          </>
        )}
      </div>

      {actionMutation.isError && (
        <div className={cn('mt-3 text-semantic-error', typography.caption)}>
          {(actionMutation.error as Error).message}
        </div>
      )}
    </Card>
  )
}
