import type {
  AiProviderSettings,
  AutomationSettings,
  ExternalApiSettings,
  FeatureCapabilitySnapshot,
  OcrValidationSettings as OcrValidationSettingsType,
  ProviderEndpointProbeResult,
  ProviderSurfaceSpec,
  SandboxSettings,
  SceneActionOverrideSettings as SceneActionOverrideSettingsType,
  SceneIntelligenceSettings as SceneIntelligenceSettingsType,
  SecretBackendCapabilities,
} from '../../../api/client'
import type { EndpointSurfaceKind } from '../../../features/providerSurfaces'
import type { SettingsFormTabProps } from '../types'

export interface AiAutomationTabProps extends SettingsFormTabProps {
  allProviderSurfaces: ProviderSurfaceSpec[]
  providerSurfaceOptions: Record<'ocr_api' | 'llm_api', ProviderSurfaceSpec[]>
  featureCapabilities?: FeatureCapabilitySnapshot | null
  secretBackendCapabilities?: SecretBackendCapabilities | null
  modelCatalogNotice: Record<'ocr_api' | 'llm_api', string | null>
  modelCompatibilityNotice: Record<'ocr_api' | 'llm_api', string | null>
  modelCatalogLoading: 'ocr_api' | 'llm_api' | null
  endpointProbeResult: Record<'ocr_api' | 'llm_api', ProviderEndpointProbeResult | null>
  endpointProbeLoading: Record<'ocr_api' | 'llm_api', boolean>
  onAutomationChange: (field: keyof AutomationSettings, value: boolean) => void
  onSandboxChange: (field: keyof SandboxSettings, value: boolean | string | number | string[]) => void
  onAiProviderChange: (
    field: keyof AiProviderSettings,
    value: string | boolean | ExternalApiSettings | OcrValidationSettingsType | SceneIntelligenceSettingsType | null,
  ) => void
  onOcrValidationChange: (field: keyof OcrValidationSettingsType, value: boolean | number) => void
  onSceneActionOverrideChange: (field: keyof SceneActionOverrideSettingsType, value: boolean | string | null) => void
  onSceneIntelligenceChange: (field: keyof SceneIntelligenceSettingsType, value: boolean | number) => void
  onExternalApiChange: (
    which: 'ocr_api' | 'llm_api',
    field: keyof ExternalApiSettings,
    value: string | number | boolean | null,
  ) => void
  resolveProviderSurface: (which: 'ocr_api' | 'llm_api') => ProviderSurfaceSpec | undefined
  onProviderSurfaceChange: (which: 'ocr_api' | 'llm_api', surfaceId: string) => void
  onSelectAiProviderProfile: (profileId: string | null) => void
  onSaveAiProviderProfile: (name: string) => void
  onDeleteAiProviderProfile: (profileId: string) => void
  onDiscoverModels: (which: 'ocr_api' | 'llm_api') => void
  getModelOptions: (which: 'ocr_api' | 'llm_api') => string[]
  canDiscoverModels: (which: 'ocr_api' | 'llm_api') => boolean
}

export interface EndpointConfigProps {
  formData: AiAutomationTabProps['formData']
  endpointKind: 'ocr_api' | 'llm_api'
  surface: ProviderSurfaceSpec | undefined
  usesNoAuth: boolean
  supportsModelSelection: boolean
  modelSupport: boolean | null
  modelCatalogNotice: string | null
  modelCompatibilityNotice: string | null
  modelCatalogLoading: 'ocr_api' | 'llm_api' | null
  endpointProbeResult: ProviderEndpointProbeResult | null
  endpointProbeLoading: boolean
  onExternalApiChange: AiAutomationTabProps['onExternalApiChange']
  onDiscoverModels: AiAutomationTabProps['onDiscoverModels']
  getModelOptions: AiAutomationTabProps['getModelOptions']
  canDiscoverModels: AiAutomationTabProps['canDiscoverModels']
  showDirectApiFields: boolean
  showManagedHttpFields: boolean
  showSubprocessFields: boolean
}

export interface ProfileManagerProps {
  formData: AiAutomationTabProps['formData']
  onSelectAiProviderProfile: AiAutomationTabProps['onSelectAiProviderProfile']
  onSaveAiProviderProfile: AiAutomationTabProps['onSaveAiProviderProfile']
  onDeleteAiProviderProfile: AiAutomationTabProps['onDeleteAiProviderProfile']
}

export interface SandboxConfigProps {
  formData: AiAutomationTabProps['formData']
  onSandboxChange: AiAutomationTabProps['onSandboxChange']
}

export interface SceneIntelligenceConfigProps {
  formData: AiAutomationTabProps['formData']
  onSceneActionOverrideChange: AiAutomationTabProps['onSceneActionOverrideChange']
  onSceneIntelligenceChange: AiAutomationTabProps['onSceneIntelligenceChange']
  onOcrValidationChange: AiAutomationTabProps['onOcrValidationChange']
}

export interface SurfaceStatusProps {
  surface: ProviderSurfaceSpec | undefined
  endpointKind: EndpointSurfaceKind
  featureCapabilities?: FeatureCapabilitySnapshot | null
  endpointProbeResult: ProviderEndpointProbeResult | null
  endpointProbeLoading: boolean
  usesCustomSelfHostedEndpoint: boolean
}
