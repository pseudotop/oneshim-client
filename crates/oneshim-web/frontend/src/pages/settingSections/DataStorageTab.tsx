import { useTranslation } from 'react-i18next'
import type { ExportDataType, ExportFormat, StorageStats, TelemetrySettings } from '../../api/client'
import { Card, CardTitle, Input, Spinner } from '../../components/ui'
import { colors, form } from '../../styles/tokens'
import { cn } from '../../utils/cn'
import { formatBytes, formatNumber } from '../../utils/formatters'
import ToggleRow from './ToggleRow'
import type { SettingsFormTabProps } from './types'

interface StorageCardProps {
  label: string
  value: string
  subValue: string
}

function StorageCard({ label, value, subValue }: StorageCardProps) {
  return (
    <Card variant="elevated" padding="md">
      <div className={cn('text-sm', colors.text.secondary)}>{label}</div>
      <div className={cn('mt-1 font-bold text-2xl', colors.text.primary)}>{value}</div>
      <div className={cn('mt-1 text-xs', colors.text.tertiary)}>{subValue}</div>
    </Card>
  )
}

interface ExportButtonProps {
  label: string
  description: string
  onClick: () => void
  loading: boolean
}

function ExportButton({ label, description, onClick, loading }: ExportButtonProps) {
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

interface DataStorageTabProps extends SettingsFormTabProps {
  storageStats?: StorageStats
  storageLoading: boolean
  exportFormat: ExportFormat
  exportLoading: ExportDataType | null
  onExportFormatChange: (format: ExportFormat) => void
  onExport: (dataType: ExportDataType) => void
  onRootChange: (field: 'retention_days' | 'max_storage_mb', value: number) => void
  onTelemetryChange: (field: keyof TelemetrySettings, value: boolean) => void
}

export default function DataStorageTab({
  formData,
  storageStats,
  storageLoading,
  exportFormat,
  exportLoading,
  onExportFormatChange,
  onExport,
  onRootChange,
  onTelemetryChange,
}: DataStorageTabProps) {
  const { t } = useTranslation()

  return (
    <div className="space-y-6">
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('settings.storageStats')}</CardTitle>
        {storageLoading ? (
          <div className="flex h-32 items-center justify-center">
            <Spinner size="lg" className={colors.primary.text} />
          </div>
        ) : (
          <>
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
          </>
        )}
      </Card>

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
              onChange={() => onExportFormatChange('json')}
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
              onChange={() => onExportFormatChange('csv')}
              className={form.radio}
            />
            <span className="ml-2 text-content-strong">CSV</span>
          </label>
        </div>

        <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
          <ExportButton
            label={t('settings.exportMetricsLabel')}
            description={t('settings.exportMetricsDesc')}
            onClick={() => onExport('metrics')}
            loading={exportLoading === 'metrics'}
          />
          <ExportButton
            label={t('settings.exportEventsLabel')}
            description={t('settings.exportEventsDesc')}
            onClick={() => onExport('events')}
            loading={exportLoading === 'events'}
          />
          <ExportButton
            label={t('settings.exportFramesLabel')}
            description={t('settings.exportFramesDesc')}
            onClick={() => onExport('frames')}
            loading={exportLoading === 'frames'}
          />
        </div>
      </Card>

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
              onChange={(e) => onRootChange('retention_days', parseInt(e.target.value, 10) || 30)}
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
              onChange={(e) => onRootChange('max_storage_mb', parseInt(e.target.value, 10) || 500)}
            />
            <p className={form.helper}>{t('settings.maxStorageOverflow')}</p>
          </div>
        </div>
      </Card>

      <Card id="section-telemetry" variant="default" padding="lg">
        <CardTitle className="mb-4">{t('settings.telemetryTitle')}</CardTitle>
        <p className="mb-4 text-content-secondary text-sm">{t('settings.telemetryDesc')}</p>
        <div className="space-y-4">
          <ToggleRow
            label={t('settings.telemetryEnabled')}
            description={t('settings.telemetryEnabledDesc')}
            checked={formData.telemetry.enabled}
            onChange={(value) => onTelemetryChange('enabled', value)}
          />

          <div
            className={`space-y-4 border-DEFAULT border-l-2 pl-4 ${!formData.telemetry.enabled ? 'pointer-events-none opacity-50' : ''}`}
          >
            <ToggleRow
              label={t('settings.crashReports')}
              description={t('settings.crashReportsDesc')}
              checked={formData.telemetry.crash_reports}
              onChange={(value) => onTelemetryChange('crash_reports', value)}
            />
            <ToggleRow
              label={t('settings.usageStats')}
              description={t('settings.usageStatsDesc')}
              checked={formData.telemetry.usage_analytics}
              onChange={(value) => onTelemetryChange('usage_analytics', value)}
            />
            <ToggleRow
              label={t('settings.perfMetrics')}
              description={t('settings.perfMetricsDesc')}
              checked={formData.telemetry.performance_metrics}
              onChange={(value) => onTelemetryChange('performance_metrics', value)}
            />
          </div>
        </div>
      </Card>
    </div>
  )
}
