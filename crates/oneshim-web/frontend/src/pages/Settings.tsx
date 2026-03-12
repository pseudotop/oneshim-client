import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  type AiProviderSettings,
  type AppSettings,
  type AutomationSettings,
  discoverProviderModels,
  downloadBlob,
  type ExportDataType,
  type ExportFormat,
  type ExternalApiSettings,
  exportData,
  fetchProviderPresets,
  fetchSettings,
  fetchStorageStats,
  fetchUpdateStatus,
  type MonitorControlSettings,
  type NotificationSettings as NotificationSettingsType,
  type OcrValidationSettings as OcrValidationSettingsType,
  type PrivacySettings as PrivacySettingsType,
  type ProviderModelsResponse,
  type ProviderPreset,
  postUpdateAction,
  type SandboxSettings,
  type SceneActionOverrideSettings as SceneActionOverrideSettingsType,
  type SceneIntelligenceSettings as SceneIntelligenceSettingsType,
  type ScheduleSettings as ScheduleSettingsType,
  type TelemetrySettings,
  type UpdateAction,
  type UpdateStatus,
  updateSettings,
} from '../api/client'
import { Button, Spinner, Tabs } from '../components/ui'
import { useToast } from '../hooks/useToast'
import { colors, typography } from '../styles/tokens'
import { cn } from '../utils/cn'
import { AiAutomationTab, DataStorageTab, GeneralTab, MonitoringTab, PrivacyTab } from './settingSections'

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
    llm_endpoint: 'https://generativelanguage.googleapis.com/v1beta/models/gemini-flash-latest:generateContent',
    ocr_endpoint: 'https://vision.googleapis.com/v1/images:annotate',
    model_catalog_endpoint: 'https://generativelanguage.googleapis.com/v1beta/models',
    ocr_model_catalog_supported: false,
    ocr_model_catalog_notice: 'Google Vision OCR endpoint does not expose a selectable model catalog.',
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

type SettingsTabId = 'general' | 'privacy' | 'monitoring' | 'ai-automation' | 'data'

export default function Settings() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const { show: showToast } = useToast()
  const [activeTab, setActiveTab] = useState<SettingsTabId>('general')
  const [formData, setFormData] = useState<AppSettings | null>(null)
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

  useEffect(() => {
    if (settings) {
      setFormData((current) => current ?? settings)
    }
  }, [settings])

  const providerPresets =
    providerPresetCatalog?.providers && providerPresetCatalog.providers.length > 0
      ? providerPresetCatalog.providers
      : DEFAULT_PROVIDER_PRESETS

  const saveMutation = useMutation({
    mutationFn: updateSettings,
    onSuccess: (savedSettings) => {
      queryClient.setQueryData(['settings'], savedSettings)
      queryClient.invalidateQueries({ queryKey: ['settings'] })
      setFormData(savedSettings)
      showToast('success', t('settings.savedFull'), 5000)
    },
    onError: (error: Error) => {
      showToast('error', error.message, 5000)
    },
  })

  const updateActionMutation = useMutation({
    mutationFn: (action: UpdateAction) => postUpdateAction(action),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['update-status'] })
      showToast('success', t('settings.updateActionSuccess'), 3000)
    },
    onError: (error: Error) => {
      showToast('error', error.message, 5000)
    },
  })

  const handleRootChange = (field: keyof AppSettings, value: number | boolean) => {
    setFormData((current) => (current ? { ...current, [field]: value } : current))
  }

  const handleNotificationChange = (field: keyof NotificationSettingsType, value: number | boolean) => {
    setFormData((current) =>
      current
        ? {
            ...current,
            notification: { ...current.notification, [field]: value },
          }
        : current,
    )
  }

  const handleTelemetryChange = (field: keyof TelemetrySettings, value: boolean) => {
    setFormData((current) =>
      current
        ? {
            ...current,
            telemetry: { ...current.telemetry, [field]: value },
          }
        : current,
    )
  }

  const handleMonitorChange = (field: keyof MonitorControlSettings, value: boolean) => {
    setFormData((current) =>
      current
        ? {
            ...current,
            monitor: { ...current.monitor, [field]: value },
          }
        : current,
    )
  }

  const handlePrivacyChange = (field: keyof PrivacySettingsType, value: boolean | string | string[]) => {
    setFormData((current) =>
      current
        ? {
            ...current,
            privacy: { ...current.privacy, [field]: value },
          }
        : current,
    )
  }

  const handleScheduleChange = (field: keyof ScheduleSettingsType, value: boolean | number | string[]) => {
    setFormData((current) =>
      current
        ? {
            ...current,
            schedule: { ...current.schedule, [field]: value },
          }
        : current,
    )
  }

  const handleUpdateChange = (field: keyof AppSettings['update'], value: boolean | number) => {
    setFormData((current) =>
      current
        ? {
            ...current,
            update: { ...current.update, [field]: value },
          }
        : current,
    )
  }

  const handleAutomationChange = (field: keyof AutomationSettings, value: boolean) => {
    setFormData((current) =>
      current
        ? {
            ...current,
            automation: { ...current.automation, [field]: value },
          }
        : current,
    )
  }

  const handleSandboxChange = (field: keyof SandboxSettings, value: boolean | string | number | string[]) => {
    setFormData((current) =>
      current
        ? {
            ...current,
            sandbox: { ...current.sandbox, [field]: value },
          }
        : current,
    )
  }

  const handleAiProviderChange = (
    field: keyof AiProviderSettings,
    value: string | boolean | ExternalApiSettings | OcrValidationSettingsType | SceneIntelligenceSettingsType | null,
  ) => {
    setFormData((current) =>
      current
        ? {
            ...current,
            ai_provider: { ...current.ai_provider, [field]: value },
          }
        : current,
    )
  }

  const handleOcrValidationChange = (field: keyof OcrValidationSettingsType, value: boolean | number) => {
    setFormData((current) =>
      current
        ? {
            ...current,
            ai_provider: {
              ...current.ai_provider,
              ocr_validation: {
                ...current.ai_provider.ocr_validation,
                [field]: value,
              },
            },
          }
        : current,
    )
  }

  const handleSceneActionOverrideChange = (
    field: keyof SceneActionOverrideSettingsType,
    value: boolean | string | null,
  ) => {
    setFormData((current) =>
      current
        ? {
            ...current,
            ai_provider: {
              ...current.ai_provider,
              scene_action_override: {
                ...current.ai_provider.scene_action_override,
                [field]: value,
              },
            },
          }
        : current,
    )
  }

  const handleSceneIntelligenceChange = (field: keyof SceneIntelligenceSettingsType, value: boolean | number) => {
    setFormData((current) =>
      current
        ? {
            ...current,
            ai_provider: {
              ...current.ai_provider,
              scene_intelligence: {
                ...current.ai_provider.scene_intelligence,
                [field]: value,
              },
            },
          }
        : current,
    )
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
    value: string | number | null,
  ) => {
    setFormData((current) => {
      if (!current) return current
      const existing = current.ai_provider[which] ?? defaultExternalApiSettings()

      return {
        ...current,
        ai_provider: {
          ...current.ai_provider,
          [which]: { ...existing, [field]: value },
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
        preset.aliases.some((alias) => alias.toLowerCase() === normalized),
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
    const presetEndpoint = which === 'ocr_api' ? (preset?.ocr_endpoint ?? '') : (preset?.llm_endpoint ?? '')
    const presetModel = which === 'ocr_api' ? (preset?.ocr_models?.[0] ?? null) : (preset?.llm_models?.[0] ?? null)

    setFormData((current) => {
      if (!current) return current
      const existing = current.ai_provider[which] ?? defaultExternalApiSettings()
      const endpoint = existing.endpoint && existing.endpoint.trim().length > 0 ? existing.endpoint : presetEndpoint
      const model = existing.model && existing.model.trim().length > 0 ? existing.model : presetModel

      return {
        ...current,
        ai_provider: {
          ...current.ai_provider,
          [which]: {
            ...existing,
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
    result: ProviderModelsResponse,
  ) => {
    setModelCatalog((current) => ({
      ...current,
      [which]: result.models,
    }))
    setModelCatalogNotice((current) => ({
      ...current,
      [which]: result.notice ?? (result.models.length === 0 ? t('settingsAutomation.modelDiscoveryNoModels') : null),
    }))

    if ((!currentModel || !currentModel.trim()) && result.models.length > 0) {
      handleExternalApiChange(which, 'model', result.models[0])
    }
  }

  const discoverModels = async (which: 'ocr_api' | 'llm_api') => {
    if (!formData) return
    const current = formData.ai_provider[which]
    if (!current) {
      showToast('error', t('settingsAutomation.modelDiscoveryMissingConfig'), 5000)
      return
    }
    if (!current.api_key_masked?.trim()) {
      showToast('error', t('settingsAutomation.modelDiscoveryMissingKey'), 5000)
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
      setModelCatalog((currentCatalog) => ({
        ...currentCatalog,
        [which]: [],
      }))
      setModelCatalogNotice((currentNotice) => ({
        ...currentNotice,
        [which]: message,
      }))
      showToast('error', message, 5000)
    } finally {
      setModelCatalogLoading(null)
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
      showToast('success', t('settings.exportDone'), 3000)
    } catch (error) {
      showToast('error', `${t('settings.saveFailed')}: ${error instanceof Error ? error.message : String(error)}`, 5000)
    } finally {
      setExportLoading(null)
    }
  }

  const tabs = [
    { id: 'general', label: t('settings.tabs.general') },
    { id: 'privacy', label: t('settings.tabs.privacy') },
    { id: 'monitoring', label: t('settings.tabs.monitoring') },
    { id: 'ai-automation', label: t('settings.tabs.aiAutomation') },
    { id: 'data', label: t('settings.tabs.dataStorage') },
  ]

  const saveDisabled =
    !settings || !formData || saveMutation.isPending || JSON.stringify(formData) === JSON.stringify(settings)

  if (settingsLoading || !formData) {
    return (
      <div className="flex h-64 items-center justify-center">
        <Spinner size="lg" className={colors.primary.text} />
        <span className={cn('ml-3', colors.text.secondary)}>{t('common.loading')}</span>
      </div>
    )
  }

  return (
    <div className="min-h-full space-y-6 p-6">
      <div className="flex items-center justify-between">
        <h1 className={cn(typography.h1, colors.text.primary)}>{t('settings.title')}</h1>
      </div>

      <Tabs
        tabs={tabs}
        activeTab={activeTab}
        onTabChange={(tab) => setActiveTab(tab as SettingsTabId)}
        ariaLabel={t('settings.title')}
      />

      <div className="space-y-6">
        <div hidden={activeTab !== 'general'} aria-hidden={activeTab !== 'general'}>
          <GeneralTab
            formData={formData}
            updateStatus={updateStatus}
            updateActionPending={updateActionMutation.isPending}
            onRootChange={(field, value) => handleRootChange(field as keyof AppSettings, value)}
            onNotificationChange={handleNotificationChange}
            onScheduleChange={handleScheduleChange}
            onUpdateChange={handleUpdateChange}
            onUpdateAction={(action) => updateActionMutation.mutate(action)}
          />
        </div>

        <div hidden={activeTab !== 'privacy'} aria-hidden={activeTab !== 'privacy'}>
          <PrivacyTab formData={formData} onPrivacyChange={handlePrivacyChange} />
        </div>

        <div hidden={activeTab !== 'monitoring'} aria-hidden={activeTab !== 'monitoring'}>
          <MonitoringTab
            formData={formData}
            onRootChange={(field, value) => handleRootChange(field as keyof AppSettings, value)}
            onMonitorChange={handleMonitorChange}
          />
        </div>

        <div hidden={activeTab !== 'ai-automation'} aria-hidden={activeTab !== 'ai-automation'}>
          <AiAutomationTab
            formData={formData}
            providerPresets={providerPresets}
            modelCatalogNotice={modelCatalogNotice}
            modelCatalogLoading={modelCatalogLoading}
            onAutomationChange={handleAutomationChange}
            onSandboxChange={handleSandboxChange}
            onAiProviderChange={handleAiProviderChange}
            onOcrValidationChange={handleOcrValidationChange}
            onSceneActionOverrideChange={handleSceneActionOverrideChange}
            onSceneIntelligenceChange={handleSceneIntelligenceChange}
            onExternalApiChange={handleExternalApiChange}
            onProviderTypeChange={handleProviderTypeChange}
            onDiscoverModels={(which) => void discoverModels(which)}
            getModelOptions={getModelOptions}
          />
        </div>

        <div hidden={activeTab !== 'data'} aria-hidden={activeTab !== 'data'}>
          <DataStorageTab
            formData={formData}
            storageStats={storageStats}
            storageLoading={storageLoading}
            exportFormat={exportFormat}
            exportLoading={exportLoading}
            onExportFormatChange={setExportFormat}
            onExport={(dataType) => void handleExport(dataType)}
            onRootChange={(field, value) => handleRootChange(field as keyof AppSettings, value)}
            onTelemetryChange={handleTelemetryChange}
          />
        </div>
      </div>

      <div className="flex justify-end">
        <Button
          data-testid="settings-save"
          type="button"
          variant="primary"
          size="lg"
          isLoading={saveMutation.isPending}
          disabled={saveDisabled}
          onClick={() => saveMutation.mutate(formData)}
        >
          {saveMutation.isPending ? t('settings.saving') : t('settings.saveSettings')}
        </Button>
      </div>
    </div>
  )
}
