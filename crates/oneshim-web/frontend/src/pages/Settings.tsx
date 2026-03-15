import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useDeferredValue, useEffect, useState } from 'react'
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
  fetchFeatureCapabilities,
  exportData,
  type FeatureCapabilitySnapshot,
  fetchProviderSurfaces,
  probeProviderSurfaceEndpoint,
  type ProviderEndpointProbeResult,
  fetchSecretBackendCapabilities,
  fetchSettings,
  fetchStorageStats,
  fetchUpdateStatus,
  type MonitorControlSettings,
  type NotificationSettings as NotificationSettingsType,
  type OcrValidationSettings as OcrValidationSettingsType,
  type PrivacySettings as PrivacySettingsType,
  type ProviderDiscoveredModel,
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
  sortProviderSurfaces,
  surfaceKnownModel,
  surfaceSupportsModelSelection,
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

function supportsProjectionFor(authMode: string, backendKind: string): boolean {
  return (
    authMode === 'api_key' && (backendKind === 'os_secret_store' || backendKind === 'file_secret_store')
  )
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
  const [modelCatalogDetails, setModelCatalogDetails] = useState<
    Record<'ocr_api' | 'llm_api', ProviderDiscoveredModel[]>
  >({
    ocr_api: [],
    llm_api: [],
  })
  const [modelCatalogNotice, setModelCatalogNotice] = useState<Record<'ocr_api' | 'llm_api', string | null>>({
    ocr_api: null,
    llm_api: null,
  })
  const [modelCatalogLoading, setModelCatalogLoading] = useState<'ocr_api' | 'llm_api' | null>(null)
  const canQueryDesktopCapabilities = IS_TAURI && !isStandaloneModeEnabled()

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

  const { data: featureCapabilities } = useQuery<FeatureCapabilitySnapshot>({
    queryKey: ['feature-capabilities'],
    queryFn: fetchFeatureCapabilities,
    enabled: canQueryDesktopCapabilities,
    retry: 1,
  })

  const { data: secretBackendCapabilities } = useQuery({
    queryKey: ['secret-backend-capabilities'],
    queryFn: fetchSecretBackendCapabilities,
    enabled: canQueryDesktopCapabilities,
    retry: 1,
  })

  const providerCatalog = providerSurfaceCatalog ?? DEFAULT_PROVIDER_SURFACE_CATALOG
  const resolveSurfaceForState = (
    state: AppSettings | null,
    which: 'ocr_api' | 'llm_api',
  ): ProviderSurfaceSpec | undefined => {
    const endpoint = state?.ai_provider[which]
    const providerType = resolveProviderTypeForSurface(providerCatalog, endpoint?.surface_id, endpoint?.provider_type)
    const surfaceId =
      endpoint?.surface_id ??
      deriveDefaultProviderSurfaceId(
        providerCatalog,
        state?.ai_provider.access_mode,
        which,
        providerType,
        featureCapabilities,
      )
    return providerSurfaceById(providerCatalog, surfaceId)
  }
  const currentOcrSurface = resolveSurfaceForState(formData, 'ocr_api')
  const currentLlmSurface = resolveSurfaceForState(formData, 'llm_api')
  const deferredOcrEndpoint = useDeferredValue(formData?.ai_provider.ocr_api?.endpoint?.trim() ?? '')
  const deferredLlmEndpoint = useDeferredValue(formData?.ai_provider.llm_api?.endpoint?.trim() ?? '')
  const shouldProbeProviderEndpoint = (
    surface: ProviderSurfaceSpec | undefined,
    endpoint: string,
  ): surface is ProviderSurfaceSpec =>
    canQueryDesktopCapabilities &&
    Boolean(surface?.availability_probe) &&
    surface?.execution_kind === 'direct_http' &&
    (surface.placement_kind === 'self_hosted' || surface.placement_kind === 'custom_hosted') &&
    endpoint.length > 0

  const { data: ocrEndpointProbe, isFetching: ocrEndpointProbeLoading } = useQuery<ProviderEndpointProbeResult>({
    queryKey: ['provider-endpoint-probe', 'ocr_api', currentOcrSurface?.surface_id ?? null, deferredOcrEndpoint],
    queryFn: () =>
      probeProviderSurfaceEndpoint({
        surface_id: currentOcrSurface!.surface_id,
        endpoint_kind: 'ocr_api',
        endpoint: deferredOcrEndpoint,
      }),
    enabled: shouldProbeProviderEndpoint(currentOcrSurface, deferredOcrEndpoint),
    retry: 0,
  })

  const { data: llmEndpointProbe, isFetching: llmEndpointProbeLoading } = useQuery<ProviderEndpointProbeResult>({
    queryKey: ['provider-endpoint-probe', 'llm_api', currentLlmSurface?.surface_id ?? null, deferredLlmEndpoint],
    queryFn: () =>
      probeProviderSurfaceEndpoint({
        surface_id: currentLlmSurface!.surface_id,
        endpoint_kind: 'llm_api',
        endpoint: deferredLlmEndpoint,
      }),
    enabled: shouldProbeProviderEndpoint(currentLlmSurface, deferredLlmEndpoint),
    retry: 0,
  })

  const resetModelDiscoveryState = (targets: Array<'ocr_api' | 'llm_api'>) => {
    setModelCatalog((current) => {
      const next = { ...current }
      for (const target of targets) {
        next[target] = []
      }
      return next
    })
    setModelCatalogDetails((current) => {
      const next = { ...current }
      for (const target of targets) {
        next[target] = []
      }
      return next
    })
    setModelCatalogNotice((current) => {
      const next = { ...current }
      for (const target of targets) {
        next[target] = null
      }
      return next
    })
    setModelCatalogLoading((current) => (current && targets.includes(current) ? null : current))
  }

  const defaultByokBackendKind = secretBackendCapabilities?.byok_backend_kind ?? 'legacy_config'

  const deriveEndpointAuthMode = (
    accessMode: string,
    endpointKind: EndpointSurfaceKind,
    surface: ProviderSurfaceSpec | undefined,
  ): string => {
    if (surface?.execution_kind === 'managed_http') {
      return 'managed_oauth'
    }

    if (surface?.execution_kind === 'subprocess_cli') {
      return 'cli_bridge'
    }

    if (accessMode === 'ProviderOAuth' && endpointKind === 'llm_api') {
      return 'managed_oauth'
    }

    if (accessMode === 'ProviderSubscriptionCli' && endpointKind === 'llm_api') {
      return 'cli_bridge'
    }

    return 'api_key'
  }

  const deriveEndpointBackendKind = (authMode: string): string => {
    switch (authMode) {
      case 'managed_oauth':
        return 'os_secret_store'
      case 'cli_bridge':
        return 'bridge_managed'
      default:
        return defaultByokBackendKind
    }
  }

  const normalizeEndpointSettings = (
    accessMode: string,
    endpointKind: EndpointSurfaceKind,
    endpoint: ExternalApiSettings | null | undefined,
    requestedSurface?: ProviderSurfaceSpec,
  ): ExternalApiSettings | null => {
    const seed = endpoint ?? defaultExternalApiSettings(accessMode, endpointKind)
    if (!seed) {
      return null
    }

    const providerType = resolveProviderTypeForSurface(
      providerCatalog,
      requestedSurface?.surface_id ?? seed.surface_id,
      requestedSurface?.provider_type ?? seed.provider_type,
    )
    const surfaceId =
      requestedSurface?.surface_id ??
      deriveDefaultProviderSurfaceId(providerCatalog, accessMode, endpointKind, providerType, featureCapabilities)
    const previousSurface = providerSurfaceById(providerCatalog, seed.surface_id)
    const nextSurface = requestedSurface ?? providerSurfaceById(providerCatalog, surfaceId)
    const previousDefaultEndpoint = defaultSurfaceEndpoint(previousSurface, endpointKind)
    const nextDefaultEndpoint = defaultSurfaceEndpoint(nextSurface, endpointKind)
    const previousDefaultModel = defaultSurfaceModel(previousSurface, endpointKind)
    const nextDefaultModel = defaultSurfaceModel(nextSurface, endpointKind)
    const supportsModelSelection = surfaceSupportsModelSelection(nextSurface, endpointKind)
    const previousProviderType = resolveProviderTypeForSurface(
      providerCatalog,
      seed.surface_id,
      seed.provider_type,
    )
    const authMode = deriveEndpointAuthMode(accessMode, endpointKind, nextSurface)
    const backendKind = deriveEndpointBackendKind(authMode)
    const currentModel = seed.model?.trim() ?? ''
    const knownNextModel = surfaceKnownModel(nextSurface, currentModel)
    const knownNextModelSupported = knownNextModel
      ? endpointKind === 'ocr_api'
        ? knownNextModel.capabilities.ocr && knownNextModel.capabilities.image_input
        : knownNextModel.capabilities.llm
      : null
    const providerChanged = previousProviderType !== providerType
    const executionChanged = previousSurface?.execution_kind !== nextSurface?.execution_kind
    const shouldResetModelForSurfaceChange =
      Boolean(currentModel) &&
      previousSurface?.surface_id !== nextSurface?.surface_id &&
      (knownNextModelSupported === false || (!knownNextModel && (providerChanged || executionChanged)))

    return {
      ...seed,
      endpoint:
        !seed.endpoint.trim() || seed.endpoint === previousDefaultEndpoint ? nextDefaultEndpoint : seed.endpoint,
      model: !supportsModelSelection
        ? null
        : shouldResetModelForSurfaceChange
          ? nextDefaultModel
          : !seed.model?.trim() || seed.model === previousDefaultModel
          ? nextDefaultModel
          : seed.model,
      provider_type: providerType,
      surface_id: surfaceId,
      auth_mode: authMode,
      backend_kind: backendKind,
      can_edit_secret: backendAllowsSecretEditing(backendKind) && authMode === 'api_key',
      projection_enabled: supportsProjectionFor(authMode, backendKind) ? seed.projection_enabled : false,
    }
  }

  const sanitizeLoadedSettings = (incoming: AppSettings): AppSettings => {
    const aiProvider = { ...incoming.ai_provider }

    if (aiProvider.ocr_provider === 'Remote') {
      aiProvider.ocr_api = normalizeEndpointSettings(aiProvider.access_mode, 'ocr_api', aiProvider.ocr_api)
    }

    if (aiProvider.llm_provider === 'Remote' || aiProvider.access_mode === 'ProviderSubscriptionCli' || aiProvider.access_mode === 'ProviderOAuth') {
      aiProvider.llm_api = normalizeEndpointSettings(aiProvider.access_mode, 'llm_api', aiProvider.llm_api)
    }

    return {
      ...incoming,
      ai_provider: aiProvider,
    }
  }

  useEffect(() => {
    if (settings) {
      setFormData((current) => current ?? sanitizeLoadedSettings(settings))
    }
  }, [settings, providerCatalog, featureCapabilities])

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

  const applyAccessModeDefaults = (
    currentAiProvider: AiProviderSettings,
    nextAccessMode: string,
  ): AiProviderSettings => {
    const nextAiProvider: AiProviderSettings = {
      ...currentAiProvider,
      access_mode: nextAccessMode,
    }

    if (nextAccessMode === 'ProviderSubscriptionCli') {
      nextAiProvider.llm_provider = 'Remote'
      nextAiProvider.llm_api = normalizeEndpointSettings(nextAccessMode, 'llm_api', nextAiProvider.llm_api)
      if (nextAiProvider.ocr_provider === 'Remote') {
        nextAiProvider.ocr_api = normalizeEndpointSettings(nextAccessMode, 'ocr_api', nextAiProvider.ocr_api)
      }
      return nextAiProvider
    }

    if (nextAccessMode === 'ProviderOAuth') {
      nextAiProvider.llm_provider = 'Remote'
      nextAiProvider.llm_api = normalizeEndpointSettings(nextAccessMode, 'llm_api', nextAiProvider.llm_api)
      if (nextAiProvider.ocr_provider === 'Remote') {
        nextAiProvider.ocr_api = normalizeEndpointSettings(nextAccessMode, 'ocr_api', nextAiProvider.ocr_api)
      }
      return nextAiProvider
    }

    if (nextAiProvider.ocr_provider === 'Remote') {
      nextAiProvider.ocr_api = normalizeEndpointSettings(nextAccessMode, 'ocr_api', nextAiProvider.ocr_api)
    }

    if (nextAiProvider.llm_provider === 'Remote') {
      nextAiProvider.llm_api = normalizeEndpointSettings(nextAccessMode, 'llm_api', nextAiProvider.llm_api)
    }

    return nextAiProvider
  }

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
    if (field === 'access_mode' && typeof value === 'string') {
      resetModelDiscoveryState(['ocr_api', 'llm_api'])
    }
    if (field === 'ocr_provider' && value === 'Remote') {
      resetModelDiscoveryState(['ocr_api'])
    }
    if (field === 'llm_provider' && value === 'Remote') {
      resetModelDiscoveryState(['llm_api'])
    }

    setFormData((current) =>
      current
        ? (() => {
            if (field === 'access_mode' && typeof value === 'string') {
              return {
                ...current,
                ai_provider: applyAccessModeDefaults(current.ai_provider, value),
              }
            }

            const nextAiProvider = { ...current.ai_provider, [field]: value }

            if (field === 'ocr_provider' && value === 'Remote') {
              nextAiProvider.ocr_api = normalizeEndpointSettings(
                current.ai_provider.access_mode,
                'ocr_api',
                nextAiProvider.ocr_api,
              )
            }

            if (field === 'llm_provider' && value === 'Remote') {
              nextAiProvider.llm_api = normalizeEndpointSettings(
                current.ai_provider.access_mode,
                'llm_api',
                nextAiProvider.llm_api,
              )
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
    const surfaceId = deriveDefaultProviderSurfaceId(
      providerCatalog,
      accessMode,
      endpointKind,
      'Generic',
      featureCapabilities,
    )
    const surface = providerSurfaceById(providerCatalog, surfaceId)
    const authMode = deriveEndpointAuthMode(accessMode, endpointKind, surface)
    const backendKind = deriveEndpointBackendKind(authMode)

    return {
      endpoint: defaultSurfaceEndpoint(surface, endpointKind),
      api_key_masked: '',
      model: defaultSurfaceModel(surface, endpointKind),
      provider_type: surface?.provider_type ?? 'Generic',
      surface_id: surfaceId,
      timeout_secs: 30,
      auth_mode: authMode,
      backend_kind: backendKind,
      has_secret: false,
      can_edit_secret: backendAllowsSecretEditing(backendKind) && authMode === 'api_key',
      secret_display_hint: null,
      projection_enabled: false,
    }
  }

  const resolveEndpointSurface = (which: 'ocr_api' | 'llm_api'): ProviderSurfaceSpec | undefined =>
    resolveSurfaceForState(formData, which)

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
    sortProviderSurfaces(
      getCompatibleProviderSurfaces(providerCatalog, formData?.ai_provider.access_mode, which, featureCapabilities),
      featureCapabilities,
    )

  const getSurfaceModels = (which: 'ocr_api' | 'llm_api'): string[] => {
    const surface = resolveEndpointSurface(which)
    if (!surface) return []
    return which === 'ocr_api' ? (surface.default_models.ocr_models ?? []) : (surface.default_models.llm_models ?? [])
  }

  const isOcrModelExplicitlyUnsupported = (detail: ProviderDiscoveredModel | undefined): boolean =>
    Boolean(
      detail &&
        (detail.supports_ocr === false ||
          detail.ocr_support === 'unsupported' ||
          detail.image_input_support === 'unsupported'),
    )

  const isLlmModelExplicitlyUnsupported = (detail: ProviderDiscoveredModel | undefined): boolean =>
    Boolean(detail && detail.llm_support === 'unsupported')

  const normalizeModelId = (value: string | null | undefined): string => (value ?? '').trim().toLowerCase()

  const findModelDetail = (
    which: 'ocr_api' | 'llm_api',
    modelId: string | null | undefined,
  ): ProviderDiscoveredModel | undefined => {
    const normalized = normalizeModelId(modelId)
    if (!normalized) return undefined
    return modelCatalogDetails[which].find((detail) => normalizeModelId(detail.id) === normalized)
  }

  const getModelOptions = (which: 'ocr_api' | 'llm_api'): string[] => {
    const surfaceModels = getSurfaceModels(which)
    const discoveredModels =
      modelCatalogDetails[which].length > 0
        ? modelCatalogDetails[which]
            .filter((detail) =>
              which === 'ocr_api'
                ? !isOcrModelExplicitlyUnsupported(detail)
                : !isLlmModelExplicitlyUnsupported(detail),
            )
            .map((detail) => detail.id)
        : modelCatalog[which]
    const allowedSurfaceModels =
      which === 'ocr_api'
        ? surfaceModels.filter((model) => !isOcrModelExplicitlyUnsupported(findModelDetail(which, model)))
        : surfaceModels.filter((model) => !isLlmModelExplicitlyUnsupported(findModelDetail(which, model)))
    return Array.from(new Set([...discoveredModels, ...allowedSurfaceModels]))
  }

  const getModelCompatibilityNotice = (which: 'ocr_api' | 'llm_api'): string | null => {
    const currentModel = formData?.ai_provider[which]?.model
    const detail = findModelDetail(which, currentModel)
    if (!detail) {
      if (currentModel?.trim() && modelCatalogDetails[which].length > 0) {
        return which === 'ocr_api'
          ? t('settingsAutomation.ocrModelCompatibilityUnknown', { model: currentModel })
          : t('settingsAutomation.llmModelCompatibilityUnknown', { model: currentModel })
      }
      return null
    }
    if (which === 'ocr_api' && isOcrModelExplicitlyUnsupported(detail)) {
      return t('settingsAutomation.ocrModelUnsupported', {
        model: detail.display_name ?? detail.id,
      })
    }
    if (which === 'llm_api' && isLlmModelExplicitlyUnsupported(detail)) {
      return t('settingsAutomation.llmModelUnsupported', {
        model: detail.display_name ?? detail.id,
      })
    }
    return null
  }

  const canDiscoverModels = (which: 'ocr_api' | 'llm_api'): boolean => {
    const surface = resolveEndpointSurface(which)
    const transport = surface?.model_catalog_transport
    if (!transport) {
      return surface?.supports.model_catalog ?? true
    }

    return which === 'ocr_api' ? transport.ocr_supported : transport.llm_supported
  }

  const handleProviderSurfaceChange = (which: 'ocr_api' | 'llm_api', nextSurfaceId: string) => {
    const nextSurface = providerSurfaceById(providerCatalog, nextSurfaceId)
    if (!nextSurface) {
      return
    }

    resetModelDiscoveryState([which])
    setFormData((current) => {
      if (!current) return current
      const existing = current.ai_provider[which] ?? defaultExternalApiSettings(current.ai_provider.access_mode, which)
      return {
        ...current,
        ai_provider: {
          ...current.ai_provider,
          [which]: normalizeEndpointSettings(current.ai_provider.access_mode, which, existing, nextSurface),
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
    setModelCatalogDetails((current) => ({
      ...current,
      [which]: result.model_details ?? [],
    }))

    const preferredDiscoveredModel =
      which === 'ocr_api'
        ? (result.model_details ?? []).find((detail) => !isOcrModelExplicitlyUnsupported(detail))?.id
        : (result.model_details ?? []).find((detail) => !isLlmModelExplicitlyUnsupported(detail))?.id

    const canFallbackToRawModelList = !result.model_details || result.model_details.length === 0
    if ((!currentModel || !currentModel.trim()) && (preferredDiscoveredModel || (canFallbackToRawModelList && result.models.length > 0))) {
      handleExternalApiChange(which, 'model', preferredDiscoveredModel ?? result.models[0])
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
    const surface = resolveEndpointSurface(which)
    const usesNoAuth =
      which === 'ocr_api'
        ? surface?.ocr_transport?.auth_scheme === 'none'
        : surface?.llm_transport?.auth_scheme === 'none'
    const useSavedSecret = current.has_secret && !current.api_key_masked?.trim()
    if (!usesNoAuth && !current.api_key_masked?.trim() && !useSavedSecret) {
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
      setModelCatalogDetails((currentDetails) => ({
        ...currentDetails,
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
              allProviderSurfaces={providerCatalog.surfaces}
              providerSurfaceOptions={{
                ocr_api: getCompatibleSurfaceOptions('ocr_api'),
                llm_api: getCompatibleSurfaceOptions('llm_api'),
              }}
              featureCapabilities={featureCapabilities}
              secretBackendCapabilities={secretBackendCapabilities}
              modelCatalogNotice={modelCatalogNotice}
              modelCompatibilityNotice={{
                ocr_api: getModelCompatibilityNotice('ocr_api'),
                llm_api: getModelCompatibilityNotice('llm_api'),
              }}
              modelCatalogLoading={modelCatalogLoading}
              endpointProbeResult={{
                ocr_api: ocrEndpointProbe ?? null,
                llm_api: llmEndpointProbe ?? null,
              }}
              endpointProbeLoading={{
                ocr_api: ocrEndpointProbeLoading,
                llm_api: llmEndpointProbeLoading,
              }}
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
