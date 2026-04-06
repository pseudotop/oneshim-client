import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Clock, Download, RefreshCw, RotateCcw, Shield } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  fetchSettings,
  fetchUpdateStatus,
  postUpdateAction,
  updateSettings,
  type AppSettings,
  type UpdateAction,
  type UpdateChannel,
  type UpdateStatus,
} from '../api/client'
import UpdatePanel from '../components/UpdatePanel'
import { Badge, Button, Card, CardTitle, Spinner } from '../components/ui'
import { addToast } from '../hooks/useToast'
import { colors, typography } from '../styles/tokens'
import { cn } from '../utils/cn'

declare const __APP_VERSION__: string

const FEATURE_ICONS = [RefreshCw, Shield, RotateCcw, Clock] as const

const CHANNEL_OPTIONS: { value: UpdateChannel; labelKey: string; descKey: string }[] = [
  {
    value: 'stable',
    labelKey: 'settings.channelStable',
    descKey: 'updates.channelStableDesc',
  },
  {
    value: 'pre_release',
    labelKey: 'settings.channelPreRelease',
    descKey: 'updates.channelPreReleaseDesc',
  },
  {
    value: 'nightly',
    labelKey: 'settings.channelNightly',
    descKey: 'updates.channelNightlyDesc',
  },
]

export default function Updates() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [savingChannel, setSavingChannel] = useState(false)

  const featureKeys = [
    'updates.featureAuto',
    'updates.featureIntegrity',
    'updates.featureRollback',
    'updates.featureMinimal',
  ] as const

  const { data: settings } = useQuery<AppSettings>({
    queryKey: ['settings'],
    queryFn: fetchSettings,
    retry: 1,
  })

  const { data: status } = useQuery<UpdateStatus>({
    queryKey: ['update-status'],
    queryFn: fetchUpdateStatus,
    refetchInterval: 15000,
    retry: 1,
  })

  const settingsMutation = useMutation({
    mutationFn: (updated: AppSettings) => updateSettings(updated),
    onSuccess: (data) => {
      queryClient.setQueryData(['settings'], data)
      addToast('success', t('updates.channelSaved', 'Update channel saved'))
    },
    onError: () => {
      addToast('error', t('updates.channelSaveFailed', 'Failed to save update channel'))
    },
    onSettled: () => setSavingChannel(false),
  })

  const actionMutation = useMutation({
    mutationFn: (action: UpdateAction) => postUpdateAction(action),
    onSuccess: (response) => {
      queryClient.setQueryData(['update-status'], response.status)
      queryClient.invalidateQueries({ queryKey: ['update-status'] })
    },
  })

  const handleChannelChange = (channel: UpdateChannel) => {
    if (!settings) return
    setSavingChannel(true)
    const updated: AppSettings = {
      ...settings,
      update: { ...settings.update, channel },
    }
    settingsMutation.mutate(updated)
  }

  const currentChannel = settings?.update?.channel ?? 'stable'

  const isDownloading = status?.phase === 'Downloading' || status?.phase === 'Installing'

  const versionSummary = useMemo(() => {
    if (!status?.pending) return null
    return {
      current: status.pending.current_version,
      latest: status.pending.latest_version,
      releaseUrl: status.pending.release_url,
      releaseName: status.pending.release_name,
      publishedAt: status.pending.published_at,
    }
  }, [status?.pending])

  return (
    <div className="min-h-full space-y-6 p-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h1 className={cn(typography.h1, colors.text.pageTitle)}>{t('updates.title')}</h1>
          <Badge color="info">{__APP_VERSION__}</Badge>
        </div>
        <div className="flex items-center gap-2">
          <Button
            type="button"
            variant="secondary"
            size="sm"
            isLoading={actionMutation.isPending}
            onClick={() => actionMutation.mutate('CheckNow')}
          >
            <RefreshCw size={14} className="mr-1.5" />
            {t('updates.checkNow')}
          </Button>
        </div>
      </div>

      {/* Live status panel */}
      <div id="section-status">
        <UpdatePanel />
      </div>

      {/* Version info + Download progress */}
      {(versionSummary || isDownloading) && (
        <Card id="section-version" variant="default" padding="lg">
          <CardTitle className="mb-4">{t('updates.versionInfo', 'Version Information')}</CardTitle>

          {versionSummary && (
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <div className="rounded-lg bg-surface-muted p-3">
                <span className="block text-content-secondary text-xs">{t('updates.currentVersion')}</span>
                <span className="font-mono text-content-strong text-sm">{versionSummary.current}</span>
              </div>
              <div className="rounded-lg bg-surface-muted p-3">
                <span className="block text-content-secondary text-xs">{t('updates.latestVersion')}</span>
                <span className="font-mono text-content-strong text-sm">{versionSummary.latest}</span>
              </div>
              {versionSummary.releaseName && (
                <div className="rounded-lg bg-surface-muted p-3 sm:col-span-2">
                  <span className="block text-content-secondary text-xs">
                    {t('updates.releaseName', 'Release')}
                  </span>
                  <span className="text-content-strong text-sm">{versionSummary.releaseName}</span>
                  {versionSummary.publishedAt && (
                    <span className="ml-2 text-content-secondary text-xs">
                      {new Date(versionSummary.publishedAt).toLocaleDateString()}
                    </span>
                  )}
                </div>
              )}
              {versionSummary.releaseUrl && (
                <div className="sm:col-span-2">
                  <a
                    href={versionSummary.releaseUrl}
                    target="_blank"
                    rel="noreferrer"
                    className="text-brand-text text-sm underline"
                  >
                    {t('updates.openRelease')}
                  </a>
                </div>
              )}
            </div>
          )}

          {isDownloading && (
            <div className="mt-4 flex items-center gap-3 rounded-lg border border-brand-muted bg-brand-muted/10 p-3">
              <Download size={18} className="shrink-0 text-brand-text" />
              <div className="flex-1">
                <span className="block text-content-strong text-sm">
                  {t('updates.downloadInProgress', 'Downloading update...')}
                </span>
                <div className="mt-1.5 h-1.5 w-full overflow-hidden rounded-full bg-surface-muted">
                  <div className="h-full animate-pulse rounded-full bg-brand-text" style={{ width: '60%' }} />
                </div>
              </div>
              <Spinner size="sm" />
            </div>
          )}
        </Card>
      )}

      {/* Channel selector */}
      <Card id="section-channel" variant="default" padding="lg">
        <CardTitle className="mb-4">{t('updates.channelTitle', 'Update Channel')}</CardTitle>
        <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
          {CHANNEL_OPTIONS.map((opt) => {
            const isActive = currentChannel === opt.value
            return (
              <button
                key={opt.value}
                type="button"
                disabled={savingChannel}
                onClick={() => handleChannelChange(opt.value)}
                className={cn(
                  'rounded-lg border p-3 text-left transition-colors',
                  isActive
                    ? 'border-brand-text bg-brand-muted/10'
                    : 'border-muted bg-surface hover:border-brand-muted hover:bg-surface-muted',
                )}
              >
                <div className="flex items-center justify-between">
                  <span className={cn('text-sm font-medium', isActive ? 'text-brand-text' : 'text-content-strong')}>
                    {t(opt.labelKey, opt.value)}
                  </span>
                  {isActive && <Badge color="success" size="sm">{t('updates.active', 'Active')}</Badge>}
                </div>
                <p className="mt-1 text-content-secondary text-xs">
                  {t(opt.descKey, '')}
                </p>
              </button>
            )
          })}
        </div>
        {savingChannel && (
          <div className="mt-3 flex items-center gap-2 text-content-secondary text-sm">
            <Spinner size="sm" />
            {t('updates.savingChannel', 'Saving...')}
          </div>
        )}
      </Card>

      {/* Features */}
      <Card id="section-features" variant="default" padding="lg">
        <CardTitle className="mb-4">{t('updates.featuresTitle')}</CardTitle>
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
          {featureKeys.map((key, index) => {
            const Icon = FEATURE_ICONS[index]

            return (
              <div key={key} className="flex items-start gap-3 rounded-lg bg-surface-muted p-3">
                <div className="mt-0.5 shrink-0 text-brand-text">
                  <Icon size={18} aria-hidden="true" />
                </div>
                <span className="text-content-strong text-sm">{t(key)}</span>
              </div>
            )
          })}
        </div>
      </Card>

      {/* Security policy */}
      <Card id="section-policy" variant="default" padding="lg">
        <CardTitle className="mb-3">{t('updates.policyTitle')}</CardTitle>
        <ul className="space-y-1 text-content-strong text-sm">
          <li>{t('updates.policyIntegrity')}</li>
          <li>{t('updates.policySignature')}</li>
          <li>{t('updates.policyRollback')}</li>
        </ul>
      </Card>
    </div>
  )
}
