import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { fetchUpdateStatus, postUpdateAction, type UpdateAction, type UpdateStatus } from '../api/client'
import { useUpdateStream } from '../hooks/useUpdateStream'
import { Badge, Button, Card, CardTitle, Spinner } from './ui'

type UpdatePanelProps = {
  compact?: boolean
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

      <p className="text-content-strong text-sm">{status?.message ?? t('updates.statusUnavailable')}</p>

      <p className="mt-2 text-content-secondary text-xs">
        {freshness.ageSec === null
          ? t('updates.lastUpdateUnknown')
          : t('updates.lastUpdateAge', { seconds: freshness.ageSec })}
      </p>

      {freshness.severelyStale && (
        <div className="mt-2 text-accent-amber text-xs">{t('updates.staleActionBlocked')}</div>
      )}

      {(stream.lastError || stream.retryCount > 0) && (
        <div className="mt-2 text-accent-amber text-xs">
          {stream.lastError ? t('updates.streamIssue') : t('updates.reconnecting', { count: stream.retryCount })}
        </div>
      )}

      {status?.pending && (
        <div className="mt-3 space-y-1 rounded-lg border border-muted p-3 text-content-secondary text-xs">
          <div>
            {t('updates.currentVersion')}: {status.pending.current_version}
          </div>
          <div>
            {t('updates.latestVersion')}: {status.pending.latest_version}
          </div>
          <a href={status.pending.release_url} target="_blank" rel="noreferrer" className="text-accent-teal underline">
            {t('updates.openRelease')}
          </a>
        </div>
      )}

      {!compact && (
        <div className="mt-3 text-content-secondary text-xs">
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
      </div>

      {actionMutation.isError && (
        <div className="mt-3 text-red-500 text-xs">{(actionMutation.error as Error).message}</div>
      )}
    </Card>
  )
}
