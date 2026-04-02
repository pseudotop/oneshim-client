import type { Meta, StoryObj } from '@storybook/react'
import AiAutomationTab from './ai-automation'
import { makeDefaultFormData } from './stories-utils'

const noop = () => {}

const baseArgs = {
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
  onAutomationChange: noop,
  onSandboxChange: noop,
  onAiProviderChange: noop,
  onOcrValidationChange: noop,
  onSceneActionOverrideChange: noop,
  onSceneIntelligenceChange: noop,
  onExternalApiChange: noop,
  resolveProviderSurface: () => undefined,
  onProviderSurfaceChange: noop,
  onSelectAiProviderProfile: noop,
  onSaveAiProviderProfile: noop,
  onDeleteAiProviderProfile: noop,
  onDiscoverModels: noop,
  getModelOptions: () => [],
  canDiscoverModels: () => false,
} as const

const meta = {
  title: 'Settings/AiAutomationTab',
  component: AiAutomationTab,
  tags: ['autodocs'],
} satisfies Meta<typeof AiAutomationTab>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {
  args: { ...baseArgs },
}

export const AutomationEnabled: Story = {
  args: {
    ...baseArgs,
    formData: makeDefaultFormData({
      automation: { enabled: true },
      ai_provider: {
        access_mode: 'local',
        ocr_provider: 'tesseract',
        llm_provider: 'ollama',
        external_data_policy: 'redacted_only',
        allow_unredacted_external_ocr: false,
        ocr_validation: { enabled: true, min_confidence: 0.8, max_invalid_ratio: 0.2 },
        scene_action_override: { enabled: false, reason: '', approved_by: '', expires_at: null },
        scene_intelligence: {
          enabled: true,
          overlay_enabled: true,
          allow_action_execution: false,
          min_confidence: 0.7,
          max_elements: 50,
          calibration_enabled: true,
          calibration_min_elements: 5,
          calibration_min_avg_confidence: 0.7,
        },
        fallback_to_local: true,
        ocr_api: null,
        llm_api: {
          endpoint: 'http://localhost:11434',
          api_key: '',
          model: 'llama3.2',
          timeout_ms: 30000,
          max_retries: 2,
          verify_ssl: true,
        },
        active_profile_id: null,
        saved_profiles: [],
      },
    }),
  },
}

export const ExternalProviders: Story = {
  args: {
    ...baseArgs,
    formData: makeDefaultFormData({
      automation: { enabled: true },
      ai_provider: {
        access_mode: 'external',
        ocr_provider: 'external',
        llm_provider: 'external',
        external_data_policy: 'full_access',
        allow_unredacted_external_ocr: true,
        ocr_validation: { enabled: true, min_confidence: 0.7, max_invalid_ratio: 0.3 },
        scene_action_override: { enabled: false, reason: '', approved_by: '', expires_at: null },
        scene_intelligence: {
          enabled: false,
          overlay_enabled: false,
          allow_action_execution: false,
          min_confidence: 0.6,
          max_elements: 50,
          calibration_enabled: false,
          calibration_min_elements: 5,
          calibration_min_avg_confidence: 0.7,
        },
        fallback_to_local: false,
        ocr_api: {
          endpoint: 'https://api.openai.com/v1',
          api_key: 'sk-***',
          model: 'gpt-4o-mini',
          timeout_ms: 60000,
          max_retries: 3,
          verify_ssl: true,
        },
        llm_api: {
          endpoint: 'https://api.anthropic.com/v1',
          api_key: 'sk-ant-***',
          model: 'claude-sonnet-4-20250514',
          timeout_ms: 60000,
          max_retries: 3,
          verify_ssl: true,
        },
        active_profile_id: null,
        saved_profiles: [],
      },
    }),
    getModelOptions: (which: 'ocr_api' | 'llm_api') =>
      which === 'ocr_api' ? ['gpt-4o-mini', 'gpt-4o'] : ['claude-sonnet-4-20250514', 'claude-haiku-4-5-20251001'],
    canDiscoverModels: () => true,
  },
}
