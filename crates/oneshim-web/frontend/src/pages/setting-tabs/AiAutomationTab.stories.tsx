import type { Meta, StoryObj } from '@storybook/react'
import AiAutomationTab from './AiAutomationTab'
import { makeDefaultFormData } from './stories-utils'

const meta = {
  title: 'Settings/AiAutomationTab',
  component: AiAutomationTab,
} satisfies Meta<typeof AiAutomationTab>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {
  args: {
    formData: makeDefaultFormData(),
    allProviderSurfaces: [],
    providerSurfaceOptions: { ocr_api: [], llm_api: [] },
    featureCapabilities: null,
    secretBackendCapabilities: null,
    modelCatalogNotice: { ocr_api: null, llm_api: null },
    modelCompatibilityNotice: { ocr_api: null, llm_api: null },
    modelCatalogLoading: null,
    endpointProbeResult: { ocr_api: null, llm_api: null },
    endpointProbeLoading: { ocr_api: false, llm_api: false },
    onAutomationChange: () => {},
    onSandboxChange: () => {},
    onAiProviderChange: () => {},
    onOcrValidationChange: () => {},
    onSceneActionOverrideChange: () => {},
    onSceneIntelligenceChange: () => {},
    onExternalApiChange: () => {},
    resolveProviderSurface: () => undefined,
    onProviderSurfaceChange: () => {},
    onSelectAiProviderProfile: () => {},
    onSaveAiProviderProfile: () => {},
    onDeleteAiProviderProfile: () => {},
    onDiscoverModels: () => {},
    getModelOptions: () => [],
    canDiscoverModels: () => false,
  },
}
