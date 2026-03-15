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
  fetchProviderSurfaces,
  fetchSecretBackendCapabilities,
  fetchSettings,
  fetchStorageStats,
  fetchUpdateStatus,
  type MonitorControlSettings,
  type NotificationSettings as NotificationSettingsType,
  type OcrValidationSettings as OcrValidationSettingsType,
  type PrivacySettings as PrivacySettingsType,
  type ProviderModelsResponse,
  type ProviderSurfaceSpec,
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
import { DEFAULT_PROVIDER_SURFACE_CATALOG } from '../api/defaultProviderSurfaceCatalog'
import { isStandaloneModeEnabled } from '../api/standalone'
import { Button, Spinner, Tabs } from '../components/ui'
import {
  defaultSurfaceEndpoint,
  defaultSurfaceModel,
  deriveDefaultProviderSurfaceId,
  getCompatibleProviderSurfaces,
  providerSurfaceById,
  resolveProviderTypeForSurface,
  type EndpointSurfaceKind,
} from '../features/providerSurfaces'
import { useToast } from '../hooks/useToast'
import { colors, typography } from '../styles/tokens'
import { cn } from '../utils/cn'
import { IS_TAURI } from '../utils/platform'
import { AiAutomationTab, DataStorageTab, GeneralTab, MonitoringTab, PrivacyTab } from './settingSections'

type SettingsTabId = 'general' | 'privacy' | 'monitoring' | 'ai-automation' | 'data'

function backendAllowsSecretEditing(backendKind: string): boolean {
  return backendKind !== 'env' && backendKind !== 'bridge_managed' && backendKind !== 'unavailable'
}

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
  const canQuerySecretBackendCapabilities = IS_TAURI && !isStandaloneModeEnabled()

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

  const { data: providerSurfaceCatalog } = useQuery({
    queryKey: ['ai-provider-surfaces'],
    queryFn: fetchProviderSurfaces,
    retry: 1,
  })

  const { data: secretBackendCapabilities } = useQuery({
    queryKey: ['secret-backend-capabilities'],
    queryFn: fetchSecretBackendCapabilities,
    enabled: canQuerySecretBackendCapabilities,
    retry: 1,
  })

  useEffect(() => {
    if (settings) {
      setFormData((current) => current ?? settings)
    }
  }, [settings])

  const defaultByokBackendKind = secretBackendCapabilities?.byok_backend_kind ?? 'legacy_config'

  useEffect(() => {
    if (!secretBackendCapabilities) {
      return
    }

    setFormData((current) => {
      if (!current) return current

      let changed = false
      const applyBackendDefault = (endpoint: ExternalApiSettings | null): ExternalApiSettings | null => {
        if (!endpoint) return endpoint
        if (endpoint.backend_kind !== 'legacy_config') return endpoint
        if (endpoint.has_secret || endpoint.api_key_masked.trim().length > 0) return endpoint
        changed = true
        return {
          ...endpoint,
          backend_kind: defaultByokBackendKind,
          can_edit_secret: backendAllowsSecretEditing(defaultByokBackendKind),
        }
      }

      const nextOcr = applyBackendDefault(current.ai_provider.ocr_api)
      const nextLlm = applyBackendDefault(current.ai_provider.llm_api)

      if (!changed) {
        return current
      }

      return {
        ...current,
        ai_provider: {
          ...current.ai_provider,
          ocr_api: nextOcr,
          llm_api: nextLlm,
        },
      }
    })
  }, [defaultByokBackendKind, secretBackendCapabilities])

  const providerCatalog = providerSurfaceCatalog ?? DEFAULT_PROVIDER_SURFACE_CATALOG

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

  const syncEndpointSurface = (
    endpointKind: EndpointSurfaceKind,
    accessMode: string,
    endpoint: ExternalApiSettings | null | undefined,
  ): ExternalApiSettings | null => {
    if (!endpoint) return null
    const providerType = resolveProviderTypeForSurface(providerCatalog, endpoint.surface_id, endpoint.provider_type)
    const surfaceId = deriveDefaultProviderSurfaceId(providerCatalog, accessMode, endpointKind, providerType)
    const previousSurface = providerSurfaceById(providerCatalog, endpoint.surface_id)
    const nextSurface = providerSurfaceById(providerCatalog, surfaceId)
    const previousDefaultEndpoint = defaultSurfaceEndpoint(previousSurface, endpointKind)
    const nextDefaultEndpoint = defaultSurfaceEndpoint(nextSurface, endpointKind)
    const previousDefaultModel = defaultSurfaceModel(previousSurface, endpointKind)
    const nextDefaultModel = defaultSurfaceModel(nextSurface, endpointKind)

    return {
      ...endpoint,
      endpoint:
        !endpoint.endpoint.trim() || endpoint.endpoint === previousDefaultEndpoint
          ? nextDefaultEndpoint
          : endpoint.endpoint,
      model:
        !endpoint.model?.trim() || endpoint.model === previousDefaultModel ? nextDefaultModel : endpoint.model,
      provider_type: providerType,
      surface_id: surfaceId,
    }
  }

  const handleAiProviderChange = (
    field: keyof AiProviderSettings,
    value: string | boolean | ExternalApiSettings | OcrValidationSettingsType | SceneIntelligenceSettingsType | null,
  ) => {
    setFormData((current) =>
      current
        ? (() => {
            const nextAiProvider = { ...current.ai_provider, [field]: value }
            if (field === 'access_mode' && typeof value === 'string') {
              nextAiProvider.ocr_api = syncEndpointSurface('ocr_api', value, nextAiProvider.ocr_api)
              nextAiProvider.llm_api = syncEndpointSurface('llm_api', value, nextAiProvider.llm_api)
            }
            return {
              ...current,
              ai_provider: nextAiProvider,
            }
          })()
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

  const defaultExternalApiSettings = (
    accessMode: string,
    endpointKind: EndpointSurfaceKind,
  ): ExternalApiSettings => {
    const surfaceId = deriveDefaultProviderSurfaceId(providerCatalog, accessMode, endpointKind, 'Generic')
    const surface = providerSurfaceById(providerCatalog, surfaceId)

    return {
      endpoint: defaultSurfaceEndpoint(surface, endpointKind),
      api_key_masked: '',
      model: defaultSurfaceModel(surface, endpointKind),
      provider_type: surface?.provider_type ?? 'Generic',
      surface_id: surfaceId,
      timeout_secs: 30,
      auth_mode: 'api_key',
      backend_kind: defaultByokBackendKind,
      has_secret: false,
      can_edit_secret: backendAllowsSecretEditing(defaultByokBackendKind),
      secret_display_hint: null,
      projection_enabled: false,
    }
  }

  const resolveEndpointSurface = (which: 'ocr_api' | 'llm_api'): ProviderSurfaceSpec | undefined => {
    const endpoint = formData?.ai_provider[which]
    const providerType = resolveProviderTypeForSurface(providerCatalog, endpoint?.surface_id, endpoint?.provider_type)
    const surfaceId =
      endpoint?.surface_id ??
      deriveDefaultProviderSurfaceId(providerCatalog, formData?.ai_provider.access_mode, which, providerType)
    return providerSurfaceById(providerCatalog, surfaceId)
  }

  const handleExternalApiChange = (
    which: 'ocr_api' | 'llm_api',
    field: keyof ExternalApiSettings,
    value: string | number | boolean | null,
  ) => {
    setFormData((current) => {
      if (!current) return current
      const existing = current.ai_provider[which] ?? defaultExternalApiSettings(current.ai_provider.access_mode, which)

      return {
        ...current,
        ai_provider: {
          ...current.ai_provider,
          [which]: { ...existing, [field]: value },
        },
      }
    })
  }

  const getCompatibleSurfaceOptions = (which: 'ocr_api' | 'llm_api'): ProviderSurfaceSpec[] =>
    getCompatibleProviderSurfaces(providerCatalog, formData?.ai_provider.access_mode, which)

  const getSurfaceModels = (which: 'ocr_api' | 'llm_api'): string[] => {
    const surface = resolveEndpointSurface(which)
    if (!surface) return []
    return which === 'ocr_api' ? (surface.default_models.ocr_models ?? []) : (surface.default_models.llm_models ?? [])
  }

  const getModelOptions = (which: 'ocr_api' | 'llm_api'): string[] => {
    const surfaceModels = getSurfaceModels(which)
    const discoveredModels = modelCatalog[which]
    return Array.from(new Set([...discoveredModels, ...surfaceModels]))
  }

  const canDiscoverModels = (which: 'ocr_api' | 'llm_api'): boolean => {
    const surface = resolveEndpointSurface(which)
    return surface?.supports.model_catalog ?? true
  }

  const handleProviderSurfaceChange = (which: 'ocr_api' | 'llm_api', nextSurfaceId: string) => {
    const nextSurface = providerSurfaceById(providerCatalog, nextSurfaceId)
    if (!nextSurface) {
      return
    }

    setFormData((current) => {
      if (!current) return current
      const existing = current.ai_provider[which] ?? defaultExternalApiSettings(current.ai_provider.access_mode, which)
      const previousSurface = providerSurfaceById(providerCatalog, existing.surface_id)
      const previousDefaultEndpoint = defaultSurfaceEndpoint(previousSurface, which)
      const nextDefaultEndpoint = defaultSurfaceEndpoint(nextSurface, which)
      const previousDefaultModel = defaultSurfaceModel(previousSurface, which)
      const nextDefaultModel = defaultSurfaceModel(nextSurface, which)

      const endpoint =
        !existing.endpoint.trim() || existing.endpoint === previousDefaultEndpoint
          ? nextDefaultEndpoint
          : existing.endpoint
      const model =
        !existing.model?.trim() || existing.model === previousDefaultModel ? nextDefaultModel : existing.model

      return {
        ...current,
        ai_provider: {
          ...current.ai_provider,
          [which]: {
            ...existing,
            provider_type: nextSurface.provider_type,
            surface_id: nextSurface.surface_id,
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
    if (!canDiscoverModels(which)) {
      showToast('error', t('settingsAutomation.modelDiscoveryUnsupportedSurface'), 5000)
      return
    }
    const useSavedSecret = current.has_secret && !current.api_key_masked?.trim()
    if (!current.api_key_masked?.trim() && !useSavedSecret) {
      showToast('error', t('settingsAutomation.modelDiscoveryMissingKey'), 5000)
      return
    }

    setModelCatalogLoading(which)
    try {
      const result = await discoverProviderModels({
        provider_type: current.provider_type ?? 'Generic',
        api_key: current.api_key_masked,
        endpoint: current.endpoint || null,
        surface: which,
        surface_id: current.surface_id || null,
        use_saved_secret: useSavedSecret,
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

  const handleSubmit = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!formData) {
      return
    }
    saveMutation.mutate(formData)
  }

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
        idBase="settings"
      />

      <form className="space-y-6" onSubmit={handleSubmit}>
        <div
          id="settings-panel-general"
          role="tabpanel"
          aria-labelledby="settings-tab-general"
          hidden={activeTab !== 'general'}
          aria-hidden={activeTab !== 'general'}
        >
          <fieldset disabled={activeTab !== 'general'} className="m-0 min-w-0 border-0 p-0">
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
          </fieldset>
        </div>

        <div
          id="settings-panel-privacy"
          role="tabpanel"
          aria-labelledby="settings-tab-privacy"
          hidden={activeTab !== 'privacy'}
          aria-hidden={activeTab !== 'privacy'}
        >
          <fieldset disabled={activeTab !== 'privacy'} className="m-0 min-w-0 border-0 p-0">
            <PrivacyTab formData={formData} onPrivacyChange={handlePrivacyChange} />
          </fieldset>
        </div>

        <div
          id="settings-panel-monitoring"
          role="tabpanel"
          aria-labelledby="settings-tab-monitoring"
          hidden={activeTab !== 'monitoring'}
          aria-hidden={activeTab !== 'monitoring'}
        >
          <fieldset disabled={activeTab !== 'monitoring'} className="m-0 min-w-0 border-0 p-0">
            <MonitoringTab
              formData={formData}
              onRootChange={(field, value) => handleRootChange(field as keyof AppSettings, value)}
              onMonitorChange={handleMonitorChange}
            />
          </fieldset>
        </div>

        <div
          id="settings-panel-ai-automation"
          role="tabpanel"
          aria-labelledby="settings-tab-ai-automation"
          hidden={activeTab !== 'ai-automation'}
          aria-hidden={activeTab !== 'ai-automation'}
        >
          <fieldset disabled={activeTab !== 'ai-automation'} className="m-0 min-w-0 border-0 p-0">
            <AiAutomationTab
              formData={formData}
              providerSurfaceOptions={{
                ocr_api: getCompatibleSurfaceOptions('ocr_api'),
                llm_api: getCompatibleSurfaceOptions('llm_api'),
              }}
              modelCatalogNotice={modelCatalogNotice}
              modelCatalogLoading={modelCatalogLoading}
              onAutomationChange={handleAutomationChange}
              onSandboxChange={handleSandboxChange}
              onAiProviderChange={handleAiProviderChange}
              onOcrValidationChange={handleOcrValidationChange}
              onSceneActionOverrideChange={handleSceneActionOverrideChange}
              onSceneIntelligenceChange={handleSceneIntelligenceChange}
              onExternalApiChange={handleExternalApiChange}
              resolveProviderSurface={resolveEndpointSurface}
              onProviderSurfaceChange={handleProviderSurfaceChange}
              onDiscoverModels={(which) => void discoverModels(which)}
              getModelOptions={getModelOptions}
              canDiscoverModels={canDiscoverModels}
            />
          </fieldset>
        </div>

        <div
          id="settings-panel-data"
          role="tabpanel"
          aria-labelledby="settings-tab-data"
          hidden={activeTab !== 'data'}
          aria-hidden={activeTab !== 'data'}
        >
          <fieldset disabled={activeTab !== 'data'} className="m-0 min-w-0 border-0 p-0">
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
          </fieldset>
        </div>

        <div className="flex justify-end">
          <Button
            data-testid="settings-save"
            type="submit"
            variant="primary"
            size="lg"
            isLoading={saveMutation.isPending}
            disabled={saveDisabled}
          >
            {saveMutation.isPending ? t('settings.saving') : t('settings.saveSettings')}
          </Button>
        </div>
      </form>
    </div>
  )
}
