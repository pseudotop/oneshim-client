/**
 * 설정 페이지
 *
 * 앱 설정 조회/수정, 저장소 현황, 데이터 내보내기
 */
import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import {
  fetchSettings,
  updateSettings,
  fetchStorageStats,
  exportData,
  downloadBlob,
  type AppSettings,
  type NotificationSettings as NotificationSettingsType,
  type TelemetrySettings,
  type MonitorControlSettings,
  type PrivacySettings as PrivacySettingsType,
  type ScheduleSettings as ScheduleSettingsType,
  type AutomationSettings,
  type SandboxSettings,
  type AiProviderSettings,
  type ExternalApiSettings,
  type ExportFormat,
  type ExportDataType
} from '../api/client'
import { Card, CardTitle, Input, Button, Spinner } from '../components/ui'
import { formatBytes, formatNumber } from '../utils/formatters'
import {
  NotificationSettings,
  PrivacySettings,
  ScheduleSettings,
  ToggleRow,
} from './settingSections'

export default function Settings() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [saveMessage, setSaveMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null)
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

  // 설정이 로드되면 폼 데이터 초기화
  if (settings && !formData) {
    setFormData(settings)
  }

  const mutation = useMutation({
    mutationFn: updateSettings,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['settings'] })
      setSaveMessage({ type: 'success', text: t('settings.savedFull') })
      setTimeout(() => setSaveMessage(null), 5000)
    },
    onError: (error: Error) => {
      setSaveMessage({ type: 'error', text: error.message })
      setTimeout(() => setSaveMessage(null), 5000)
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

  const handleExport = async (dataType: ExportDataType) => {
    setExportLoading(dataType)
    try {
      // 지난 7일 데이터 내보내기
      const to = new Date().toISOString()
      const from = new Date(Date.now() - 7 * 24 * 60 * 60 * 1000).toISOString()

      const blob = await exportData(dataType, exportFormat, from, to)
      const ext = exportFormat === 'csv' ? 'csv' : 'json'
      const timestamp = new Date().toISOString().split('T')[0]
      downloadBlob(blob, `${dataType}_${timestamp}.${ext}`)

      setSaveMessage({ type: 'success', text: t('settings.exportDone') })
      setTimeout(() => setSaveMessage(null), 3000)
    } catch (error) {
      setSaveMessage({ type: 'error', text: `${t('settings.saveFailed')}: ${error instanceof Error ? error.message : String(error)}` })
      setTimeout(() => setSaveMessage(null), 5000)
    } finally {
      setExportLoading(null)
    }
  }

  const handleNotificationChange = (field: keyof NotificationSettingsType, value: number | boolean) => {
    if (formData) {
      setFormData({
        ...formData,
        notification: { ...formData.notification, [field]: value }
      })
    }
  }

  const handleTelemetryChange = (field: keyof TelemetrySettings, value: boolean) => {
    if (formData) {
      setFormData({
        ...formData,
        telemetry: { ...formData.telemetry, [field]: value }
      })
    }
  }

  const handleMonitorChange = (field: keyof MonitorControlSettings, value: boolean) => {
    if (formData) {
      setFormData({
        ...formData,
        monitor: { ...formData.monitor, [field]: value }
      })
    }
  }

  const handlePrivacyChange = (field: keyof PrivacySettingsType, value: boolean | string | string[]) => {
    if (formData) {
      setFormData({
        ...formData,
        privacy: { ...formData.privacy, [field]: value }
      })
    }
  }

  const handleScheduleChange = (field: keyof ScheduleSettingsType, value: boolean | number | string[]) => {
    if (formData) {
      setFormData({
        ...formData,
        schedule: { ...formData.schedule, [field]: value }
      })
    }
  }

  const handleAutomationChange = (field: keyof AutomationSettings, value: boolean) => {
    if (formData) {
      setFormData({
        ...formData,
        automation: { ...formData.automation, [field]: value }
      })
    }
  }

  const handleSandboxChange = (field: keyof SandboxSettings, value: boolean | string | number | string[]) => {
    if (formData) {
      setFormData({
        ...formData,
        sandbox: { ...formData.sandbox, [field]: value }
      })
    }
  }

  const handleAiProviderChange = (field: keyof AiProviderSettings, value: string | boolean | ExternalApiSettings | null) => {
    if (formData) {
      setFormData({
        ...formData,
        ai_provider: { ...formData.ai_provider, [field]: value }
      })
    }
  }

  const handleExternalApiChange = (which: 'ocr_api' | 'llm_api', field: keyof ExternalApiSettings, value: string | number | null) => {
    if (formData) {
      const current = formData.ai_provider[which] ?? { endpoint: '', api_key_masked: '', model: null, timeout_secs: 30 }
      setFormData({
        ...formData,
        ai_provider: {
          ...formData.ai_provider,
          [which]: { ...current, [field]: value }
        }
      })
    }
  }

  if (settingsLoading || storageLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Spinner size="lg" className="text-teal-500" />
        <span className="ml-3 text-slate-600 dark:text-slate-400">{t('common.loading')}</span>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {/* 헤더 */}
      <h1 className="text-2xl font-bold text-slate-900 dark:text-white">{t('settings.title')}</h1>

      {/* 저장 메시지 */}
      {saveMessage && (
        <div
          className={`p-4 rounded-lg ${
            saveMessage.type === 'success'
              ? 'bg-green-500/20 border border-green-500 text-green-600 dark:text-green-400'
              : 'bg-red-500/20 border border-red-500 text-red-600 dark:text-red-400'
          }`}
        >
          {saveMessage.text}
        </div>
      )}

      {/* 저장소 현황 */}
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('settings.storageStats')}</CardTitle>
        {storageStats && (
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
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
          <div className="mt-4 text-sm text-slate-600 dark:text-slate-400">
            {t('settings.dataRange')}: {storageStats.oldest_data_date.split('T')[0]} ~ {storageStats.newest_data_date.split('T')[0]}
          </div>
        )}
      </Card>

      {/* 데이터 내보내기 */}
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('settings.exportTitle')}</CardTitle>
        <p className="text-sm text-slate-600 dark:text-slate-400 mb-4">{t('settings.exportDescription')}</p>

        {/* 형식 선택 */}
        <div className="flex items-center gap-4 mb-4">
          <span className="text-slate-700 dark:text-slate-300 text-sm">{t('settings.exportFormatLabel')}:</span>
          <label className="flex items-center cursor-pointer">
            <input
              type="radio"
              name="exportFormat"
              value="json"
              checked={exportFormat === 'json'}
              onChange={() => setExportFormat('json')}
              className="w-4 h-4 bg-slate-900 border-slate-700 text-teal-500 focus:ring-teal-500"
            />
            <span className="ml-2 text-slate-700 dark:text-slate-300">JSON</span>
          </label>
          <label className="flex items-center cursor-pointer">
            <input
              type="radio"
              name="exportFormat"
              value="csv"
              checked={exportFormat === 'csv'}
              onChange={() => setExportFormat('csv')}
              className="w-4 h-4 bg-slate-900 border-slate-700 text-teal-500 focus:ring-teal-500"
            />
            <span className="ml-2 text-slate-700 dark:text-slate-300">CSV</span>
          </label>
        </div>

        {/* 내보내기 버튼 */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
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

      {/* 설정 폼 */}
      {formData && (
        <form onSubmit={handleSubmit} className="space-y-6">
          {/* 데이터 보관 설정 */}
          <Card variant="default" padding="lg">
            <CardTitle className="mb-4">{t('settings.retentionTitle')}</CardTitle>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
              <div>
                <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
                  {t('settings.retentionDays')}
                </label>
                <Input
                  type="number"
                  min={1}
                  max={365}
                  value={formData.retention_days}
                  onChange={(e) => handleChange('retention_days', parseInt(e.target.value) || 30)}
                />
                <p className="mt-1 text-xs text-slate-600 dark:text-slate-500">{t('settings.retentionAutoDelete')}</p>
              </div>
              <div>
                <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
                  {t('settings.maxStorageMb')}
                </label>
                <Input
                  type="number"
                  min={100}
                  max={10000}
                  step={100}
                  value={formData.max_storage_mb}
                  onChange={(e) => handleChange('max_storage_mb', parseInt(e.target.value) || 500)}
                />
                <p className="mt-1 text-xs text-slate-600 dark:text-slate-500">{t('settings.maxStorageOverflow')}</p>
              </div>
            </div>
          </Card>

          {/* 수집 설정 */}
          <Card variant="default" padding="lg">
            <CardTitle className="mb-4">{t('settings.collectionTitle')}</CardTitle>
            <div className="space-y-4">
              <label className="flex items-center justify-between cursor-pointer">
                <div>
                  <span className="text-slate-700 dark:text-slate-300">{t('settings.captureEnabled')}</span>
                  <p className="text-xs text-slate-600 dark:text-slate-500">{t('settings.captureEnabledDesc')}</p>
                </div>
                <input
                  type="checkbox"
                  checked={formData.capture_enabled}
                  onChange={(e) => handleChange('capture_enabled', e.target.checked)}
                  className="w-5 h-5 rounded bg-slate-900 border-slate-700 text-teal-500 focus:ring-teal-500"
                />
              </label>

              <div className="grid grid-cols-1 md:grid-cols-3 gap-4 pt-4">
                <div>
                  <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
                    {t('settings.idleThresholdSecs')}
                  </label>
                  <Input
                    type="number"
                    min={60}
                    max={3600}
                    step={60}
                    value={formData.idle_threshold_secs}
                    onChange={(e) => handleChange('idle_threshold_secs', parseInt(e.target.value) || 300)}
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
                    {t('settings.metricsIntervalSecs')}
                  </label>
                  <Input
                    type="number"
                    min={1}
                    max={60}
                    value={formData.metrics_interval_secs}
                    onChange={(e) => handleChange('metrics_interval_secs', parseInt(e.target.value) || 5)}
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
                    {t('settings.processIntervalSecs')}
                  </label>
                  <Input
                    type="number"
                    min={5}
                    max={300}
                    value={formData.process_interval_secs}
                    onChange={(e) => handleChange('process_interval_secs', parseInt(e.target.value) || 10)}
                  />
                </div>
              </div>
            </div>
          </Card>

          {/* 웹 대시보드 설정 */}
          <Card variant="default" padding="lg">
            <CardTitle className="mb-4">{t('settings.webTitle')}</CardTitle>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
              <div>
                <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
                  {t('settings.portLabel')}
                </label>
                <Input
                  type="number"
                  min={1024}
                  max={65535}
                  value={formData.web_port}
                  onChange={(e) => handleChange('web_port', parseInt(e.target.value) || 9090)}
                />
                <p className="mt-1 text-xs text-slate-600 dark:text-slate-500">{t('settings.portRestart')}</p>
              </div>
              <div className="flex items-center">
                <label className="flex items-center cursor-pointer">
                  <input
                    type="checkbox"
                    checked={formData.allow_external}
                    onChange={(e) => handleChange('allow_external', e.target.checked)}
                    className="w-5 h-5 rounded bg-slate-900 border-slate-700 text-teal-500 focus:ring-teal-500 mr-3"
                  />
                  <div>
                    <span className="text-slate-700 dark:text-slate-300">{t('settings.allowExternal')}</span>
                    <p className="text-xs text-slate-600 dark:text-slate-500">{t('settings.allowExternalDesc')}</p>
                  </div>
                </label>
              </div>
            </div>
          </Card>

          {/* 알림 설정 */}
          <NotificationSettings
            notification={formData.notification}
            onChange={handleNotificationChange}
          />

          {/* 모니터링 제어 */}
          <Card variant="default" padding="lg">
            <CardTitle className="mb-4">{t('settings.monitorTitle')}</CardTitle>
            <div className="space-y-4">
              <ToggleRow
                label={t('settings.processMonitoring')}
                description={t('settings.processMonitoringDesc')}
                checked={formData.monitor.process_monitoring}
                onChange={(v) => handleMonitorChange('process_monitoring', v)}
              />
              <ToggleRow
                label={t('settings.inputActivity')}
                description={t('settings.inputActivityDesc')}
                checked={formData.monitor.input_activity}
                onChange={(v) => handleMonitorChange('input_activity', v)}
              />
              <ToggleRow
                label={t('settings.privacyMode')}
                description={t('settings.privacyModeDesc')}
                checked={formData.monitor.privacy_mode}
                onChange={(v) => handleMonitorChange('privacy_mode', v)}
              />
            </div>
          </Card>

          {/* 프라이버시 설정 */}
          <PrivacySettings
            privacy={formData.privacy}
            onChange={handlePrivacyChange}
          />

          {/* 스케줄 설정 */}
          <ScheduleSettings
            schedule={formData.schedule}
            onChange={handleScheduleChange}
          />

          {/* 텔레메트리 설정 */}
          <Card variant="default" padding="lg">
            <CardTitle className="mb-4">{t('settings.telemetryTitle')}</CardTitle>
            <p className="text-sm text-slate-600 dark:text-slate-400 mb-4">{t('settings.telemetryDesc')}</p>
            <div className="space-y-4">
              <ToggleRow
                label={t('settings.telemetryEnabled')}
                description={t('settings.telemetryEnabledDesc')}
                checked={formData.telemetry.enabled}
                onChange={(v) => handleTelemetryChange('enabled', v)}
              />

              <div className={`space-y-4 pl-4 border-l-2 border-slate-300 dark:border-slate-600 ${!formData.telemetry.enabled ? 'opacity-50 pointer-events-none' : ''}`}>
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

          {/* 자동화 설정 */}
          <Card variant="default" padding="lg">
            <CardTitle className="mb-4">{t('settingsAutomation.title')}</CardTitle>
            <div className="space-y-4">
              <ToggleRow
                label={t('settingsAutomation.enabled')}
                description={t('settingsAutomation.enabledDescription')}
                checked={formData.automation.enabled}
                onChange={(v) => handleAutomationChange('enabled', v)}
              />
            </div>
          </Card>

          {/* 샌드박스 설정 */}
          <Card variant="default" padding="lg">
            <CardTitle className="mb-4">{t('settingsAutomation.sandboxTitle')}</CardTitle>
            <div className="space-y-4">
              <ToggleRow
                label={t('settingsAutomation.sandboxEnabled')}
                description={t('settingsAutomation.sandboxEnabledDescription')}
                checked={formData.sandbox.enabled}
                onChange={(v) => handleSandboxChange('enabled', v)}
              />

              <div className={`space-y-4 ${!formData.sandbox.enabled ? 'opacity-50 pointer-events-none' : ''}`}>
                <div>
                  <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
                    {t('settingsAutomation.sandboxProfile')}
                  </label>
                  <select
                    value={formData.sandbox.profile}
                    onChange={(e) => handleSandboxChange('profile', e.target.value)}
                    className="w-full px-3 py-2 rounded-md border border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800 text-slate-900 dark:text-white focus:ring-teal-500 focus:border-teal-500"
                  >
                    <option value="Permissive">Permissive</option>
                    <option value="Standard">Standard</option>
                    <option value="Strict">Strict</option>
                  </select>
                </div>

                <ToggleRow
                  label={t('settingsAutomation.allowNetwork')}
                  description={t('settingsAutomation.allowNetworkDescription')}
                  checked={formData.sandbox.allow_network}
                  onChange={(v) => handleSandboxChange('allow_network', v)}
                />
              </div>
            </div>
          </Card>

          {/* AI 제공자 설정 */}
          <Card variant="default" padding="lg">
            <CardTitle className="mb-4">{t('settingsAutomation.aiTitle')}</CardTitle>
            <div className="space-y-4">
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div>
                  <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
                    {t('settingsAutomation.ocrProvider')}
                  </label>
                  <select
                    value={formData.ai_provider.ocr_provider}
                    onChange={(e) => handleAiProviderChange('ocr_provider', e.target.value)}
                    className="w-full px-3 py-2 rounded-md border border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800 text-slate-900 dark:text-white focus:ring-teal-500 focus:border-teal-500"
                  >
                    <option value="Local">Local</option>
                    <option value="Remote">Remote</option>
                  </select>
                </div>
                <div>
                  <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
                    {t('settingsAutomation.llmProvider')}
                  </label>
                  <select
                    value={formData.ai_provider.llm_provider}
                    onChange={(e) => handleAiProviderChange('llm_provider', e.target.value)}
                    className="w-full px-3 py-2 rounded-md border border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800 text-slate-900 dark:text-white focus:ring-teal-500 focus:border-teal-500"
                  >
                    <option value="Local">Local</option>
                    <option value="Remote">Remote</option>
                  </select>
                </div>
              </div>

              <div>
                <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
                  {t('settingsAutomation.dataPolicy')}
                </label>
                <select
                  value={formData.ai_provider.external_data_policy}
                  onChange={(e) => handleAiProviderChange('external_data_policy', e.target.value)}
                  className="w-full px-3 py-2 rounded-md border border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800 text-slate-900 dark:text-white focus:ring-teal-500 focus:border-teal-500"
                >
                  <option value="PiiFilterStrict">PII Filter Strict</option>
                  <option value="PiiFilterStandard">PII Filter Standard</option>
                  <option value="AllowFiltered">Allow Filtered</option>
                </select>
              </div>

              <ToggleRow
                label={t('settingsAutomation.fallbackToLocal')}
                description={t('settingsAutomation.fallbackToLocalDescription')}
                checked={formData.ai_provider.fallback_to_local}
                onChange={(v) => handleAiProviderChange('fallback_to_local', v)}
              />

              {/* OCR 외부 API 설정 (Remote 선택 시만 표시) */}
              {formData.ai_provider.ocr_provider === 'Remote' && (
                <div className="p-4 rounded-lg border border-slate-200 dark:border-slate-700 space-y-3">
                  <h4 className="text-sm font-medium text-slate-700 dark:text-slate-300">
                    OCR {t('settingsAutomation.externalApi')}
                  </h4>
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                    <div>
                      <label className="block text-xs text-slate-600 dark:text-slate-400 mb-1">{t('settingsAutomation.endpoint')}</label>
                      <Input
                        type="text"
                        value={formData.ai_provider.ocr_api?.endpoint ?? ''}
                        onChange={(e) => handleExternalApiChange('ocr_api', 'endpoint', e.target.value)}
                        placeholder="https://api.example.com/ocr"
                      />
                    </div>
                    <div>
                      <label className="block text-xs text-slate-600 dark:text-slate-400 mb-1">{t('settingsAutomation.apiKey')}</label>
                      <Input
                        type="password"
                        value={formData.ai_provider.ocr_api?.api_key_masked ?? ''}
                        onChange={(e) => handleExternalApiChange('ocr_api', 'api_key_masked', e.target.value)}
                        placeholder={t('settingsAutomation.apiKeyPlaceholder')}
                      />
                    </div>
                    <div>
                      <label className="block text-xs text-slate-600 dark:text-slate-400 mb-1">{t('settingsAutomation.model')}</label>
                      <Input
                        type="text"
                        value={formData.ai_provider.ocr_api?.model ?? ''}
                        onChange={(e) => handleExternalApiChange('ocr_api', 'model', e.target.value || null)}
                      />
                    </div>
                    <div>
                      <label className="block text-xs text-slate-600 dark:text-slate-400 mb-1">{t('settingsAutomation.timeoutSecs')}</label>
                      <Input
                        type="number"
                        min={5}
                        max={300}
                        value={formData.ai_provider.ocr_api?.timeout_secs ?? 30}
                        onChange={(e) => handleExternalApiChange('ocr_api', 'timeout_secs', parseInt(e.target.value) || 30)}
                      />
                    </div>
                  </div>
                </div>
              )}

              {/* LLM 외부 API 설정 (Remote 선택 시만 표시) */}
              {formData.ai_provider.llm_provider === 'Remote' && (
                <div className="p-4 rounded-lg border border-slate-200 dark:border-slate-700 space-y-3">
                  <h4 className="text-sm font-medium text-slate-700 dark:text-slate-300">
                    LLM {t('settingsAutomation.externalApi')}
                  </h4>
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                    <div>
                      <label className="block text-xs text-slate-600 dark:text-slate-400 mb-1">{t('settingsAutomation.endpoint')}</label>
                      <Input
                        type="text"
                        value={formData.ai_provider.llm_api?.endpoint ?? ''}
                        onChange={(e) => handleExternalApiChange('llm_api', 'endpoint', e.target.value)}
                        placeholder="https://api.example.com/llm"
                      />
                    </div>
                    <div>
                      <label className="block text-xs text-slate-600 dark:text-slate-400 mb-1">{t('settingsAutomation.apiKey')}</label>
                      <Input
                        type="password"
                        value={formData.ai_provider.llm_api?.api_key_masked ?? ''}
                        onChange={(e) => handleExternalApiChange('llm_api', 'api_key_masked', e.target.value)}
                        placeholder={t('settingsAutomation.apiKeyPlaceholder')}
                      />
                    </div>
                    <div>
                      <label className="block text-xs text-slate-600 dark:text-slate-400 mb-1">{t('settingsAutomation.model')}</label>
                      <Input
                        type="text"
                        value={formData.ai_provider.llm_api?.model ?? ''}
                        onChange={(e) => handleExternalApiChange('llm_api', 'model', e.target.value || null)}
                      />
                    </div>
                    <div>
                      <label className="block text-xs text-slate-600 dark:text-slate-400 mb-1">{t('settingsAutomation.timeoutSecs')}</label>
                      <Input
                        type="number"
                        min={5}
                        max={300}
                        value={formData.ai_provider.llm_api?.timeout_secs ?? 30}
                        onChange={(e) => handleExternalApiChange('llm_api', 'timeout_secs', parseInt(e.target.value) || 30)}
                      />
                    </div>
                  </div>
                </div>
              )}
            </div>
          </Card>

          {/* 저장 버튼 */}
          <div className="flex justify-end">
            <Button
              type="submit"
              variant="primary"
              size="lg"
              isLoading={mutation.isPending}
            >
              {mutation.isPending ? t('settings.saving') : t('settings.saveSettings')}
            </Button>
          </div>
        </form>
      )}
    </div>
  )
}

interface StorageCardProps {
  label: string
  value: string
  subValue: string
}

function StorageCard({ label, value, subValue }: StorageCardProps) {
  return (
    <Card variant="elevated" padding="md">
      <div className="text-sm text-slate-600 dark:text-slate-400">{label}</div>
      <div className="text-2xl font-bold text-slate-900 dark:text-white mt-1">{value}</div>
      <div className="text-xs text-slate-600 dark:text-slate-500 mt-1">{subValue}</div>
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
      onClick={onClick}
      disabled={loading}
      className="flex flex-col items-start p-4 bg-slate-200 dark:bg-slate-900 rounded-lg border border-slate-300 dark:border-slate-700 hover:border-teal-500 hover:bg-slate-300 dark:hover:bg-slate-800 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
    >
      <div className="flex items-center gap-2">
        <svg
          className="w-5 h-5 text-teal-600 dark:text-teal-400"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"
          />
        </svg>
        <span className="text-slate-900 dark:text-white font-medium">{label}</span>
        {loading && <Spinner size="sm" className="text-teal-400" />}
      </div>
      <span className="text-xs text-slate-600 dark:text-slate-500 mt-1">{description}</span>
    </button>
  )
}
