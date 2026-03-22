import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useCallback, useDeferredValue, useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useSearchParams } from 'react-router-dom'
import {
  type AiProviderProfileConfig,
  type AiProviderSettings,
  type AppSettings,
  type AutomationSettings,
  discoverProviderModels,
  downloadBlob,
  type ExportDataType,
  type ExportFormat,
  type ExternalApiSettings,
  exportData,
  type FeatureCapabilitySnapshot,
  fetchFeatureCapabilities,
  fetchProviderSurfaces,
  fetchSecretBackendCapabilities,
  fetchSettings,
  fetchStorageStats,
  fetchUpdateStatus,
  type MonitorControlSettings,
  type NotificationSettings as NotificationSettingsType,
  type OcrValidationSettings as OcrValidationSettingsType,
  type PrivacySettings as PrivacySettingsType,
  type ProviderDiscoveredModel,
  type ProviderEndpointProbeResult,
  type ProviderModelsResponse,
  type ProviderSurfaceSpec,
  postUpdateAction,
  probeProviderSurfaceEndpoint,
  type SandboxSettings,
  type SavedAiProviderProfile,
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
import { useShellLayoutContext } from '../contexts/ShellLayoutContext'
import {
  defaultSurfaceEndpoint,
  defaultSurfaceModel,
  deriveDefaultProviderSurfaceId,
  type EndpointSurfaceKind,
  getCompatibleProviderSurfaces,
  providerSurfaceById,
  resolveProviderTypeForSurface,
  sortProviderSurfaces,
  surfaceCompatibleWithAccessMode,
  surfaceKnownModel,
  surfaceOcrRequiresStructuredOutputModel,
  surfaceSupportsModelSelection,
  surfaceUnknownModelPolicy,
} from '../features/providerSurfaces'
import { useToast } from '../hooks/useToast'
import { colors, typography } from '../styles/tokens'
import { cn } from '../utils/cn'
import { IS_TAURI } from '../utils/platform'
import { AiAutomationTab, CoachingGoalsTab, DataStorageTab, GeneralTab, MonitoringTab, PrivacyTab } from './settingSections'

type SettingsTabId = 'general' | 'privacy' | 'monitoring' | 'ai-automation' | 'data' | 'coaching'

function isSettingsTabId(value: string | null): value is SettingsTabId {
  return (
    value === 'general' ||
    value === 'privacy' ||
    value === 'monitoring' ||
    value === 'ai-automation' ||
    value === 'data' ||
    value === 'coaching'
  )
}

function backendAllowsSecretEditing(backendKind: string): boolean {
  return backendKind !== 'env' && backendKind !== 'bridge_managed' && backendKind !== 'unavailable'
}

function supportsProjectionFor(authMode: string, backendKind: string): boolean {
  return authMode === 'api_key' && (backendKind === 'os_secret_store' || backendKind === 'file_secret_store')
}

function modelDiscoverySensitiveField(field: keyof ExternalApiSettings): boolean {
  return (
    field === 'endpoint' ||
    field === 'api_key_masked' ||
    field === 'provider_type' ||
    field === 'surface_id' ||
    field === 'auth_mode' ||
    field === 'backend_kind' ||
    field === 'has_secret'
  )
}

function cloneExternalApiSettings(endpoint: ExternalApiSettings | null | undefined): ExternalApiSettings | null {
  return endpoint ? { ...endpoint } : null
}

function cloneAiProviderProfileConfig(
  aiProvider: AiProviderProfileConfig | AiProviderSettings,
): AiProviderProfileConfig {
  return {
    access_mode: aiProvider.access_mode,
    ocr_provider: aiProvider.ocr_provider,
    llm_provider: aiProvider.llm_provider,
    external_data_policy: aiProvider.external_data_policy,
    allow_unredacted_external_ocr: aiProvider.allow_unredacted_external_ocr,
    ocr_validation: { ...aiProvider.ocr_validation },
    scene_action_override: { ...aiProvider.scene_action_override },
    scene_intelligence: { ...aiProvider.scene_intelligence },
    fallback_to_local: aiProvider.fallback_to_local,
    ocr_api: cloneExternalApiSettings(aiProvider.ocr_api),
    llm_api: cloneExternalApiSettings(aiProvider.llm_api),
  }
}

function normalizeSavedProfileName(value: string): string {
  return value.trim().replace(/\s+/g, ' ')
}

function slugifySavedProfileId(value: string): string {
  const normalized = normalizeSavedProfileName(value)
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
  return normalized || 'ai-profile'
}

export default function Settings() {
  const { t } = useTranslation()
  const [searchParams, setSearchParams] = useSearchParams()
  const { sidebarCollapsed } = useShellLayoutContext()
  const queryClient = useQueryClient()
  const { show: showToast } = useToast()
  const [activeTab, setActiveTab] = useState<SettingsTabId>(() => {
    const tab = searchParams.get('tab')
    return isSettingsTabId(tab) ? tab : 'general'
  })
  const [formData, setFormData] = useState<AppSettings | null>(null)
  const formDataRef = useRef<AppSettings | null>(null)
  const lastLoadedSettingsRef = useRef<string | null>(null)
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
    staleTime: Number.POSITIVE_INFINITY, // only changes on user save
  })

  const { data: storageStats, isLoading: storageLoading } = useQuery({
    queryKey: ['storage-stats'],
    queryFn: fetchStorageStats,
    staleTime: 60_000, // storage stats change slowly
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
    staleTime: 300_000, // 5 min — surface catalog rarely changes
    retry: 1,
  })

  const { data: featureCapabilities } = useQuery<FeatureCapabilitySnapshot>({
    queryKey: ['feature-capabilities'],
    queryFn: fetchFeatureCapabilities,
    staleTime: 300_000, // 5 min — capabilities rarely change
    enabled: canQueryDesktopCapabilities,
    retry: 1,
  })

  const { data: secretBackendCapabilities } = useQuery({
    queryKey: ['secret-backend-capabilities'],
    queryFn: fetchSecretBackendCapabilities,
    staleTime: 300_000, // 5 min — backend capabilities rarely change
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
    queryFn: () => {
      if (!currentOcrSurface) {
        throw new Error('OCR endpoint probe requested without an active surface')
      }
      return probeProviderSurfaceEndpoint({
        surface_id: currentOcrSurface.surface_id,
        endpoint_kind: 'ocr_api',
        endpoint: deferredOcrEndpoint,
      })
    },
    enabled: shouldProbeProviderEndpoint(currentOcrSurface, deferredOcrEndpoint),
    retry: 0,
  })

  const { data: llmEndpointProbe, isFetching: llmEndpointProbeLoading } = useQuery<ProviderEndpointProbeResult>({
    queryKey: ['provider-endpoint-probe', 'llm_api', currentLlmSurface?.surface_id ?? null, deferredLlmEndpoint],
    queryFn: () => {
      if (!currentLlmSurface) {
        throw new Error('LLM endpoint probe requested without an active surface')
      }
      return probeProviderSurfaceEndpoint({
        surface_id: currentLlmSurface.surface_id,
        endpoint_kind: 'llm_api',
        endpoint: deferredLlmEndpoint,
      })
    },
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

  const defaultByokBackendKind = secretBackendCapabilities?.byok_backend_kind ?? 'unavailable'

  const deriveEndpointAuthMode = useCallback(
    (accessMode: string, endpointKind: EndpointSurfaceKind, surface: ProviderSurfaceSpec | undefined): string => {
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
    },
    [],
  )

  const deriveEndpointBackendKind = useCallback(
    (authMode: string): string => {
      switch (authMode) {
        case 'managed_oauth':
          return 'os_secret_store'
        case 'cli_bridge':
          return 'bridge_managed'
        default:
          return defaultByokBackendKind
      }
    },
    [defaultByokBackendKind],
  )

  const defaultExternalApiSettings = useCallback(
    (accessMode: string, endpointKind: EndpointSurfaceKind): ExternalApiSettings => {
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
    },
    [deriveEndpointAuthMode, deriveEndpointBackendKind, featureCapabilities, providerCatalog],
  )

  const normalizeEndpointSettings = useCallback(
    (
      accessMode: string,
      endpointKind: EndpointSurfaceKind,
      endpoint: ExternalApiSettings | null | undefined,
      requestedSurface?: ProviderSurfaceSpec,
    ): ExternalApiSettings | null => {
      const seed = endpoint ?? defaultExternalApiSettings(accessMode, endpointKind)
      if (!seed) {
        return null
      }

      const seedProviderType = resolveProviderTypeForSurface(
        providerCatalog,
        requestedSurface?.surface_id ?? seed.surface_id,
        requestedSurface?.provider_type ?? seed.provider_type,
      )
      const previousSurface = providerSurfaceById(providerCatalog, seed.surface_id)
      const preservedSurface =
        !requestedSurface && surfaceCompatibleWithAccessMode(previousSurface, accessMode, endpointKind)
          ? previousSurface
          : undefined
      const surfaceId =
        requestedSurface?.surface_id ??
        preservedSurface?.surface_id ??
        deriveDefaultProviderSurfaceId(providerCatalog, accessMode, endpointKind, seedProviderType, featureCapabilities)
      const nextSurface = requestedSurface ?? preservedSurface ?? providerSurfaceById(providerCatalog, surfaceId)
      const providerType = resolveProviderTypeForSurface(
        providerCatalog,
        nextSurface?.surface_id ?? surfaceId,
        nextSurface?.provider_type ?? seedProviderType,
      )
      const previousDefaultEndpoint = defaultSurfaceEndpoint(previousSurface, endpointKind)
      const nextDefaultEndpoint = defaultSurfaceEndpoint(nextSurface, endpointKind)
      const previousDefaultModel = defaultSurfaceModel(previousSurface, endpointKind)
      const nextDefaultModel = defaultSurfaceModel(nextSurface, endpointKind)
      const supportsModelSelection = surfaceSupportsModelSelection(nextSurface, endpointKind)
      const previousProviderType = resolveProviderTypeForSurface(providerCatalog, seed.surface_id, seed.provider_type)
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
    },
    [
      defaultExternalApiSettings,
      deriveEndpointAuthMode,
      deriveEndpointBackendKind,
      featureCapabilities,
      providerCatalog,
    ],
  )

  const normalizeAiProviderProfileConfig = useCallback(
    (config: AiProviderProfileConfig): AiProviderProfileConfig => {
      const next = cloneAiProviderProfileConfig(config)

      if (next.ocr_provider === 'Remote') {
        next.ocr_api = normalizeEndpointSettings(next.access_mode, 'ocr_api', next.ocr_api)
      }

      if (next.llm_provider === 'Remote') {
        next.llm_api = normalizeEndpointSettings(next.access_mode, 'llm_api', next.llm_api)
      }

      return next
    },
    [normalizeEndpointSettings],
  )

  const normalizeSavedProfiles = useCallback(
    (profiles: SavedAiProviderProfile[] | null | undefined): SavedAiProviderProfile[] =>
      (profiles ?? []).map((profile) => ({
        profile_id: profile.profile_id,
        name: normalizeSavedProfileName(profile.name),
        ai_provider: normalizeAiProviderProfileConfig(profile.ai_provider),
        updated_at: profile.updated_at ?? null,
      })),
    [normalizeAiProviderProfileConfig],
  )

  const sanitizeLoadedSettings = useCallback(
    (incoming: AppSettings): AppSettings => {
      const normalizedProfiles = normalizeSavedProfiles(incoming.ai_provider.saved_profiles)
      const aiProvider = {
        ...normalizeAiProviderProfileConfig(incoming.ai_provider),
        active_profile_id: normalizedProfiles.some(
          (profile) => profile.profile_id === incoming.ai_provider.active_profile_id,
        )
          ? (incoming.ai_provider.active_profile_id ?? null)
          : null,
        saved_profiles: normalizedProfiles,
      }

      return {
        ...incoming,
        ai_provider: aiProvider,
      }
    },
    [normalizeAiProviderProfileConfig, normalizeSavedProfiles],
  )

  useEffect(() => {
    formDataRef.current = formData
  }, [formData])

  useEffect(() => {
    const tab = searchParams.get('tab')
    if (isSettingsTabId(tab) && tab !== activeTab) {
      setActiveTab(tab)
    }
  }, [activeTab, searchParams])

  useEffect(() => {
    if (settings) {
      const sanitized = sanitizeLoadedSettings(settings)
      const serialized = JSON.stringify(sanitized)
      setFormData((current) => {
        if (!current) {
          return sanitized
        }

        if (lastLoadedSettingsRef.current && JSON.stringify(current) === lastLoadedSettingsRef.current) {
          return sanitized
        }

        return current
      })
      lastLoadedSettingsRef.current = serialized
    }
  }, [sanitizeLoadedSettings, settings])

  useEffect(() => {
    if (!secretBackendCapabilities) {
      return
    }

    setFormData((current) => {
      if (!current) return current

      let changed = false
      const applyBackendDefault = (endpoint: ExternalApiSettings | null): ExternalApiSettings | null => {
        if (!endpoint) return endpoint
        if (endpoint.backend_kind !== 'unavailable') return endpoint
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
      if (nextAiProvider.llm_provider === 'Remote') {
        nextAiProvider.llm_api = normalizeEndpointSettings(nextAccessMode, 'llm_api', nextAiProvider.llm_api)
      }
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

  const markAiProviderAsCustom = useCallback(
    (aiProvider: AiProviderSettings): AiProviderSettings => {
      const savedProfiles = normalizeSavedProfiles(aiProvider.saved_profiles)
      return {
        ...aiProvider,
        active_profile_id: savedProfiles.some((profile) => profile.profile_id === aiProvider.active_profile_id)
          ? (aiProvider.active_profile_id ?? null)
          : null,
        saved_profiles: savedProfiles,
      }
    },
    [normalizeSavedProfiles],
  )

  const createSavedAiProviderProfile = useCallback(
    (
      currentAiProvider: AiProviderSettings,
      existingProfiles: SavedAiProviderProfile[],
      requestedName: string,
    ): SavedAiProviderProfile | null => {
      const normalizedName = normalizeSavedProfileName(requestedName)
      if (!normalizedName) {
        return null
      }

      const activeProfile = currentAiProvider.active_profile_id
        ? existingProfiles.find((profile) => profile.profile_id === currentAiProvider.active_profile_id)
        : undefined
      const matchedByName = existingProfiles.find(
        (profile) => profile.name.localeCompare(normalizedName, undefined, { sensitivity: 'base' }) === 0,
      )
      const profileId =
        activeProfile?.name === normalizedName
          ? activeProfile.profile_id
          : (matchedByName?.profile_id ?? slugifySavedProfileId(normalizedName))

      const usedIds = new Set(
        existingProfiles.filter((profile) => profile.profile_id !== profileId).map((profile) => profile.profile_id),
      )
      let nextProfileId = profileId
      let suffix = 2
      while (usedIds.has(nextProfileId)) {
        nextProfileId = `${profileId}-${suffix}`
        suffix += 1
      }

      return {
        profile_id: nextProfileId,
        name: normalizedName,
        ai_provider: normalizeAiProviderProfileConfig(currentAiProvider),
        updated_at: new Date().toISOString(),
      }
    },
    [normalizeAiProviderProfileConfig],
  )

  const saveMutation = useMutation({
    mutationFn: updateSettings,
    onSuccess: (savedSettings) => {
      queryClient.setQueryData(['settings'], savedSettings)
      queryClient.invalidateQueries({ queryKey: ['settings'] })
      const sanitized = sanitizeLoadedSettings(savedSettings)
      lastLoadedSettingsRef.current = JSON.stringify(sanitized)
      setFormData(sanitized)
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
                ai_provider: markAiProviderAsCustom(applyAccessModeDefaults(current.ai_provider, value)),
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
              ai_provider: markAiProviderAsCustom(nextAiProvider),
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
            ai_provider: markAiProviderAsCustom({
              ...current.ai_provider,
              ocr_validation: {
                ...current.ai_provider.ocr_validation,
                [field]: value,
              },
            }),
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
            ai_provider: markAiProviderAsCustom({
              ...current.ai_provider,
              scene_action_override: {
                ...current.ai_provider.scene_action_override,
                [field]: value,
              },
            }),
          }
        : current,
    )
  }

  const handleSceneIntelligenceChange = (field: keyof SceneIntelligenceSettingsType, value: boolean | number) => {
    setFormData((current) =>
      current
        ? {
            ...current,
            ai_provider: markAiProviderAsCustom({
              ...current.ai_provider,
              scene_intelligence: {
                ...current.ai_provider.scene_intelligence,
                [field]: value,
              },
            }),
          }
        : current,
    )
  }

  const resolveEndpointSurface = (which: 'ocr_api' | 'llm_api'): ProviderSurfaceSpec | undefined =>
    resolveSurfaceForState(formData, which)

  const handleExternalApiChange = (
    which: 'ocr_api' | 'llm_api',
    field: keyof ExternalApiSettings,
    value: string | number | boolean | null,
  ) => {
    if (modelDiscoverySensitiveField(field)) {
      resetModelDiscoveryState([which])
    }
    setFormData((current) => {
      if (!current) return current
      const existing = current.ai_provider[which] ?? defaultExternalApiSettings(current.ai_provider.access_mode, which)

      return {
        ...current,
        ai_provider: markAiProviderAsCustom({
          ...current.ai_provider,
          [which]: { ...existing, [field]: value },
        }),
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
          detail.image_input_support === 'unsupported' ||
          (surfaceOcrRequiresStructuredOutputModel(resolveEndpointSurface('ocr_api')) &&
            detail.structured_output_support === 'unsupported')),
    )

  const isLlmModelExplicitlyUnsupported = (detail: ProviderDiscoveredModel | undefined): boolean =>
    Boolean(detail && detail.llm_support === 'unsupported')

  const isOcrModelCompatibilityUnknown = (detail: ProviderDiscoveredModel | undefined): boolean =>
    Boolean(
      detail &&
        (detail.ocr_support === 'unknown' ||
          detail.image_input_support === 'unknown' ||
          (surfaceOcrRequiresStructuredOutputModel(resolveEndpointSurface('ocr_api')) &&
            detail.structured_output_support === 'unknown') ||
          detail.supports_ocr == null),
    )

  const isLlmModelCompatibilityUnknown = (detail: ProviderDiscoveredModel | undefined): boolean =>
    Boolean(detail && detail.llm_support === 'unknown')

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
    const surface = resolveEndpointSurface(which)
    const unknownPolicy = surfaceUnknownModelPolicy(surface, which)
    const isAllowedDiscoveredModel = (detail: ProviderDiscoveredModel): boolean => {
      if (which === 'ocr_api') {
        if (isOcrModelExplicitlyUnsupported(detail)) {
          return false
        }
        return !(unknownPolicy === 'reject' && isOcrModelCompatibilityUnknown(detail))
      }

      if (isLlmModelExplicitlyUnsupported(detail)) {
        return false
      }
      return !(unknownPolicy === 'reject' && isLlmModelCompatibilityUnknown(detail))
    }
    const discoveredModels =
      modelCatalogDetails[which].length > 0
        ? modelCatalogDetails[which].filter((detail) => isAllowedDiscoveredModel(detail)).map((detail) => detail.id)
        : modelCatalog[which]
    const allowedSurfaceModels =
      which === 'ocr_api'
        ? surfaceModels.filter((model) => {
            const detail = findModelDetail(which, model)
            return !detail || isAllowedDiscoveredModel(detail)
          })
        : surfaceModels.filter((model) => {
            const detail = findModelDetail(which, model)
            return !detail || isAllowedDiscoveredModel(detail)
          })
    return Array.from(new Set([...discoveredModels, ...allowedSurfaceModels]))
  }

  const getModelCompatibilityNotice = (which: 'ocr_api' | 'llm_api'): string | null => {
    const currentModel = formData?.ai_provider[which]?.model
    const surface = resolveEndpointSurface(which)
    const unknownPolicy = surfaceUnknownModelPolicy(surface, which)
    const detail = findModelDetail(which, currentModel)
    if (!detail) {
      if (currentModel?.trim() && !surfaceKnownModel(surface, currentModel)) {
        if (unknownPolicy === 'reject') {
          return which === 'ocr_api'
            ? t('settingsAutomation.ocrModelCompatibilityUnknownRejected', { model: currentModel })
            : t('settingsAutomation.llmModelCompatibilityUnknownRejected', { model: currentModel })
        }
        if (unknownPolicy === 'warn') {
          return which === 'ocr_api'
            ? t('settingsAutomation.ocrModelCompatibilityUnknown', { model: currentModel })
            : t('settingsAutomation.llmModelCompatibilityUnknown', { model: currentModel })
        }
      }
      return null
    }
    if (which === 'ocr_api' && isOcrModelExplicitlyUnsupported(detail)) {
      return t('settingsAutomation.ocrModelUnsupported', {
        model: detail.display_name ?? detail.id,
      })
    }
    if (which === 'ocr_api' && isOcrModelCompatibilityUnknown(detail)) {
      if (unknownPolicy === 'reject') {
        return t('settingsAutomation.ocrModelCompatibilityUnknownRejected', {
          model: detail.display_name ?? detail.id,
        })
      }
      if (unknownPolicy === 'warn') {
        return t('settingsAutomation.ocrModelCompatibilityUnknown', {
          model: detail.display_name ?? detail.id,
        })
      }
    }
    if (which === 'llm_api' && isLlmModelExplicitlyUnsupported(detail)) {
      return t('settingsAutomation.llmModelUnsupported', {
        model: detail.display_name ?? detail.id,
      })
    }
    if (which === 'llm_api' && isLlmModelCompatibilityUnknown(detail)) {
      if (unknownPolicy === 'reject') {
        return t('settingsAutomation.llmModelCompatibilityUnknownRejected', {
          model: detail.display_name ?? detail.id,
        })
      }
      if (unknownPolicy === 'warn') {
        return t('settingsAutomation.llmModelCompatibilityUnknown', {
          model: detail.display_name ?? detail.id,
        })
      }
    }
    return null
  }

  const canDiscoverModels = (which: 'ocr_api' | 'llm_api'): boolean => {
    const surface = resolveEndpointSurface(which)
    if (!surface) {
      return false
    }
    const transport = surface?.model_catalog_transport
    if (!transport) {
      return surface.supports.model_catalog
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
        ai_provider: markAiProviderAsCustom({
          ...current.ai_provider,
          [which]: normalizeEndpointSettings(current.ai_provider.access_mode, which, existing, nextSurface),
        }),
      }
    })
  }

  const handleSelectAiProviderProfile = (profileId: string | null) => {
    resetModelDiscoveryState(['ocr_api', 'llm_api'])
    setFormData((current) => {
      if (!current) return current

      const savedProfiles = normalizeSavedProfiles(current.ai_provider.saved_profiles)
      if (!profileId) {
        return {
          ...current,
          ai_provider: {
            ...current.ai_provider,
            active_profile_id: null,
            saved_profiles: savedProfiles,
          },
        }
      }

      const selectedProfile = savedProfiles.find((profile) => profile.profile_id === profileId)
      if (!selectedProfile) {
        return {
          ...current,
          ai_provider: {
            ...current.ai_provider,
            active_profile_id: null,
            saved_profiles: savedProfiles,
          },
        }
      }

      return {
        ...current,
        ai_provider: {
          ...normalizeAiProviderProfileConfig(selectedProfile.ai_provider),
          active_profile_id: selectedProfile.profile_id,
          saved_profiles: savedProfiles,
        },
      }
    })
  }

  const handleSaveAiProviderProfile = (requestedName: string) => {
    let savedProfileName: string | null = null
    setFormData((current) => {
      if (!current) return current

      const savedProfiles = normalizeSavedProfiles(current.ai_provider.saved_profiles)
      const nextProfile = createSavedAiProviderProfile(current.ai_provider, savedProfiles, requestedName)
      if (!nextProfile) {
        return current
      }

      const nextProfiles = [
        ...savedProfiles.filter((profile) => profile.profile_id !== nextProfile.profile_id),
        nextProfile,
      ].sort((left, right) => left.name.localeCompare(right.name, undefined, { sensitivity: 'base' }))
      savedProfileName = nextProfile.name

      return {
        ...current,
        ai_provider: {
          ...current.ai_provider,
          active_profile_id: nextProfile.profile_id,
          saved_profiles: nextProfiles,
        },
      }
    })

    if (savedProfileName) {
      showToast('success', t('settingsAutomation.profileSavedSuccess', { name: savedProfileName }), 3000)
    } else {
      showToast('error', t('settingsAutomation.profileNameRequired'), 4000)
    }
  }

  const handleDeleteAiProviderProfile = (profileId: string) => {
    let deletedProfileName: string | null = null
    setFormData((current) => {
      if (!current) return current

      const savedProfiles = normalizeSavedProfiles(current.ai_provider.saved_profiles)
      const profileToDelete = savedProfiles.find((profile) => profile.profile_id === profileId)
      if (!profileToDelete) {
        return current
      }

      deletedProfileName = profileToDelete.name
      return {
        ...current,
        ai_provider: {
          ...current.ai_provider,
          active_profile_id:
            current.ai_provider.active_profile_id === profileId
              ? null
              : (current.ai_provider.active_profile_id ?? null),
          saved_profiles: savedProfiles.filter((profile) => profile.profile_id !== profileId),
        },
      }
    })

    if (deletedProfileName) {
      showToast('success', t('settingsAutomation.profileDeletedSuccess', { name: deletedProfileName }), 3000)
    }
  }

  const handleModelDiscoveryResult = (
    which: 'ocr_api' | 'llm_api',
    currentModel: string | null | undefined,
    requestSignature: string,
    result: ProviderModelsResponse,
  ) => {
    const latestSignature = modelDiscoverySignature(formDataRef.current?.ai_provider[which])
    if (latestSignature !== requestSignature) {
      return
    }

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

    const unknownPolicy = surfaceUnknownModelPolicy(resolveEndpointSurface(which), which)
    const preferredDiscoveredModel =
      which === 'ocr_api'
        ? (result.model_details ?? []).find(
            (detail) =>
              !isOcrModelExplicitlyUnsupported(detail) &&
              !(unknownPolicy === 'reject' && isOcrModelCompatibilityUnknown(detail)),
          )?.id
        : (result.model_details ?? []).find(
            (detail) =>
              !isLlmModelExplicitlyUnsupported(detail) &&
              !(unknownPolicy === 'reject' && isLlmModelCompatibilityUnknown(detail)),
          )?.id

    const canFallbackToRawModelList = !result.model_details || result.model_details.length === 0
    if (
      (!currentModel || !currentModel.trim()) &&
      (preferredDiscoveredModel ||
        (canFallbackToRawModelList && unknownPolicy !== 'reject' && result.models.length > 0))
    ) {
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
      const requestSignature = modelDiscoverySignature(current)
      const result = await discoverProviderModels({
        provider_type: current.provider_type ?? 'Generic',
        api_key: current.api_key_masked,
        endpoint: current.endpoint || null,
        surface: which,
        surface_id: current.surface_id || null,
        use_saved_secret: useSavedSecret,
      })
      handleModelDiscoveryResult(which, current.model, requestSignature, result)
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

  const modelDiscoverySignature = (endpoint: ExternalApiSettings | null | undefined): string =>
    JSON.stringify({
      provider_type: endpoint?.provider_type ?? '',
      surface_id: endpoint?.surface_id ?? '',
      endpoint: endpoint?.endpoint?.trim() ?? '',
      auth_mode: endpoint?.auth_mode ?? '',
      backend_kind: endpoint?.backend_kind ?? '',
      api_key_masked: endpoint?.api_key_masked ?? '',
      has_secret: Boolean(endpoint?.has_secret),
    })

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
    { id: 'coaching', label: t('settings.tabs.coaching', 'Coaching Goals') },
  ]

  const serializedFormData = formData ? JSON.stringify(formData) : null
  const hasUnsavedChanges = Boolean(
    settings && formData && serializedFormData && serializedFormData !== lastLoadedSettingsRef.current,
  )
  const saveDisabled = !settings || !formData || saveMutation.isPending || !hasUnsavedChanges

  const handleSubmit = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (!formData) {
      return
    }
    saveMutation.mutate(formData)
  }

  const handleTabChange = (tab: SettingsTabId) => {
    setActiveTab(tab)
    const nextParams = new URLSearchParams(searchParams)
    nextParams.set('tab', tab)
    setSearchParams(nextParams, { replace: true })
  }

  const handleRevertChanges = () => {
    const lastLoaded = lastLoadedSettingsRef.current
    if (!lastLoaded) {
      return
    }

    const parsed = JSON.parse(lastLoaded) as AppSettings
    setFormData(parsed)
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
    <div className="min-h-full space-y-6 p-6 pb-28">
      <div className="flex items-center justify-between">
        <h1 className={cn(typography.h1, colors.text.pageTitle)}>
          {t('settings.title')}
          {activeTab !== 'general' && (
            <span className="text-base font-normal text-gray-400 ml-2">
              {'›'} {tabs.find(tab => tab.id === activeTab)?.label}
            </span>
          )}
        </h1>
      </div>

      {sidebarCollapsed && (
        <Tabs
          tabs={tabs}
          activeTab={activeTab}
          onTabChange={(tab) => handleTabChange(tab as SettingsTabId)}
          ariaLabel={t('settings.title')}
          idBase="settings"
        />
      )}

      {hasUnsavedChanges && (
        <div className="pointer-events-none fixed right-6 bottom-10 z-30 flex justify-end">
          <div className="pointer-events-auto flex items-center gap-4 rounded-xl border border-muted bg-surface-overlay px-4 py-3 shadow-2xl">
            <div className="min-w-0">
              <p className={cn('font-semibold text-sm', colors.text.primary)}>{t('settings.unsavedChanges')}</p>
              <p className={cn('text-xs', colors.text.secondary)}>{t('settings.unsavedChangesHint')}</p>
            </div>
            <Button
              type="button"
              variant="secondary"
              size="lg"
              onClick={handleRevertChanges}
              disabled={saveMutation.isPending}
            >
              {t('settings.revertChanges')}
            </Button>
            <Button
              data-testid="settings-save-floating"
              type="submit"
              form="settings-form"
              variant="primary"
              size="lg"
              isLoading={saveMutation.isPending}
              disabled={saveDisabled}
            >
              {saveMutation.isPending ? t('settings.saving') : t('settings.saveSettings')}
            </Button>
          </div>
        </div>
      )}

      <form id="settings-form" className="space-y-6" onSubmit={handleSubmit}>
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
              onSelectAiProviderProfile={handleSelectAiProviderProfile}
              onSaveAiProviderProfile={handleSaveAiProviderProfile}
              onDeleteAiProviderProfile={handleDeleteAiProviderProfile}
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

        <div
          id="settings-panel-coaching"
          role="tabpanel"
          aria-labelledby="settings-tab-coaching"
          hidden={activeTab !== 'coaching'}
          aria-hidden={activeTab !== 'coaching'}
        >
          <fieldset disabled={activeTab !== 'coaching'} className="m-0 min-w-0 border-0 p-0">
            <CoachingGoalsTab />
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
