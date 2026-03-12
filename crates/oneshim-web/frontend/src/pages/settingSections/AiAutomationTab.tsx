/**
 * AI & Automation settings tab: AI provider config, sandbox, automation policy, scene intelligence.
 */
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  type AiProviderSettings,
  type AppSettings,
  type AutomationSettings,
  discoverProviderModels,
  type ExternalApiSettings,
  fetchProviderPresets,
  fetchSettings,
  type ProviderModelsResponse,
  type ProviderPreset,
  type SandboxSettings,
  type SceneIntelligenceSettings as SceneIntelligenceSettingsType,
  updateSettings,
} from '../../api/client'
import { Button, Card, CardTitle, Input, Select, Spinner } from '../../components/ui'
import { useToast } from '../../hooks/useToast'
import { colors, form } from '../../styles/tokens'
import { cn } from '../../utils/cn'
import ToggleRow from './ToggleRow'

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

export default function AiAutomationTab() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const toast = useToast()

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
      toast.show('success', t('settings.savedFull'))
    },
    onError: (error: Error) => {
      toast.show('error', error.message)
    },
  })

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (formData) {
      mutation.mutate(formData)
    }
  }

  const handleAutomationChange = (field: keyof AutomationSettings, value: boolean) => {
    if (formData) {
      setFormData({
        ...formData,
        automation: { ...formData.automation, [field]: value },
      })
    }
  }

  const handleSandboxChange = (field: keyof SandboxSettings, value: boolean | string | number | string[]) => {
    if (formData) {
      setFormData({
        ...formData,
        sandbox: { ...formData.sandbox, [field]: value },
      })
    }
  }

  const handleAiProviderChange = (
    field: keyof AiProviderSettings,
    value: string | boolean | ExternalApiSettings | SceneIntelligenceSettingsType | null,
  ) => {
    if (formData) {
      setFormData({
        ...formData,
        ai_provider: { ...formData.ai_provider, [field]: value },
      })
    }
  }

  const handleSceneIntelligenceChange = (field: keyof SceneIntelligenceSettingsType, value: boolean | number) => {
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
    value: string | number | null,
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

    setFormData((prev) => {
      if (!prev) return prev
      const current = prev.ai_provider[which] ?? defaultExternalApiSettings()
      const endpoint = current.endpoint && current.endpoint.trim().length > 0 ? current.endpoint : presetEndpoint
      const model = current.model && current.model.trim().length > 0 ? current.model : presetModel
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
    result: ProviderModelsResponse,
  ) => {
    setModelCatalog((prev) => ({
      ...prev,
      [which]: result.models,
    }))
    setModelCatalogNotice((prev) => ({
      ...prev,
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
      toast.show('error', t('settingsAutomation.modelDiscoveryMissingConfig'))
      return
    }
    if (!current.api_key_masked?.trim()) {
      toast.show('error', t('settingsAutomation.modelDiscoveryMissingKey'))
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
      toast.show('error', message)
    } finally {
      setModelCatalogLoading(null)
    }
  }

  if (settingsLoading) {
    return (
      <div className="flex h-64 items-center justify-center">
        <Spinner size="lg" className={colors.primary.text} />
        <span className={cn('ml-3', colors.text.secondary)}>{t('common.loading')}</span>
      </div>
    )
  }

  if (!formData) return null

  return (
    <form onSubmit={handleSubmit} className="space-y-6">
      {/* Automation Toggle */}
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

      {/* Sandbox */}
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('settingsAutomation.sandboxTitle')}</CardTitle>
        <div className="space-y-4">
          <ToggleRow
            label={t('settingsAutomation.sandboxEnabled')}
            description={t('settingsAutomation.sandboxEnabledDescription')}
            checked={formData.sandbox.enabled}
            onChange={(v) => handleSandboxChange('enabled', v)}
          />

          <div className={`space-y-4 ${!formData.sandbox.enabled ? 'pointer-events-none opacity-50' : ''}`}>
            <div>
              <label htmlFor="settings-sandbox-profile" className={form.label}>
                {t('settingsAutomation.sandboxProfile')}
              </label>
              <Select
                id="settings-sandbox-profile"
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

      {/* AI Provider */}
      <Card id="section-ai" variant="default" padding="lg">
        <CardTitle className="mb-4">{t('settingsAutomation.aiTitle')}</CardTitle>
        <div className="space-y-4">
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
            <div>
              <label htmlFor="settings-ocr-provider" className={form.label}>
                {t('settingsAutomation.ocrProvider')}
              </label>
              <Select
                id="settings-ocr-provider"
                value={formData.ai_provider.ocr_provider}
                onChange={(e) => handleAiProviderChange('ocr_provider', e.target.value)}
              >
                <option value="Local">Local</option>
                <option value="Remote">Remote</option>
              </Select>
            </div>
            <div>
              <label htmlFor="settings-llm-provider" className={form.label}>
                {t('settingsAutomation.llmProvider')}
              </label>
              <Select
                id="settings-llm-provider"
                value={formData.ai_provider.llm_provider}
                onChange={(e) => handleAiProviderChange('llm_provider', e.target.value)}
              >
                <option value="Local">Local</option>
                <option value="Remote">Remote</option>
              </Select>
            </div>
          </div>

          <div>
            <label htmlFor="settings-data-policy" className={form.label}>
              {t('settingsAutomation.dataPolicy')}
            </label>
            <Select
              id="settings-data-policy"
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

          <ToggleRow
            label={t('settingsAutomation.fallbackToLocal')}
            description={t('settingsAutomation.fallbackToLocalDescription')}
            checked={formData.ai_provider.fallback_to_local}
            onChange={(v) => handleAiProviderChange('fallback_to_local', v)}
          />

          {/* Scene Intelligence */}
          <div className="space-y-3 rounded-lg border border-muted p-4">
            <h4 className="font-medium text-content-strong text-sm">
              {t('settingsAutomation.sceneIntelligenceTitle', 'Scene Intelligence')}
            </h4>
            <ToggleRow
              label={t('settingsAutomation.sceneIntelligenceEnabled', 'Enable Scene Intelligence')}
              description={t(
                'settingsAutomation.sceneIntelligenceEnabledDescription',
                'Enable OCR-based UI structure detection and assistant recommendations.',
              )}
              checked={formData.ai_provider.scene_intelligence.enabled}
              onChange={(v) => handleSceneIntelligenceChange('enabled', v)}
            />
            <div
              className={`space-y-3 ${!formData.ai_provider.scene_intelligence.enabled ? 'pointer-events-none opacity-50' : ''}`}
            >
              <ToggleRow
                label={t('settingsAutomation.sceneOverlayEnabled', 'Show Overlay')}
                description={t(
                  'settingsAutomation.sceneOverlayEnabledDescription',
                  'Render detected UI element boxes on session replay screenshots.',
                )}
                checked={formData.ai_provider.scene_intelligence.overlay_enabled}
                onChange={(v) => handleSceneIntelligenceChange('overlay_enabled', v)}
              />
              <ToggleRow
                label={t('settingsAutomation.sceneAllowExecution', 'Allow Scene Action Execution')}
                description={t(
                  'settingsAutomation.sceneAllowExecutionDescription',
                  'Permit direct click/type execution from scene coordinates (RPA gate).',
                )}
                checked={formData.ai_provider.scene_intelligence.allow_action_execution}
                onChange={(v) => handleSceneIntelligenceChange('allow_action_execution', v)}
              />
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                <div>
                  <label
                    htmlFor="settings-scene-min-confidence"
                    className="mb-1 block text-content-secondary text-xs"
                  >
                    {t('settingsAutomation.sceneMinConfidence', 'Scene Min Confidence')}
                  </label>
                  <Input
                    id="settings-scene-min-confidence"
                    type="number"
                    min={0}
                    max={1}
                    step={0.05}
                    value={formData.ai_provider.scene_intelligence.min_confidence}
                    onChange={(e) => handleSceneIntelligenceChange('min_confidence', Number(e.target.value))}
                  />
                </div>
                <div>
                  <label
                    htmlFor="settings-scene-max-elements"
                    className="mb-1 block text-content-secondary text-xs"
                  >
                    {t('settingsAutomation.sceneMaxElements', 'Scene Max Elements')}
                  </label>
                  <Input
                    id="settings-scene-max-elements"
                    type="number"
                    min={1}
                    max={1000}
                    step={1}
                    value={formData.ai_provider.scene_intelligence.max_elements}
                    onChange={(e) => handleSceneIntelligenceChange('max_elements', Number(e.target.value))}
                  />
                </div>
              </div>
              <div className="space-y-3 rounded-md bg-surface-elevated/70 p-3">
                <ToggleRow
                  label={t('settingsAutomation.sceneCalibrationEnabled', 'Enable Calibration Validation')}
                  description={t(
                    'settingsAutomation.sceneCalibrationEnabledDescription',
                    'Validate whether current scene quality is sufficient before assistant usage.',
                  )}
                  checked={formData.ai_provider.scene_intelligence.calibration_enabled}
                  onChange={(v) => handleSceneIntelligenceChange('calibration_enabled', v)}
                />
                <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                  <div>
                    <label
                      htmlFor="settings-scene-cal-min-elements"
                      className="mb-1 block text-content-secondary text-xs"
                    >
                      {t('settingsAutomation.sceneCalibrationMinElements', 'Calibration Min Elements')}
                    </label>
                    <Input
                      id="settings-scene-cal-min-elements"
                      type="number"
                      min={1}
                      max={1000}
                      step={1}
                      value={formData.ai_provider.scene_intelligence.calibration_min_elements}
                      onChange={(e) =>
                        handleSceneIntelligenceChange('calibration_min_elements', Number(e.target.value))
                      }
                    />
                  </div>
                  <div>
                    <label
                      htmlFor="settings-scene-cal-min-confidence"
                      className="mb-1 block text-content-secondary text-xs"
                    >
                      {t('settingsAutomation.sceneCalibrationMinAvgConfidence', 'Calibration Min Avg Confidence')}
                    </label>
                    <Input
                      id="settings-scene-cal-min-confidence"
                      type="number"
                      min={0}
                      max={1}
                      step={0.05}
                      value={formData.ai_provider.scene_intelligence.calibration_min_avg_confidence}
                      onChange={(e) =>
                        handleSceneIntelligenceChange('calibration_min_avg_confidence', Number(e.target.value))
                      }
                    />
                  </div>
                </div>
              </div>
            </div>
          </div>

          {/* OCR External API */}
          {formData.ai_provider.ocr_provider === 'Remote' && (
            <div className="space-y-3 rounded-lg border border-muted p-4">
              <h4 className="font-medium text-content-strong text-sm">OCR {t('settingsAutomation.externalApi')}</h4>
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                <div>
                  <label htmlFor="settings-ocr-provider-type" className="mb-1 block text-content-secondary text-xs">
                    {t('settingsAutomation.providerType')}
                  </label>
                  <Select
                    id="settings-ocr-provider-type"
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
                  <label htmlFor="settings-ocr-endpoint" className="mb-1 block text-content-secondary text-xs">
                    {t('settingsAutomation.endpoint')}
                  </label>
                  <Input
                    id="settings-ocr-endpoint"
                    type="text"
                    value={formData.ai_provider.ocr_api?.endpoint ?? ''}
                    onChange={(e) => handleExternalApiChange('ocr_api', 'endpoint', e.target.value)}
                    placeholder={t('settingsAutomation.endpointPlaceholderOcr', 'https://api.example.com/ocr')}
                  />
                </div>
                <div>
                  <label htmlFor="settings-ocr-api-key" className="mb-1 block text-content-secondary text-xs">
                    {t('settingsAutomation.apiKey')}
                  </label>
                  <Input
                    id="settings-ocr-api-key"
                    type="password"
                    value={formData.ai_provider.ocr_api?.api_key_masked ?? ''}
                    onChange={(e) => handleExternalApiChange('ocr_api', 'api_key_masked', e.target.value)}
                    placeholder={t('settingsAutomation.apiKeyPlaceholder')}
                  />
                </div>
                <div>
                  <label htmlFor="settings-ocr-model" className="mb-1 block text-content-secondary text-xs">
                    {t('settingsAutomation.model')}
                  </label>
                  <Input
                    id="settings-ocr-model"
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
                  <label htmlFor="settings-ocr-timeout" className="mb-1 block text-content-secondary text-xs">
                    {t('settingsAutomation.timeoutSecs')}
                  </label>
                  <Input
                    id="settings-ocr-timeout"
                    type="number"
                    min={5}
                    max={300}
                    value={formData.ai_provider.ocr_api?.timeout_secs ?? 30}
                    onChange={(e) =>
                      handleExternalApiChange('ocr_api', 'timeout_secs', parseInt(e.target.value, 10) || 30)
                    }
                  />
                </div>
              </div>
              {modelCatalogNotice.ocr_api && (
                <p className="text-content-secondary text-xs">{modelCatalogNotice.ocr_api}</p>
              )}
            </div>
          )}

          {/* LLM External API */}
          {formData.ai_provider.llm_provider === 'Remote' && (
            <div className="space-y-3 rounded-lg border border-muted p-4">
              <h4 className="font-medium text-content-strong text-sm">LLM {t('settingsAutomation.externalApi')}</h4>
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                <div>
                  <label htmlFor="settings-llm-provider-type" className="mb-1 block text-content-secondary text-xs">
                    {t('settingsAutomation.providerType')}
                  </label>
                  <Select
                    id="settings-llm-provider-type"
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
                  <label htmlFor="settings-llm-endpoint" className="mb-1 block text-content-secondary text-xs">
                    {t('settingsAutomation.endpoint')}
                  </label>
                  <Input
                    id="settings-llm-endpoint"
                    type="text"
                    value={formData.ai_provider.llm_api?.endpoint ?? ''}
                    onChange={(e) => handleExternalApiChange('llm_api', 'endpoint', e.target.value)}
                    placeholder={t('settingsAutomation.endpointPlaceholderLlm', 'https://api.example.com/llm')}
                  />
                </div>
                <div>
                  <label htmlFor="settings-llm-api-key" className="mb-1 block text-content-secondary text-xs">
                    {t('settingsAutomation.apiKey')}
                  </label>
                  <Input
                    id="settings-llm-api-key"
                    type="password"
                    value={formData.ai_provider.llm_api?.api_key_masked ?? ''}
                    onChange={(e) => handleExternalApiChange('llm_api', 'api_key_masked', e.target.value)}
                    placeholder={t('settingsAutomation.apiKeyPlaceholder')}
                  />
                </div>
                <div>
                  <label htmlFor="settings-llm-model" className="mb-1 block text-content-secondary text-xs">
                    {t('settingsAutomation.model')}
                  </label>
                  <Input
                    id="settings-llm-model"
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
                  <label htmlFor="settings-llm-timeout" className="mb-1 block text-content-secondary text-xs">
                    {t('settingsAutomation.timeoutSecs')}
                  </label>
                  <Input
                    id="settings-llm-timeout"
                    type="number"
                    min={5}
                    max={300}
                    value={formData.ai_provider.llm_api?.timeout_secs ?? 30}
                    onChange={(e) =>
                      handleExternalApiChange('llm_api', 'timeout_secs', parseInt(e.target.value, 10) || 30)
                    }
                  />
                </div>
              </div>
              {modelCatalogNotice.llm_api && (
                <p className="text-content-secondary text-xs">{modelCatalogNotice.llm_api}</p>
              )}
            </div>
          )}
        </div>
      </Card>

      {/* Save button */}
      <div className="flex justify-end">
        <Button type="submit" variant="primary" size="lg" isLoading={mutation.isPending}>
          {mutation.isPending ? t('settings.saving') : t('settings.saveSettings')}
        </Button>
      </div>
    </form>
  )
}
