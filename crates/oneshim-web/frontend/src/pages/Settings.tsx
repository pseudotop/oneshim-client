/**
 *
 */
import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import {
  fetchSettings,
  updateSettings,
  fetchStorageStats,
  fetchUpdateStatus,
  postUpdateAction,
  fetchProviderPresets,
  discoverProviderModels,
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
  type OcrValidationSettings as OcrValidationSettingsType,
  type SceneActionOverrideSettings as SceneActionOverrideSettingsType,
  type SceneIntelligenceSettings as SceneIntelligenceSettingsType,
  type ExternalApiSettings,
  type ProviderPreset,
  type ProviderModelsResponse,
  type ExportFormat,
  type ExportDataType,
  type UpdateStatus,
  type UpdateAction
} from '../api/client'
import { Card, CardTitle, Input, Button, Spinner, Select } from '../components/ui'
import { colors, form, typography } from '../styles/tokens'
import { cn } from '../utils/cn'
import { formatBytes, formatNumber } from '../utils/formatters'
import {
  NotificationSettings,
  PrivacySettings,
  ScheduleSettings,
  ToggleRow,
} from './settingSections'

const DEFAULT_PROVIDER_PRESETS: ProviderPreset[] = [
  {
    provider_type: 'Anthropic',
    aliases: ['anthropic'],
    display_name: 'Anthropic',
    llm_endpoint: 'https://api.anthropic.com/v1/messages',
    ocr_endpoint: 'https://api.anthropic.com/v1/messages',
    model_catalog_endpoint: 'https://api.anthropic.com/v1/models',
    ocr_model_catalog_supported: true,
    llm_models: ['claude-sonnet-4-5', 'claude-opus-4-1'],
    ocr_models: ['claude-sonnet-4-5', 'claude-opus-4-1'],
  },
  {
    provider_type: 'OpenAi',
    aliases: ['openai', 'open_ai', 'open-ai', 'openai-compatible'],
    display_name: 'OpenAI',
    llm_endpoint: 'https://api.openai.com/v1/chat/completions',
    ocr_endpoint: 'https://api.openai.com/v1/chat/completions',
    model_catalog_endpoint: 'https://api.openai.com/v1/models',
    ocr_model_catalog_supported: true,
    llm_models: ['gpt-4.1', 'gpt-4.1-mini', 'o3-mini'],
    ocr_models: ['gpt-4.1', 'gpt-4.1-mini'],
  },
  {
    provider_type: 'Google',
    aliases: ['google', 'gemini'],
    display_name: 'Google',
    llm_endpoint:
      'https://generativelanguage.googleapis.com/v1beta/models/gemini-flash-latest:generateContent',
    ocr_endpoint: 'https://vision.googleapis.com/v1/images:annotate',
    model_catalog_endpoint: 'https://generativelanguage.googleapis.com/v1beta/models',
    ocr_model_catalog_supported: false,
    ocr_model_catalog_notice:
      'Google Vision OCR endpoint does not expose a selectable model catalog.',
    llm_models: ['gemini-flash-latest', 'gemini-2.5-flash', 'gemini-2.5-pro'],
    ocr_models: [],
  },
  {
    provider_type: 'Generic',
    aliases: ['generic'],
    display_name: 'Generic',
    llm_endpoint: 'https://api.openai.com/v1/chat/completions',
    ocr_endpoint: 'https://api.openai.com/v1/chat/completions',
    model_catalog_endpoint: 'https://api.openai.com/v1/models',
    ocr_model_catalog_supported: true,
    llm_models: ['gpt-4.1-mini', 'o3-mini'],
    ocr_models: ['gpt-4.1-mini'],
  },
]

export default function Settings() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [saveMessage, setSaveMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null)
  const [exportFormat, setExportFormat] = useState<ExportFormat>('json')
  const [exportLoading, setExportLoading] = useState<ExportDataType | null>(null)
  const [modelCatalog, setModelCatalog] = useState<Record<'ocr_api' | 'llm_api', string[]>>({
    ocr_api: [],
    llm_api: [],
  })
  const [modelCatalogNotice, setModelCatalogNotice] = useState<Record<'ocr_api' | 'llm_api', string | null>>({
    ocr_api: null,
    llm_api: null,
  })
  const [modelCatalogLoading, setModelCatalogLoading] = useState<'ocr_api' | 'llm_api' | null>(null)

  const toDateTimeLocalValue = (value: string | null | undefined): string => {
    if (!value) return ''
    const d = new Date(value)
    if (Number.isNaN(d.getTime())) return ''
    const pad = (n: number) => String(n).padStart(2, '0')
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
  }

  const toRfc3339OrNull = (value: string): string | null => {
    if (!value.trim()) return null
    const d = new Date(value)
    if (Number.isNaN(d.getTime())) return null
    return d.toISOString()
  }

  const { data: settings, isLoading: settingsLoading } = useQuery({
    queryKey: ['settings'],
    queryFn: fetchSettings,
  })

  const { data: storageStats, isLoading: storageLoading } = useQuery({
    queryKey: ['storage-stats'],
    queryFn: fetchStorageStats,
  })

  const { data: updateStatus } = useQuery<UpdateStatus>({
    queryKey: ['update-status'],
    queryFn: fetchUpdateStatus,
    refetchInterval: 15000,
    retry: 1,
  })

  const { data: providerPresetCatalog } = useQuery({
    queryKey: ['ai-provider-presets'],
    queryFn: fetchProviderPresets,
    retry: 1,
  })

  const providerPresets =
    providerPresetCatalog?.providers && providerPresetCatalog.providers.length > 0
      ? providerPresetCatalog.providers
      : DEFAULT_PROVIDER_PRESETS

  const [formData, setFormData] = useState<AppSettings | null>(null)

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

  const updateActionMutation = useMutation({
    mutationFn: (action: UpdateAction) => postUpdateAction(action),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['update-status'] })
      setSaveMessage({ type: 'success', text: t('settings.updateActionSuccess') })
      setTimeout(() => setSaveMessage(null), 3000)
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

  const handleUpdateChange = (field: keyof AppSettings['update'], value: boolean | number) => {
    if (formData) {
      setFormData({
        ...formData,
        update: { ...formData.update, [field]: value }
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

  const handleAiProviderChange = (
    field: keyof AiProviderSettings,
    value:
      | string
      | boolean
      | ExternalApiSettings
      | OcrValidationSettingsType
      | SceneIntelligenceSettingsType
      | null
  ) => {
    if (formData) {
      setFormData({
        ...formData,
        ai_provider: { ...formData.ai_provider, [field]: value }
      })
    }
  }

  const handleOcrValidationChange = (
    field: keyof OcrValidationSettingsType,
    value: boolean | number
  ) => {
    if (formData) {
      setFormData({
        ...formData,
        ai_provider: {
          ...formData.ai_provider,
          ocr_validation: {
            ...formData.ai_provider.ocr_validation,
            [field]: value,
          },
        },
      })
    }
  }

  const handleSceneActionOverrideChange = (
    field: keyof SceneActionOverrideSettingsType,
    value: boolean | string | null
  ) => {
    if (formData) {
      setFormData({
        ...formData,
        ai_provider: {
          ...formData.ai_provider,
          scene_action_override: {
            ...formData.ai_provider.scene_action_override,
            [field]: value,
          },
        },
      })
    }
  }

  const handleSceneIntelligenceChange = (
    field: keyof SceneIntelligenceSettingsType,
    value: boolean | number
  ) => {
    if (formData) {
      setFormData({
        ...formData,
        ai_provider: {
          ...formData.ai_provider,
          scene_intelligence: {
            ...formData.ai_provider.scene_intelligence,
            [field]: value,
          },
        },
      })
    }
  }

  const defaultExternalApiSettings = (): ExternalApiSettings => ({
    endpoint: '',
    api_key_masked: '',
    model: null,
    provider_type: 'Generic',
    timeout_secs: 30,
  })

  const handleExternalApiChange = (
    which: 'ocr_api' | 'llm_api',
    field: keyof ExternalApiSettings,
    value: string | number | null
  ) => {
    setFormData((prev) => {
      if (!prev) return prev
      const current = prev.ai_provider[which] ?? defaultExternalApiSettings()
      return {
        ...prev,
        ai_provider: {
          ...prev.ai_provider,
          [which]: { ...current, [field]: value },
        },
      }
    })
  }

  const findProviderPreset = (raw: string | null | undefined): ProviderPreset | undefined => {
    const normalized = (raw ?? '').trim().toLowerCase()
    if (!normalized) {
      return providerPresets.find((preset) => preset.provider_type === 'Generic')
    }
    return providerPresets.find(
      (preset) =>
        preset.provider_type.toLowerCase() === normalized ||
        preset.aliases.some((alias) => alias.toLowerCase() === normalized)
    )
  }

  const resolveProviderType = (raw: string | null | undefined): string => {
    return findProviderPreset(raw)?.provider_type ?? 'Generic'
  }

  const getPresetModels = (which: 'ocr_api' | 'llm_api', rawProviderType: string | null | undefined): string[] => {
    const preset = findProviderPreset(rawProviderType)
    return which === 'ocr_api' ? (preset?.ocr_models ?? []) : (preset?.llm_models ?? [])
  }

  const getModelOptions = (which: 'ocr_api' | 'llm_api'): string[] => {
    const providerType = resolveProviderType(formData?.ai_provider[which]?.provider_type)
    const presetModels = getPresetModels(which, providerType)
    const discoveredModels = modelCatalog[which]
    return Array.from(new Set([...discoveredModels, ...presetModels]))
  }

  const handleProviderTypeChange = (which: 'ocr_api' | 'llm_api', rawProviderType: string) => {
    const providerType = resolveProviderType(rawProviderType)
    const preset = findProviderPreset(providerType)
    const presetEndpoint =
      which === 'ocr_api' ? preset?.ocr_endpoint ?? '' : preset?.llm_endpoint ?? ''
    const presetModel =
      which === 'ocr_api' ? preset?.ocr_models?.[0] ?? null : preset?.llm_models?.[0] ?? null

    setFormData((prev) => {
      if (!prev) return prev
      const current = prev.ai_provider[which] ?? defaultExternalApiSettings()
      const endpoint =
        current.endpoint && current.endpoint.trim().length > 0
          ? current.endpoint
          : presetEndpoint
      const model =
        current.model && current.model.trim().length > 0 ? current.model : presetModel
      return {
        ...prev,
        ai_provider: {
          ...prev.ai_provider,
          [which]: {
            ...current,
            provider_type: providerType,
            endpoint,
            model,
          },
        },
      }
    })
  }

  const handleModelDiscoveryResult = (
    which: 'ocr_api' | 'llm_api',
    currentModel: string | null | undefined,
    result: ProviderModelsResponse
  ) => {
    setModelCatalog((prev) => ({
      ...prev,
      [which]: result.models,
    }))
    setModelCatalogNotice((prev) => ({
      ...prev,
      [which]:
        result.notice ??
        (result.models.length === 0
          ? t('settingsAutomation.modelDiscoveryNoModels')
          : null),
    }))

    if ((!currentModel || !currentModel.trim()) && result.models.length > 0) {
      handleExternalApiChange(which, 'model', result.models[0])
    }
  }

  const discoverModels = async (which: 'ocr_api' | 'llm_api') => {
    if (!formData) return
    const current = formData.ai_provider[which]
    if (!current) {
      setSaveMessage({ type: 'error', text: t('settingsAutomation.modelDiscoveryMissingConfig') })
      return
    }
    if (!current.api_key_masked?.trim()) {
      setSaveMessage({ type: 'error', text: t('settingsAutomation.modelDiscoveryMissingKey') })
      return
    }

    setModelCatalogLoading(which)
    try {
      const result = await discoverProviderModels({
        provider_type: current.provider_type ?? 'Generic',
        api_key: current.api_key_masked,
        endpoint: current.endpoint || null,
      })
      handleModelDiscoveryResult(which, current.model, result)
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      setModelCatalog((prev) => ({
        ...prev,
        [which]: [],
      }))
      setModelCatalogNotice((prev) => ({
        ...prev,
        [which]: message,
      }))
      setSaveMessage({ type: 'error', text: message })
      setTimeout(() => setSaveMessage(null), 5000)
    } finally {
      setModelCatalogLoading(null)
    }
  }

  const updateSectionDirty = Boolean(
    formData && settings && JSON.stringify(formData.update) !== JSON.stringify(settings.update)
  )

  const saveUpdateSection = () => {
    if (!formData) {
      return
    }

    const normalizedInterval = Math.max(1, Math.min(168, formData.update.check_interval_hours))
    mutation.mutate({
      ...formData,
      update: {
        ...formData.update,
        check_interval_hours: normalizedInterval,
      },
    })
  }

  if (settingsLoading || storageLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Spinner size="lg" className={colors.primary.text} />
        <span className={cn('ml-3', colors.text.secondary)}>{t('common.loading')}</span>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {/* UI note */}
      <h1 className={cn(typography.h1, colors.text.primary)}>{t('settings.title')}</h1>

      {/* UI note */}
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

      {/* UI note */}
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

      {/* UI note */}
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('settings.exportTitle')}</CardTitle>
        <p className="text-sm text-slate-600 dark:text-slate-400 mb-4">{t('settings.exportDescription')}</p>

        {/* UI note */}
        <div className="flex items-center gap-4 mb-4">
          <span className="text-slate-700 dark:text-slate-300 text-sm">{t('settings.exportFormatLabel')}:</span>
          <label className="flex items-center cursor-pointer">
            <input
              type="radio"
              name="exportFormat"
              value="json"
              checked={exportFormat === 'json'}
              onChange={() => setExportFormat('json')}
              className={form.radio}
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
              className={form.radio}
            />
            <span className="ml-2 text-slate-700 dark:text-slate-300">CSV</span>
          </label>
        </div>

        {/* UI note */}
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

      {/* UI note */}
      {formData && (
        <form onSubmit={handleSubmit} className="space-y-6">
          {/* UI note */}
          <Card variant="default" padding="lg">
            <CardTitle className="mb-4">{t('settings.retentionTitle')}</CardTitle>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
              <div>
                <label className={form.label}>
                  {t('settings.retentionDays')}
                </label>
                <Input
                  type="number"
                  min={1}
                  max={365}
                  value={formData.retention_days}
                  onChange={(e) => handleChange('retention_days', parseInt(e.target.value) || 30)}
                />
                <p className={form.helper}>{t('settings.retentionAutoDelete')}</p>
              </div>
              <div>
                <label className={form.label}>
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
                <p className={form.helper}>{t('settings.maxStorageOverflow')}</p>
              </div>
            </div>
          </Card>

          {/* UI note */}
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
                  className={form.checkbox}
                />
              </label>

              <div className="grid grid-cols-1 md:grid-cols-3 gap-4 pt-4">
                <div>
                  <label className={form.label}>
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
                  <label className={form.label}>
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
                  <label className={form.label}>
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

          {/* UI note */}
          <Card variant="default" padding="lg">
            <CardTitle className="mb-4">{t('settings.webTitle')}</CardTitle>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
              <div>
                <label className={form.label}>
                  {t('settings.portLabel')}
                </label>
                <Input
                  type="number"
                  min={1024}
                  max={65535}
                  value={formData.web_port}
                  onChange={(e) => handleChange('web_port', parseInt(e.target.value) || 9090)}
                />
                <p className={form.helper}>{t('settings.portRestart')}</p>
              </div>
              <div className="flex items-center">
                <label className="flex items-center cursor-pointer">
                  <input
                    type="checkbox"
                    checked={formData.allow_external}
                    onChange={(e) => handleChange('allow_external', e.target.checked)}
                    className={form.checkboxInline}
                  />
                  <div>
                    <span className="text-slate-700 dark:text-slate-300">{t('settings.allowExternal')}</span>
                    <p className="text-xs text-slate-600 dark:text-slate-500">{t('settings.allowExternalDesc')}</p>
                  </div>
                </label>
              </div>
            </div>
          </Card>

          <Card variant="default" padding="lg">
            <CardTitle className="mb-4">{t('settings.updateTitle')}</CardTitle>
            <div className="space-y-4">
              <ToggleRow
                label={t('settings.updateEnabled')}
                description={t('settings.updateEnabledDesc')}
                checked={formData.update.enabled}
                onChange={(v) => handleUpdateChange('enabled', v)}
              />

              <ToggleRow
                label={t('settings.updateAutoInstall')}
                description={t('settings.updateAutoInstallDesc')}
                checked={formData.update.auto_install}
                onChange={(v) => handleUpdateChange('auto_install', v)}
              />

              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div>
                  <label className={form.label}>
                    {t('settings.updateIntervalHours')}
                  </label>
                  <Input
                    type="number"
                    min={1}
                    max={168}
                    value={formData.update.check_interval_hours}
                    onChange={(e) => handleUpdateChange('check_interval_hours', parseInt(e.target.value) || 24)}
                  />
                </div>
                <div className="flex items-end">
                  <label className="flex items-center cursor-pointer">
                    <input
                      type="checkbox"
                      checked={formData.update.include_prerelease}
                      onChange={(e) => handleUpdateChange('include_prerelease', e.target.checked)}
                      className={form.checkboxInline}
                    />
                    <div>
                      <span className="text-slate-700 dark:text-slate-300">{t('settings.updateIncludePrerelease')}</span>
                      <p className="text-xs text-slate-600 dark:text-slate-500">{t('settings.updateIncludePrereleaseDesc')}</p>
                    </div>
                  </label>
                </div>
              </div>

              <div className="mt-2 p-4 rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-900/40">
                <div className="text-sm font-medium text-slate-900 dark:text-white">{t('settings.updateRuntimeStatus')}</div>
                <div className="mt-1 text-sm text-slate-700 dark:text-slate-300">
                  {updateStatus?.message ?? t('settings.updateStatusUnavailable')}
                </div>
                {updateStatus?.pending && (
                  <div className="mt-2 text-xs text-slate-600 dark:text-slate-400 space-y-1">
                    <div>{t('settings.updateCurrentVersion')}: {updateStatus.pending.current_version}</div>
                    <div>{t('settings.updateLatestVersion')}: {updateStatus.pending.latest_version}</div>
                    <a
                      href={updateStatus.pending.release_url}
                      target="_blank"
                      rel="noreferrer"
                      className="text-teal-600 dark:text-teal-400 underline"
                    >
                      {t('settings.updateReleaseNote')}
                    </a>
                  </div>
                )}
                <div className="mt-4 flex flex-wrap gap-2">
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    isLoading={updateActionMutation.isPending}
                    onClick={() => updateActionMutation.mutate('CheckNow')}
                  >
                    {t('settings.updateCheckNow')}
                  </Button>
                  <Button
                    type="button"
                    variant="primary"
                    size="sm"
                    isLoading={updateActionMutation.isPending}
                    onClick={() => updateActionMutation.mutate('Approve')}
                  >
                    {t('settings.updateApproveNow')}
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    isLoading={updateActionMutation.isPending}
                    onClick={() => updateActionMutation.mutate('Defer')}
                  >
                    {t('settings.updateDefer')}
                  </Button>
                </div>

                <div className="mt-4 flex justify-end">
                  <Button
                    type="button"
                    variant="primary"
                    size="sm"
                    isLoading={mutation.isPending}
                    disabled={!updateSectionDirty || mutation.isPending}
                    onClick={saveUpdateSection}
                  >
                    {t('settings.saveSettings')}
                  </Button>
                </div>
              </div>
            </div>
          </Card>

          {/* UI note */}
          <NotificationSettings
            notification={formData.notification}
            onChange={handleNotificationChange}
          />

          {/* UI note */}
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

          {/* UI note */}
          <PrivacySettings
            privacy={formData.privacy}
            onChange={handlePrivacyChange}
          />

          {/* UI note */}
          <ScheduleSettings
            schedule={formData.schedule}
            onChange={handleScheduleChange}
          />

          {/* UI note */}
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

          {/* UI note */}
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

          {/* UI note */}
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
                  <label className={form.label}>
                    {t('settingsAutomation.sandboxProfile')}
                  </label>
                  <Select
                    value={formData.sandbox.profile}
                    onChange={(e) => handleSandboxChange('profile', e.target.value)}
                  >
                    <option value="Permissive">Permissive</option>
                    <option value="Standard">Standard</option>
                    <option value="Strict">Strict</option>
                  </Select>
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

          {/* UI note */}
          <Card variant="default" padding="lg">
            <CardTitle className="mb-4">{t('settingsAutomation.aiTitle')}</CardTitle>
            <div className="space-y-4">
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div>
                  <label className={form.label}>
                    {t('settingsAutomation.ocrProvider')}
                  </label>
                  <Select
                    value={formData.ai_provider.ocr_provider}
                    onChange={(e) => handleAiProviderChange('ocr_provider', e.target.value)}
                  >
                    <option value="Local">Local</option>
                    <option value="Remote">Remote</option>
                  </Select>
                </div>
                <div>
                  <label className={form.label}>
                    {t('settingsAutomation.llmProvider')}
                  </label>
                  <Select
                    value={formData.ai_provider.llm_provider}
                    onChange={(e) => handleAiProviderChange('llm_provider', e.target.value)}
                  >
                    <option value="Local">Local</option>
                    <option value="Remote">Remote</option>
                  </Select>
                </div>
              </div>

              <div>
                <label className={form.label}>
                  {t('settingsAutomation.dataPolicy')}
                </label>
                <Select
                  value={formData.ai_provider.external_data_policy}
                  onChange={(e) => handleAiProviderChange('external_data_policy', e.target.value)}
                >
                  <option value="PiiFilterStrict">PII Filter Strict</option>
                  <option value="PiiFilterStandard">PII Filter Standard</option>
                  <option value="AllowFiltered">Allow Filtered</option>
                </Select>
              </div>

              <ToggleRow
                label={t('settingsAutomation.allowUnredactedExternalOcr')}
                description={t('settingsAutomation.allowUnredactedExternalOcrDescription')}
                checked={formData.ai_provider.allow_unredacted_external_ocr}
                onChange={(v) => handleAiProviderChange('allow_unredacted_external_ocr', v)}
              />

              <div className="p-4 rounded-lg border border-slate-200 dark:border-slate-700 space-y-3">
                <h4 className="text-sm font-medium text-slate-700 dark:text-slate-300">
                  {t('settingsAutomation.sceneActionOverrideTitle')}
                </h4>
                <ToggleRow
                  label={t('settingsAutomation.sceneActionOverrideEnabled')}
                  description={t('settingsAutomation.sceneActionOverrideEnabledDescription')}
                  checked={formData.ai_provider.scene_action_override.enabled}
                  onChange={(v) => handleSceneActionOverrideChange('enabled', v)}
                />
                <div className={`grid grid-cols-1 md:grid-cols-2 gap-3 ${!formData.ai_provider.scene_action_override.enabled ? 'opacity-50 pointer-events-none' : ''}`}>
                  <div className="md:col-span-2">
                    <label className="block text-xs text-slate-600 dark:text-slate-400 mb-1">
                      {t('settingsAutomation.sceneActionOverrideReason')}
                    </label>
                    <Input
                      type="text"
                      value={formData.ai_provider.scene_action_override.reason}
                      onChange={(e) => handleSceneActionOverrideChange('reason', e.target.value)}
                      placeholder={t('settingsAutomation.sceneActionOverrideReasonPlaceholder')}
                    />
                  </div>
                  <div>
                    <label className="block text-xs text-slate-600 dark:text-slate-400 mb-1">
                      {t('settingsAutomation.sceneActionOverrideApprovedBy')}
                    </label>
                    <Input
                      type="text"
                      value={formData.ai_provider.scene_action_override.approved_by}
                      onChange={(e) => handleSceneActionOverrideChange('approved_by', e.target.value)}
                      placeholder={t('settingsAutomation.sceneActionOverrideApprovedByPlaceholder')}
                    />
                  </div>
                  <div>
                    <label className="block text-xs text-slate-600 dark:text-slate-400 mb-1">
                      {t('settingsAutomation.sceneActionOverrideExpiresAt')}
                    </label>
                    <Input
                      type="datetime-local"
                      value={toDateTimeLocalValue(formData.ai_provider.scene_action_override.expires_at)}
                      onChange={(e) =>
                        handleSceneActionOverrideChange(
                          'expires_at',
                          toRfc3339OrNull(e.target.value)
                        )
                      }
                    />
                  </div>
                </div>
              </div>

              <div className="p-4 rounded-lg border border-slate-200 dark:border-slate-700 space-y-3">
                <h4 className="text-sm font-medium text-slate-700 dark:text-slate-300">
                  {t('settingsAutomation.sceneIntelligenceTitle', 'Scene Intelligence')}
                </h4>
                <ToggleRow
                  label={t('settingsAutomation.sceneIntelligenceEnabled', 'Enable Scene Intelligence')}
                  description={t(
                    'settingsAutomation.sceneIntelligenceEnabledDescription',
                    'Enable OCR-based UI structure detection and assistant recommendations.'
                  )}
                  checked={formData.ai_provider.scene_intelligence.enabled}
                  onChange={(v) => handleSceneIntelligenceChange('enabled', v)}
                />
                <div className={`space-y-3 ${!formData.ai_provider.scene_intelligence.enabled ? 'opacity-50 pointer-events-none' : ''}`}>
                  <ToggleRow
                    label={t('settingsAutomation.sceneOverlayEnabled', 'Show Overlay')}
                    description={t(
                      'settingsAutomation.sceneOverlayEnabledDescription',
                      'Render detected UI element boxes on session replay screenshots.'
                    )}
                    checked={formData.ai_provider.scene_intelligence.overlay_enabled}
                    onChange={(v) => handleSceneIntelligenceChange('overlay_enabled', v)}
                  />
                  <ToggleRow
                    label={t(
                      'settingsAutomation.sceneAllowExecution',
                      'Allow Scene Action Execution'
                    )}
                    description={t(
                      'settingsAutomation.sceneAllowExecutionDescription',
                      'Permit direct click/type execution from scene coordinates (RPA gate).'
                    )}
                    checked={formData.ai_provider.scene_intelligence.allow_action_execution}
                    onChange={(v) => handleSceneIntelligenceChange('allow_action_execution', v)}
                  />
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                    <div>
                      <label className="block text-xs text-slate-600 dark:text-slate-400 mb-1">
                        {t('settingsAutomation.sceneMinConfidence', 'Scene Min Confidence')}
                      </label>
                      <Input
                        type="number"
                        min={0}
                        max={1}
                        step={0.05}
                        value={formData.ai_provider.scene_intelligence.min_confidence}
                        onChange={(e) =>
                          handleSceneIntelligenceChange(
                            'min_confidence',
                            Number(e.target.value)
                          )
                        }
                      />
                    </div>
                    <div>
                      <label className="block text-xs text-slate-600 dark:text-slate-400 mb-1">
                        {t('settingsAutomation.sceneMaxElements', 'Scene Max Elements')}
                      </label>
                      <Input
                        type="number"
                        min={1}
                        max={1000}
                        step={1}
                        value={formData.ai_provider.scene_intelligence.max_elements}
                        onChange={(e) =>
                          handleSceneIntelligenceChange(
                            'max_elements',
                            Number(e.target.value)
                          )
                        }
                      />
                    </div>
                  </div>
                  <div className="p-3 rounded-md bg-slate-100/70 dark:bg-slate-800/70 space-y-3">
                    <ToggleRow
                      label={t(
                        'settingsAutomation.sceneCalibrationEnabled',
                        'Enable Calibration Validation'
                      )}
                      description={t(
                        'settingsAutomation.sceneCalibrationEnabledDescription',
                        'Validate whether current scene quality is sufficient before assistant usage.'
                      )}
                      checked={formData.ai_provider.scene_intelligence.calibration_enabled}
                      onChange={(v) => handleSceneIntelligenceChange('calibration_enabled', v)}
                    />
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                      <div>
                        <label className="block text-xs text-slate-600 dark:text-slate-400 mb-1">
                          {t(
                            'settingsAutomation.sceneCalibrationMinElements',
                            'Calibration Min Elements'
                          )}
                        </label>
                        <Input
                          type="number"
                          min={1}
                          max={1000}
                          step={1}
                          value={formData.ai_provider.scene_intelligence.calibration_min_elements}
                          onChange={(e) =>
                            handleSceneIntelligenceChange(
                              'calibration_min_elements',
                              Number(e.target.value)
                            )
                          }
                        />
                      </div>
                      <div>
                        <label className="block text-xs text-slate-600 dark:text-slate-400 mb-1">
                          {t(
                            'settingsAutomation.sceneCalibrationMinAvgConfidence',
                            'Calibration Min Avg Confidence'
                          )}
                        </label>
                        <Input
                          type="number"
                          min={0}
                          max={1}
                          step={0.05}
                          value={
                            formData.ai_provider.scene_intelligence.calibration_min_avg_confidence
                          }
                          onChange={(e) =>
                            handleSceneIntelligenceChange(
                              'calibration_min_avg_confidence',
                              Number(e.target.value)
                            )
                          }
                        />
                      </div>
                    </div>
                  </div>
                </div>
              </div>

              <div className="p-4 rounded-lg border border-slate-200 dark:border-slate-700 space-y-3">
                <h4 className="text-sm font-medium text-slate-700 dark:text-slate-300">
                  {t('settingsAutomation.ocrValidationTitle')}
                </h4>
                <ToggleRow
                  label={t('settingsAutomation.ocrValidationEnabled')}
                  description={t('settingsAutomation.ocrValidationEnabledDescription')}
                  checked={formData.ai_provider.ocr_validation.enabled}
                  onChange={(v) => handleOcrValidationChange('enabled', v)}
                />
                <div className={`grid grid-cols-1 md:grid-cols-2 gap-3 ${!formData.ai_provider.ocr_validation.enabled ? 'opacity-50 pointer-events-none' : ''}`}>
                  <div>
                    <label className="block text-xs text-slate-600 dark:text-slate-400 mb-1">
                      {t('settingsAutomation.ocrMinConfidence')}
                    </label>
                    <Input
                      type="number"
                      min={0}
                      max={1}
                      step={0.05}
                      value={formData.ai_provider.ocr_validation.min_confidence}
                      onChange={(e) => handleOcrValidationChange('min_confidence', Number(e.target.value))}
                    />
                  </div>
                  <div>
                    <label className="block text-xs text-slate-600 dark:text-slate-400 mb-1">
                      {t('settingsAutomation.ocrMaxInvalidRatio')}
                    </label>
                    <Input
                      type="number"
                      min={0}
                      max={1}
                      step={0.05}
                      value={formData.ai_provider.ocr_validation.max_invalid_ratio}
                      onChange={(e) => handleOcrValidationChange('max_invalid_ratio', Number(e.target.value))}
                    />
                  </div>
                </div>
              </div>

              <ToggleRow
                label={t('settingsAutomation.fallbackToLocal')}
                description={t('settingsAutomation.fallbackToLocalDescription')}
                checked={formData.ai_provider.fallback_to_local}
                onChange={(v) => handleAiProviderChange('fallback_to_local', v)}
              />

              {/* UI note */}
              {formData.ai_provider.ocr_provider === 'Remote' && (
                <div className="p-4 rounded-lg border border-slate-200 dark:border-slate-700 space-y-3">
                  <h4 className="text-sm font-medium text-slate-700 dark:text-slate-300">
                    OCR {t('settingsAutomation.externalApi')}
                  </h4>
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                    <div>
                      <label className="block text-xs text-slate-600 dark:text-slate-400 mb-1">
                        {t('settingsAutomation.providerType')}
                      </label>
                      <Select
                        value={resolveProviderType(formData.ai_provider.ocr_api?.provider_type)}
                        onChange={(e) => handleProviderTypeChange('ocr_api', e.target.value)}
                      >
                        {providerPresets.map((preset) => (
                          <option key={preset.provider_type} value={preset.provider_type}>
                            {preset.display_name}
                          </option>
                        ))}
                      </Select>
                    </div>
                    <div className="flex items-end">
                      <Button
                        type="button"
                        variant="secondary"
                        size="sm"
                        isLoading={modelCatalogLoading === 'ocr_api'}
                        onClick={() => void discoverModels('ocr_api')}
                      >
                        {t('settingsAutomation.loadModels')}
                      </Button>
                    </div>
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
                        list="ocr-model-catalog"
                        value={formData.ai_provider.ocr_api?.model ?? ''}
                        onChange={(e) => handleExternalApiChange('ocr_api', 'model', e.target.value || null)}
                      />
                      {getModelOptions('ocr_api').length > 0 && (
                        <datalist id="ocr-model-catalog">
                          {getModelOptions('ocr_api').map((modelName) => (
                            <option key={modelName} value={modelName} />
                          ))}
                        </datalist>
                      )}
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
                  {modelCatalogNotice.ocr_api && (
                    <p className="text-xs text-slate-500 dark:text-slate-400">
                      {modelCatalogNotice.ocr_api}
                    </p>
                  )}
                </div>
              )}

              {/* UI note */}
              {formData.ai_provider.llm_provider === 'Remote' && (
                <div className="p-4 rounded-lg border border-slate-200 dark:border-slate-700 space-y-3">
                  <h4 className="text-sm font-medium text-slate-700 dark:text-slate-300">
                    LLM {t('settingsAutomation.externalApi')}
                  </h4>
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                    <div>
                      <label className="block text-xs text-slate-600 dark:text-slate-400 mb-1">
                        {t('settingsAutomation.providerType')}
                      </label>
                      <Select
                        value={resolveProviderType(formData.ai_provider.llm_api?.provider_type)}
                        onChange={(e) => handleProviderTypeChange('llm_api', e.target.value)}
                      >
                        {providerPresets.map((preset) => (
                          <option key={preset.provider_type} value={preset.provider_type}>
                            {preset.display_name}
                          </option>
                        ))}
                      </Select>
                    </div>
                    <div className="flex items-end">
                      <Button
                        type="button"
                        variant="secondary"
                        size="sm"
                        isLoading={modelCatalogLoading === 'llm_api'}
                        onClick={() => void discoverModels('llm_api')}
                      >
                        {t('settingsAutomation.loadModels')}
                      </Button>
                    </div>
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
                        list="llm-model-catalog"
                        value={formData.ai_provider.llm_api?.model ?? ''}
                        onChange={(e) => handleExternalApiChange('llm_api', 'model', e.target.value || null)}
                      />
                      {getModelOptions('llm_api').length > 0 && (
                        <datalist id="llm-model-catalog">
                          {getModelOptions('llm_api').map((modelName) => (
                            <option key={modelName} value={modelName} />
                          ))}
                        </datalist>
                      )}
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
                  {modelCatalogNotice.llm_api && (
                    <p className="text-xs text-slate-500 dark:text-slate-400">
                      {modelCatalogNotice.llm_api}
                    </p>
                  )}
                </div>
              )}
            </div>
          </Card>

          {/* UI note */}
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
      <div className={cn('text-sm', colors.text.secondary)}>{label}</div>
      <div className={cn('text-2xl font-bold mt-1', colors.text.primary)}>{value}</div>
      <div className={cn('text-xs mt-1', colors.text.tertiary)}>{subValue}</div>
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
          className={cn('w-5 h-5', colors.primary.text)}
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
        <span className={cn('font-medium', colors.text.primary)}>{label}</span>
        {loading && <Spinner size="sm" className={colors.primary.text} />}
      </div>
      <span className={cn('text-xs mt-1', colors.text.tertiary)}>{description}</span>
    </button>
  )
}
