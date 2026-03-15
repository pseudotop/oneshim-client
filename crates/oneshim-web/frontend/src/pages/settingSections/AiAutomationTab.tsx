import { useTranslation } from 'react-i18next'
import type {
  AiProviderSettings,
  AutomationSettings,
  ExternalApiSettings,
  OcrValidationSettings as OcrValidationSettingsType,
  ProviderVendorSpec,
  SandboxSettings,
  SceneActionOverrideSettings as SceneActionOverrideSettingsType,
  SceneIntelligenceSettings as SceneIntelligenceSettingsType,
} from '../../api/client'
import { Button, Card, CardTitle, Input, Select } from '../../components/ui'
import { form } from '../../styles/tokens'
import OAuthConnectionPanel from './OAuthConnectionPanel'
import { isProviderOAuthAccessMode } from './oauth-panel-support'
import ToggleRow from './ToggleRow'
import type { SettingsFormTabProps } from './types'

function toDateTimeLocalValue(value: string | null | undefined): string {
  if (!value) return ''
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return ''
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`
}

function toRfc3339OrNull(value: string): string | null {
  if (!value.trim()) return null
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return null
  return date.toISOString()
}

function credentialBackendLabel(
  t: ReturnType<typeof useTranslation>['t'],
  backendKind: string | null | undefined,
): string {
  switch ((backendKind ?? '').trim()) {
    case 'os_secret_store':
      return t('settingsAutomation.backendOsSecretStore')
    case 'file_secret_store':
      return t('settingsAutomation.backendFileSecretStore')
    case 'env':
      return t('settingsAutomation.backendEnv')
    case 'bridge_managed':
      return t('settingsAutomation.backendBridgeManaged')
    case 'legacy_config':
      return t('settingsAutomation.backendLegacyConfig')
    default:
      return t('settingsAutomation.backendUnavailable')
  }
}

function apiKeyPlaceholder(
  t: ReturnType<typeof useTranslation>['t'],
  settings: ExternalApiSettings | null | undefined,
): string {
  if (!settings) {
    return t('settingsAutomation.apiKeyPlaceholder')
  }

  if (settings.secret_display_hint) {
    return settings.secret_display_hint
  }

  if (settings.has_secret) {
    return t('settingsAutomation.apiKeyStoredPlaceholder', {
      backend: credentialBackendLabel(t, settings.backend_kind),
    })
  }

  return t('settingsAutomation.apiKeyPlaceholder')
}

function shouldShowBackendManagedHint(settings: ExternalApiSettings | null | undefined): boolean {
  if (!settings?.has_secret) {
    return false
  }

  if (settings.api_key_masked.trim().length > 0) {
    return false
  }

  return settings.backend_kind !== 'legacy_config' && settings.backend_kind !== 'unavailable'
}

function supportsProjectionToggle(settings: ExternalApiSettings | null | undefined): boolean {
  if (!settings) {
    return false
  }

  return (
    settings.auth_mode === 'api_key' &&
    (settings.backend_kind === 'os_secret_store' || settings.backend_kind === 'file_secret_store')
  )
}

interface AiAutomationTabProps extends SettingsFormTabProps {
  providerOptions: ProviderVendorSpec[]
  modelCatalogNotice: Record<'ocr_api' | 'llm_api', string | null>
  modelCatalogLoading: 'ocr_api' | 'llm_api' | null
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
  resolveProviderType: (raw: string | null | undefined) => string
  onProviderTypeChange: (which: 'ocr_api' | 'llm_api', rawProviderType: string) => void
  onDiscoverModels: (which: 'ocr_api' | 'llm_api') => void
  getModelOptions: (which: 'ocr_api' | 'llm_api') => string[]
  canDiscoverModels: (which: 'ocr_api' | 'llm_api') => boolean
}

export default function AiAutomationTab({
  formData,
  providerOptions,
  modelCatalogNotice,
  modelCatalogLoading,
  onAutomationChange,
  onSandboxChange,
  onAiProviderChange,
  onOcrValidationChange,
  onSceneActionOverrideChange,
  onSceneIntelligenceChange,
  onExternalApiChange,
  resolveProviderType,
  onProviderTypeChange,
  onDiscoverModels,
  getModelOptions,
  canDiscoverModels,
}: AiAutomationTabProps) {
  const { t } = useTranslation()

  return (
    <div className="space-y-6">
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('settingsAutomation.title')}</CardTitle>
        <div className="space-y-4">
          <ToggleRow
            label={t('settingsAutomation.enabled')}
            description={t('settingsAutomation.enabledDescription')}
            checked={formData.automation.enabled}
            onChange={(value) => onAutomationChange('enabled', value)}
          />
        </div>
      </Card>

      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('settingsAutomation.sandboxTitle')}</CardTitle>
        <div className="space-y-4">
          <ToggleRow
            label={t('settingsAutomation.sandboxEnabled')}
            description={t('settingsAutomation.sandboxEnabledDescription')}
            checked={formData.sandbox.enabled}
            onChange={(value) => onSandboxChange('enabled', value)}
          />

          <div className={`space-y-4 ${!formData.sandbox.enabled ? 'pointer-events-none opacity-50' : ''}`}>
            <div>
              <label htmlFor="settings-sandbox-profile" className={form.label}>
                {t('settingsAutomation.sandboxProfile')}
              </label>
              <Select
                id="settings-sandbox-profile"
                value={formData.sandbox.profile}
                onChange={(e) => onSandboxChange('profile', e.target.value)}
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
              onChange={(value) => onSandboxChange('allow_network', value)}
            />
          </div>
        </div>
      </Card>

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
                onChange={(e) => onAiProviderChange('ocr_provider', e.target.value)}
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
                onChange={(e) => onAiProviderChange('llm_provider', e.target.value)}
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
              onChange={(e) => onAiProviderChange('external_data_policy', e.target.value)}
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
            onChange={(value) => onAiProviderChange('allow_unredacted_external_ocr', value)}
          />

          <div className="space-y-3 rounded-lg border border-muted p-4">
            <h4 className="font-medium text-content-strong text-sm">
              {t('settingsAutomation.sceneActionOverrideTitle')}
            </h4>
            <ToggleRow
              label={t('settingsAutomation.sceneActionOverrideEnabled')}
              description={t('settingsAutomation.sceneActionOverrideEnabledDescription')}
              checked={formData.ai_provider.scene_action_override.enabled}
              onChange={(value) => onSceneActionOverrideChange('enabled', value)}
            />
            <div
              className={`grid grid-cols-1 gap-3 md:grid-cols-2 ${!formData.ai_provider.scene_action_override.enabled ? 'pointer-events-none opacity-50' : ''}`}
            >
              <div className="md:col-span-2">
                <label htmlFor="settings-scene-override-reason" className="mb-1 block text-content-secondary text-xs">
                  {t('settingsAutomation.sceneActionOverrideReason')}
                </label>
                <Input
                  id="settings-scene-override-reason"
                  type="text"
                  value={formData.ai_provider.scene_action_override.reason}
                  onChange={(e) => onSceneActionOverrideChange('reason', e.target.value)}
                  placeholder={t('settingsAutomation.sceneActionOverrideReasonPlaceholder')}
                />
              </div>
              <div>
                <label
                  htmlFor="settings-scene-override-approved-by"
                  className="mb-1 block text-content-secondary text-xs"
                >
                  {t('settingsAutomation.sceneActionOverrideApprovedBy')}
                </label>
                <Input
                  id="settings-scene-override-approved-by"
                  type="text"
                  value={formData.ai_provider.scene_action_override.approved_by}
                  onChange={(e) => onSceneActionOverrideChange('approved_by', e.target.value)}
                  placeholder={t('settingsAutomation.sceneActionOverrideApprovedByPlaceholder')}
                />
              </div>
              <div>
                <label htmlFor="settings-scene-override-expires" className="mb-1 block text-content-secondary text-xs">
                  {t('settingsAutomation.sceneActionOverrideExpiresAt')}
                </label>
                <Input
                  id="settings-scene-override-expires"
                  type="datetime-local"
                  value={toDateTimeLocalValue(formData.ai_provider.scene_action_override.expires_at)}
                  onChange={(e) => onSceneActionOverrideChange('expires_at', toRfc3339OrNull(e.target.value))}
                />
              </div>
            </div>
          </div>

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
              onChange={(value) => onSceneIntelligenceChange('enabled', value)}
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
                onChange={(value) => onSceneIntelligenceChange('overlay_enabled', value)}
              />
              <ToggleRow
                label={t('settingsAutomation.sceneAllowExecution', 'Allow Scene Action Execution')}
                description={t(
                  'settingsAutomation.sceneAllowExecutionDescription',
                  'Permit direct click/type execution from scene coordinates (RPA gate).',
                )}
                checked={formData.ai_provider.scene_intelligence.allow_action_execution}
                onChange={(value) => onSceneIntelligenceChange('allow_action_execution', value)}
              />
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                <div>
                  <label htmlFor="settings-scene-min-confidence" className="mb-1 block text-content-secondary text-xs">
                    {t('settingsAutomation.sceneMinConfidence', 'Scene Min Confidence')}
                  </label>
                  <Input
                    id="settings-scene-min-confidence"
                    type="number"
                    min={0}
                    max={1}
                    step={0.05}
                    value={formData.ai_provider.scene_intelligence.min_confidence}
                    onChange={(e) => onSceneIntelligenceChange('min_confidence', Number(e.target.value))}
                  />
                </div>
                <div>
                  <label htmlFor="settings-scene-max-elements" className="mb-1 block text-content-secondary text-xs">
                    {t('settingsAutomation.sceneMaxElements', 'Scene Max Elements')}
                  </label>
                  <Input
                    id="settings-scene-max-elements"
                    type="number"
                    min={1}
                    max={1000}
                    step={1}
                    value={formData.ai_provider.scene_intelligence.max_elements}
                    onChange={(e) => onSceneIntelligenceChange('max_elements', Number(e.target.value))}
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
                  onChange={(value) => onSceneIntelligenceChange('calibration_enabled', value)}
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
                      onChange={(e) => onSceneIntelligenceChange('calibration_min_elements', Number(e.target.value))}
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
                        onSceneIntelligenceChange('calibration_min_avg_confidence', Number(e.target.value))
                      }
                    />
                  </div>
                </div>
              </div>
            </div>
          </div>

          <div className="space-y-3 rounded-lg border border-muted p-4">
            <h4 className="font-medium text-content-strong text-sm">{t('settingsAutomation.ocrValidationTitle')}</h4>
            <ToggleRow
              label={t('settingsAutomation.ocrValidationEnabled')}
              description={t('settingsAutomation.ocrValidationEnabledDescription')}
              checked={formData.ai_provider.ocr_validation.enabled}
              onChange={(value) => onOcrValidationChange('enabled', value)}
            />
            <div
              className={`grid grid-cols-1 gap-3 md:grid-cols-2 ${!formData.ai_provider.ocr_validation.enabled ? 'pointer-events-none opacity-50' : ''}`}
            >
              <div>
                <label htmlFor="settings-ocr-min-confidence" className="mb-1 block text-content-secondary text-xs">
                  {t('settingsAutomation.ocrMinConfidence')}
                </label>
                <Input
                  id="settings-ocr-min-confidence"
                  type="number"
                  min={0}
                  max={1}
                  step={0.05}
                  value={formData.ai_provider.ocr_validation.min_confidence}
                  onChange={(e) => onOcrValidationChange('min_confidence', Number(e.target.value))}
                />
              </div>
              <div>
                <label htmlFor="settings-ocr-max-invalid-ratio" className="mb-1 block text-content-secondary text-xs">
                  {t('settingsAutomation.ocrMaxInvalidRatio')}
                </label>
                <Input
                  id="settings-ocr-max-invalid-ratio"
                  type="number"
                  min={0}
                  max={1}
                  step={0.05}
                  value={formData.ai_provider.ocr_validation.max_invalid_ratio}
                  onChange={(e) => onOcrValidationChange('max_invalid_ratio', Number(e.target.value))}
                />
              </div>
            </div>
          </div>

          <ToggleRow
            label={t('settingsAutomation.fallbackToLocal')}
            description={t('settingsAutomation.fallbackToLocalDescription')}
            checked={formData.ai_provider.fallback_to_local}
            onChange={(value) => onAiProviderChange('fallback_to_local', value)}
          />

          {isProviderOAuthAccessMode(formData.ai_provider.access_mode) && (
            <OAuthConnectionPanel providerId="openai" providerName="OpenAI" />
          )}

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
                    onChange={(e) => onProviderTypeChange('ocr_api', e.target.value)}
                  >
                    {providerOptions.map((vendor) => (
                      <option key={vendor.provider_type} value={vendor.provider_type}>
                        {vendor.display_name}
                      </option>
                    ))}
                  </Select>
                </div>
                <div className="flex items-end">
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    disabled={!canDiscoverModels('ocr_api')}
                    isLoading={modelCatalogLoading === 'ocr_api'}
                    onClick={() => onDiscoverModels('ocr_api')}
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
                    onChange={(e) => onExternalApiChange('ocr_api', 'endpoint', e.target.value)}
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
                    onChange={(e) => onExternalApiChange('ocr_api', 'api_key_masked', e.target.value)}
                    placeholder={apiKeyPlaceholder(t, formData.ai_provider.ocr_api)}
                  />
                  {shouldShowBackendManagedHint(formData.ai_provider.ocr_api) && (
                    <p className="mt-1 text-content-secondary text-xs">
                      {t('settingsAutomation.apiKeyStoredHint', {
                        backend: credentialBackendLabel(t, formData.ai_provider.ocr_api?.backend_kind),
                      })}
                    </p>
                  )}
                </div>
                {supportsProjectionToggle(formData.ai_provider.ocr_api) && (
                  <div className="md:col-span-2">
                    <ToggleRow
                      label={t('settingsAutomation.secretProjectionEnabled')}
                      description={t('settingsAutomation.secretProjectionEnabledDescription')}
                      checked={formData.ai_provider.ocr_api?.projection_enabled ?? false}
                      onChange={(value) => onExternalApiChange('ocr_api', 'projection_enabled', value)}
                    />
                  </div>
                )}
                <div>
                  <label htmlFor="settings-ocr-model" className="mb-1 block text-content-secondary text-xs">
                    {t('settingsAutomation.model')}
                  </label>
                  <Input
                    id="settings-ocr-model"
                    type="text"
                    list="ocr-model-catalog"
                    value={formData.ai_provider.ocr_api?.model ?? ''}
                    onChange={(e) => onExternalApiChange('ocr_api', 'model', e.target.value || null)}
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
                    onChange={(e) => onExternalApiChange('ocr_api', 'timeout_secs', parseInt(e.target.value, 10) || 30)}
                  />
                </div>
              </div>
              {modelCatalogNotice.ocr_api && (
                <p className="text-content-secondary text-xs">{modelCatalogNotice.ocr_api}</p>
              )}
              {!canDiscoverModels('ocr_api') && !modelCatalogNotice.ocr_api && (
                <p className="text-content-secondary text-xs">
                  {t('settingsAutomation.modelDiscoveryUnsupportedSurface')}
                </p>
              )}
            </div>
          )}

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
                    onChange={(e) => onProviderTypeChange('llm_api', e.target.value)}
                  >
                    {providerOptions.map((vendor) => (
                      <option key={vendor.provider_type} value={vendor.provider_type}>
                        {vendor.display_name}
                      </option>
                    ))}
                  </Select>
                </div>
                <div className="flex items-end">
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    disabled={!canDiscoverModels('llm_api')}
                    isLoading={modelCatalogLoading === 'llm_api'}
                    onClick={() => onDiscoverModels('llm_api')}
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
                    onChange={(e) => onExternalApiChange('llm_api', 'endpoint', e.target.value)}
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
                    onChange={(e) => onExternalApiChange('llm_api', 'api_key_masked', e.target.value)}
                    placeholder={apiKeyPlaceholder(t, formData.ai_provider.llm_api)}
                  />
                  {shouldShowBackendManagedHint(formData.ai_provider.llm_api) && (
                    <p className="mt-1 text-content-secondary text-xs">
                      {t('settingsAutomation.apiKeyStoredHint', {
                        backend: credentialBackendLabel(t, formData.ai_provider.llm_api?.backend_kind),
                      })}
                    </p>
                  )}
                </div>
                {supportsProjectionToggle(formData.ai_provider.llm_api) && (
                  <div className="md:col-span-2">
                    <ToggleRow
                      label={t('settingsAutomation.secretProjectionEnabled')}
                      description={t('settingsAutomation.secretProjectionEnabledDescription')}
                      checked={formData.ai_provider.llm_api?.projection_enabled ?? false}
                      onChange={(value) => onExternalApiChange('llm_api', 'projection_enabled', value)}
                    />
                  </div>
                )}
                <div>
                  <label htmlFor="settings-llm-model" className="mb-1 block text-content-secondary text-xs">
                    {t('settingsAutomation.model')}
                  </label>
                  <Input
                    id="settings-llm-model"
                    type="text"
                    list="llm-model-catalog"
                    value={formData.ai_provider.llm_api?.model ?? ''}
                    onChange={(e) => onExternalApiChange('llm_api', 'model', e.target.value || null)}
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
                    onChange={(e) => onExternalApiChange('llm_api', 'timeout_secs', parseInt(e.target.value, 10) || 30)}
                  />
                </div>
              </div>
              {modelCatalogNotice.llm_api && (
                <p className="text-content-secondary text-xs">{modelCatalogNotice.llm_api}</p>
              )}
              {!canDiscoverModels('llm_api') && !modelCatalogNotice.llm_api && (
                <p className="text-content-secondary text-xs">
                  {t('settingsAutomation.modelDiscoveryUnsupportedSurface')}
                </p>
              )}
            </div>
          )}
        </div>
      </Card>
    </div>
  )
}
