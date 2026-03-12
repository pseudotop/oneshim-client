/**
 * Data & Storage settings tab: storage stats, data export, retention, telemetry.
 */
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  type AppSettings,
  downloadBlob,
  type ExportDataType,
  type ExportFormat,
  exportData,
  fetchSettings,
  fetchStorageStats,
  type TelemetrySettings,
  updateSettings,
} from '../../api/client'
import { Button, Card, CardTitle, Input, Spinner } from '../../components/ui'
import { useToast } from '../../hooks/useToast'
import { colors, form } from '../../styles/tokens'
import { cn } from '../../utils/cn'
import { formatBytes, formatNumber } from '../../utils/formatters'
import ToggleRow from './ToggleRow'

function StorageCard({ label, value, subValue }: { label: string; value: string; subValue: string }) {
  return (
    <Card variant="elevated" padding="md">
      <div className={cn('text-sm', colors.text.secondary)}>{label}</div>
      <div className={cn('mt-1 font-bold text-2xl', colors.text.primary)}>{value}</div>
      <div className={cn('mt-1 text-xs', colors.text.tertiary)}>{subValue}</div>
    </Card>
  )
}

function ExportButton({
  label,
  description,
  onClick,
  loading,
}: {
  label: string
  description: string
  onClick: () => void
  loading: boolean
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={loading}
      className="flex flex-col items-start rounded-lg border border-DEFAULT bg-surface-muted p-4 transition-colors hover:border-brand-signal hover:bg-active disabled:cursor-not-allowed disabled:opacity-50"
    >
      <div className="flex items-center gap-2">
        <svg
          className={cn('h-5 w-5', colors.primary.text)}
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          aria-hidden="true"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"
          />
        </svg>
        <span className={cn('font-medium', colors.text.primary)}>{label}</span>
        {loading && <Spinner size="sm" className={colors.primary.text} />}
      </div>
      <span className={cn('mt-1 text-xs', colors.text.tertiary)}>{description}</span>
    </button>
  )
}

export default function DataStorageTab() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const toast = useToast()
  const [exportFormat, setExportFormat] = useState<ExportFormat>('json')
  const [exportLoading, setExportLoading] = useState<ExportDataType | null>(null)

  const { data: settings, isLoading: settingsLoading } = useQuery({
    queryKey: ['settings'],
    queryFn: fetchSettings,
  })

  const { data: storageStats, isLoading: storageLoading } = useQuery({
    queryKey: ['storage-stats'],
    queryFn: fetchStorageStats,
  })

  const [formData, setFormData] = useState<AppSettings | null>(null)

  if (settings && !formData) {
    setFormData(settings)
  }

  const mutation = useMutation({
    mutationFn: updateSettings,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['settings'] })
      toast.show('success', t('settings.savedFull'))
    },
    onError: (error: Error) => {
      toast.show('error', error.message)
    },
  })

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (formData) {
      mutation.mutate(formData)
    }
  }

  const handleChange = (field: keyof AppSettings, value: number | boolean) => {
    if (formData) {
      setFormData({ ...formData, [field]: value })
    }
  }

  const handleTelemetryChange = (field: keyof TelemetrySettings, value: boolean) => {
    if (formData) {
      setFormData({
        ...formData,
        telemetry: { ...formData.telemetry, [field]: value },
      })
    }
  }

  const handleExport = async (dataType: ExportDataType) => {
    setExportLoading(dataType)
    try {
      const to = new Date().toISOString()
      const from = new Date(Date.now() - 7 * 24 * 60 * 60 * 1000).toISOString()

      const blob = await exportData(dataType, exportFormat, from, to)
      const ext = exportFormat === 'csv' ? 'csv' : 'json'
      const timestamp = new Date().toISOString().split('T')[0]
      downloadBlob(blob, `${dataType}_${timestamp}.${ext}`)

      toast.show('success', t('settings.exportDone'))
    } catch (error) {
      toast.show('error', `${t('settings.saveFailed')}: ${error instanceof Error ? error.message : String(error)}`)
    } finally {
      setExportLoading(null)
    }
  }

  if (settingsLoading || storageLoading) {
    return (
      <div className="flex h-64 items-center justify-center">
        <Spinner size="lg" className={colors.primary.text} />
        <span className={cn('ml-3', colors.text.secondary)}>{t('common.loading')}</span>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {/* Storage Stats */}
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('settings.storageStats')}</CardTitle>
        {storageStats && (
          <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
            <StorageCard
              label={t('settings.totalSize')}
              value={formatBytes(storageStats.total_size_bytes)}
              subValue={`${t('settings.dbSize')}: ${formatBytes(storageStats.db_size_bytes)} / ${t('settings.frameSize')}: ${formatBytes(storageStats.frames_size_bytes)}`}
            />
            <StorageCard
              label={t('settings.frameCount')}
              value={formatNumber(storageStats.frame_count)}
              subValue={t('settings.screenshots')}
            />
            <StorageCard
              label={t('settings.eventCount')}
              value={formatNumber(storageStats.event_count)}
              subValue={t('settings.activityLogs')}
            />
            <StorageCard
              label={t('settings.metricCount')}
              value={formatNumber(storageStats.metric_count)}
              subValue={t('settings.systemMeasure')}
            />
          </div>
        )}
        {storageStats?.oldest_data_date && storageStats?.newest_data_date && (
          <div className="mt-4 text-content-secondary text-sm">
            {t('settings.dataRange')}: {storageStats.oldest_data_date.split('T')[0]} ~{' '}
            {storageStats.newest_data_date.split('T')[0]}
          </div>
        )}
      </Card>

      {/* Data Export */}
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('settings.exportTitle')}</CardTitle>
        <p className="mb-4 text-content-secondary text-sm">{t('settings.exportDescription')}</p>

        <div className="mb-4 flex items-center gap-4">
          <span className="text-content-strong text-sm">{t('settings.exportFormatLabel')}:</span>
          <label className="flex cursor-pointer items-center">
            <input
              type="radio"
              name="exportFormat"
              value="json"
              checked={exportFormat === 'json'}
              onChange={() => setExportFormat('json')}
              className={form.radio}
            />
            <span className="ml-2 text-content-strong">JSON</span>
          </label>
          <label className="flex cursor-pointer items-center">
            <input
              type="radio"
              name="exportFormat"
              value="csv"
              checked={exportFormat === 'csv'}
              onChange={() => setExportFormat('csv')}
              className={form.radio}
            />
            <span className="ml-2 text-content-strong">CSV</span>
          </label>
        </div>

        <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
          <ExportButton
            label={t('settings.exportMetricsLabel')}
            description={t('settings.exportMetricsDesc')}
            onClick={() => handleExport('metrics')}
            loading={exportLoading === 'metrics'}
          />
          <ExportButton
            label={t('settings.exportEventsLabel')}
            description={t('settings.exportEventsDesc')}
            onClick={() => handleExport('events')}
            loading={exportLoading === 'events'}
          />
          <ExportButton
            label={t('settings.exportFramesLabel')}
            description={t('settings.exportFramesDesc')}
            onClick={() => handleExport('frames')}
            loading={exportLoading === 'frames'}
          />
        </div>
      </Card>

      {formData && (
        <form onSubmit={handleSubmit} className="space-y-6">
          {/* Data Retention */}
          <Card variant="default" padding="lg">
            <CardTitle className="mb-4">{t('settings.retentionTitle')}</CardTitle>
            <div className="grid grid-cols-1 gap-6 md:grid-cols-2">
              <div>
                <label htmlFor="settings-retention-days" className={form.label}>
                  {t('settings.retentionDays')}
                </label>
                <Input
                  id="settings-retention-days"
                  type="number"
                  min={1}
                  max={365}
                  value={formData.retention_days}
                  onChange={(e) => handleChange('retention_days', parseInt(e.target.value, 10) || 30)}
                />
                <p className={form.helper}>{t('settings.retentionAutoDelete')}</p>
              </div>
              <div>
                <label htmlFor="settings-max-storage-mb" className={form.label}>
                  {t('settings.maxStorageMb')}
                </label>
                <Input
                  id="settings-max-storage-mb"
                  type="number"
                  min={100}
                  max={10000}
                  step={100}
                  value={formData.max_storage_mb}
                  onChange={(e) => handleChange('max_storage_mb', parseInt(e.target.value, 10) || 500)}
                />
                <p className={form.helper}>{t('settings.maxStorageOverflow')}</p>
              </div>
            </div>
          </Card>

          {/* Telemetry */}
          <Card variant="default" padding="lg">
            <CardTitle className="mb-4">{t('settings.telemetryTitle')}</CardTitle>
            <p className="mb-4 text-content-secondary text-sm">{t('settings.telemetryDesc')}</p>
            <div className="space-y-4">
              <ToggleRow
                label={t('settings.telemetryEnabled')}
                description={t('settings.telemetryEnabledDesc')}
                checked={formData.telemetry.enabled}
                onChange={(v) => handleTelemetryChange('enabled', v)}
              />

              <div
                className={`space-y-4 border-DEFAULT border-l-2 pl-4 ${!formData.telemetry.enabled ? 'pointer-events-none opacity-50' : ''}`}
              >
                <ToggleRow
                  label={t('settings.crashReports')}
                  description={t('settings.crashReportsDesc')}
                  checked={formData.telemetry.crash_reports}
                  onChange={(v) => handleTelemetryChange('crash_reports', v)}
                />
                <ToggleRow
                  label={t('settings.usageStats')}
                  description={t('settings.usageStatsDesc')}
                  checked={formData.telemetry.usage_analytics}
                  onChange={(v) => handleTelemetryChange('usage_analytics', v)}
                />
                <ToggleRow
                  label={t('settings.perfMetrics')}
                  description={t('settings.perfMetricsDesc')}
                  checked={formData.telemetry.performance_metrics}
                  onChange={(v) => handleTelemetryChange('performance_metrics', v)}
                />
              </div>
            </div>
          </Card>

          {/* Save button */}
          <div className="flex justify-end">
            <Button type="submit" variant="primary" size="lg" isLoading={mutation.isPending}>
              {mutation.isPending ? t('settings.saving') : t('settings.saveSettings')}
            </Button>
          </div>
        </form>
      )}
    </div>
  )
}
