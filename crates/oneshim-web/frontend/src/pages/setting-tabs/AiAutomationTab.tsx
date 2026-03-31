import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
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
} from '../../api/client'
import { Badge, Button, Card, CardTitle, Input, Select } from '../../components/ui'
import {
  findFeatureCapability,
  maturityBadgeColor,
  providerSurfaceAvailability,
  providerSurfaceMaturity,
  providerSurfaceStatusCopyKey,
} from '../../features/featureCapabilities'
import {
  defaultSurfaceEndpoint,
  type EndpointSurfaceKind,
  preferredRelatedProviderSurfaceFromList,
  surfaceModelSupportsCapability,
  surfaceOcrExecutionStrategy,
  surfaceSupportsModelSelection,
} from '../../features/providerSurfaces'
import { form, typography } from '../../styles/tokens'
import {
  apiKeyPlaceholder,
  credentialBackendLabel,
  executionKindLabel,
  placementKindDescription,
  placementKindLabel,
  requirementLabel,
  shouldShowBackendManagedHint,
  supportsProjectionToggle,
  surfaceUsesNoAuth,
  toDateTimeLocalValue,
  toRfc3339OrNull,
} from './ai-automation-utils'
import OAuthConnectionPanel from './OAuthConnectionPanel'
import { isProviderOAuthAccessMode } from './oauth-panel-support'
import ProviderWizard, { type ProviderDef } from './ProviderWizard'
import ToggleRow from './ToggleRow'
import type { SettingsFormTabProps } from './types'

interface AiAutomationTabProps extends SettingsFormTabProps {
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

export default function AiAutomationTab({
  formData,
  allProviderSurfaces,
  providerSurfaceOptions,
  featureCapabilities,
  secretBackendCapabilities,
  modelCatalogNotice,
  modelCompatibilityNotice,
  modelCatalogLoading,
  endpointProbeResult,
  endpointProbeLoading,
  onAutomationChange,
  onSandboxChange,
  onAiProviderChange,
  onOcrValidationChange,
  onSceneActionOverrideChange,
  onSceneIntelligenceChange,
  onExternalApiChange,
  resolveProviderSurface,
  onProviderSurfaceChange,
  onSelectAiProviderProfile,
  onSaveAiProviderProfile,
  onDeleteAiProviderProfile,
  onDiscoverModels,
  getModelOptions,
  canDiscoverModels,
}: AiAutomationTabProps) {
  const { t } = useTranslation()
  const savedProfiles = formData.ai_provider.saved_profiles ?? []
  const activeSavedProfile =
    savedProfiles.find((profile) => profile.profile_id === formData.ai_provider.active_profile_id) ?? null
  const [profileNameDraft, setProfileNameDraft] = useState('')
  const currentOcrSurface = resolveProviderSurface('ocr_api')
  const currentLlmSurface = resolveProviderSurface('llm_api')
  const isCliAccessMode = formData.ai_provider.access_mode === 'ProviderSubscriptionCli'
  const isOAuthAccessMode = isProviderOAuthAccessMode(formData.ai_provider.access_mode)
  const showOcrRemoteSection = formData.ai_provider.ocr_provider === 'Remote'
  const showLlmSurfaceSection = formData.ai_provider.llm_provider === 'Remote' || isCliAccessMode
  const currentLlmFeature = currentLlmSurface
    ? findFeatureCapability(featureCapabilities, currentLlmSurface.surface_id)
    : null
  const currentLlmMaturity = providerSurfaceMaturity(currentLlmSurface, featureCapabilities)
  const currentLlmRequirements = currentLlmFeature?.requires ?? []
  const oauthPanels = [
    { endpointKind: 'llm_api' as const, surface: currentLlmSurface },
    { endpointKind: 'ocr_api' as const, surface: currentOcrSurface },
  ].flatMap((entry) => {
    if (!entry.surface || entry.surface.execution_kind !== 'managed_http') {
      return []
    }

    const preferredCliSurface = preferredRelatedProviderSurfaceFromList(
      allProviderSurfaces,
      entry.surface,
      'subprocess_cli',
      featureCapabilities,
    )
    const preferredCliAvailability = providerSurfaceAvailability(preferredCliSurface, featureCapabilities)
    return [
      {
        endpointKind: entry.endpointKind,
        oauthSurface: entry.surface,
        preferredCliSurface,
        showPreferredCliCta:
          Boolean(preferredCliSurface) &&
          preferredCliAvailability !== 'unavailable' &&
          entry.surface.surface_id !== preferredCliSurface?.surface_id,
      },
    ]
  })
  const currentLlmPreferredCli = oauthPanels.find((entry) => entry.endpointKind === 'llm_api')
  const currentOcrUsesNoAuth = surfaceUsesNoAuth(currentOcrSurface, 'ocr_api')
  const currentLlmUsesNoAuth = surfaceUsesNoAuth(currentLlmSurface, 'llm_api')
  const currentOcrSupportsModelSelection = surfaceSupportsModelSelection(currentOcrSurface, 'ocr_api')
  const currentLlmSupportsModelSelection = surfaceSupportsModelSelection(currentLlmSurface, 'llm_api')
  const currentOcrModelSupport = surfaceModelSupportsCapability(
    currentOcrSurface,
    'ocr_api',
    formData.ai_provider.ocr_api?.model,
  )
  const currentOcrStrategy = surfaceOcrExecutionStrategy(currentOcrSurface)
  const currentLlmModelSupport = surfaceModelSupportsCapability(
    currentLlmSurface,
    'llm_api',
    formData.ai_provider.llm_api?.model,
  )

  const usesCustomSelfHostedEndpoint = (
    surface: ProviderSurfaceSpec | undefined,
    endpointKind: EndpointSurfaceKind,
  ): boolean => {
    if (!surface || surface.execution_kind !== 'direct_http' || surface.placement_kind !== 'self_hosted') {
      return false
    }

    const configuredEndpoint =
      endpointKind === 'ocr_api'
        ? formData.ai_provider.ocr_api?.endpoint?.trim()
        : formData.ai_provider.llm_api?.endpoint?.trim()
    const catalogEndpoint = defaultSurfaceEndpoint(surface, endpointKind).trim()

    return Boolean(configuredEndpoint && catalogEndpoint && configuredEndpoint !== catalogEndpoint)
  }

  const surfaceAvailabilityForEndpoint = (
    surface: ProviderSurfaceSpec | undefined,
    endpointKind: EndpointSurfaceKind,
  ) => {
    const endpointProbe = endpointProbeResult[endpointKind]
    if (endpointProbe) {
      return endpointProbe.availability
    }

    return usesCustomSelfHostedEndpoint(surface, endpointKind)
      ? 'partially_available'
      : providerSurfaceAvailability(surface, featureCapabilities)
  }

  const surfaceStatusCopyKeyForEndpoint = (
    surface: ProviderSurfaceSpec | undefined,
    endpointKind: EndpointSurfaceKind,
  ) => {
    const endpointProbe = endpointProbeResult[endpointKind]
    if (endpointProbe) {
      return endpointProbe.status_copy_key
    }

    return usesCustomSelfHostedEndpoint(surface, endpointKind)
      ? null
      : providerSurfaceStatusCopyKey(surface, featureCapabilities)
  }

  const currentLlmAvailability = surfaceAvailabilityForEndpoint(currentLlmSurface, 'llm_api')
  const currentLlmStatusCopyKey = surfaceStatusCopyKeyForEndpoint(currentLlmSurface, 'llm_api')
  const llmProviderLocked = isOAuthAccessMode || isCliAccessMode
  const llmProviderLockReason = isCliAccessMode
    ? t('settingsAutomation.providerSelectionLockedCli')
    : isOAuthAccessMode
      ? t('settingsAutomation.providerSelectionLockedOAuth')
      : null
  const effectiveLlmProvider = isCliAccessMode ? 'Remote' : formData.ai_provider.llm_provider
  const activeOcrPathSummary =
    formData.ai_provider.ocr_provider === 'Remote' && currentOcrSurface
      ? t('settingsAutomation.activePathRemoteSurface', { surface: currentOcrSurface.display_name })
      : formData.ai_provider.ocr_provider === 'Remote'
        ? t('settingsAutomation.activePathRemote')
        : t('settingsAutomation.activePathLocal')
  const activeLlmPathSummary =
    effectiveLlmProvider === 'Remote' && currentLlmSurface
      ? t('settingsAutomation.activePathRemoteSurface', { surface: currentLlmSurface.display_name })
      : effectiveLlmProvider === 'Remote'
        ? t('settingsAutomation.activePathRemote')
        : t('settingsAutomation.activePathLocal')

  useEffect(() => {
    setProfileNameDraft(activeSavedProfile?.name ?? '')
  }, [activeSavedProfile?.name])

  const handleSwitchToPreferredCli = (
    endpointKind: EndpointSurfaceKind,
    preferredCliSurface: ProviderSurfaceSpec | undefined,
  ) => {
    onAiProviderChange('access_mode', 'ProviderSubscriptionCli')
    if (preferredCliSurface) {
      onProviderSurfaceChange(endpointKind, preferredCliSurface.surface_id)
    }
  }

  const accessModeOptions = [
    {
      value: 'ProviderApiKey',
      label: t('settingsAutomation.accessModeApiKeyLabel'),
      description: t('settingsAutomation.accessModeApiKeyDescription'),
      maturity: 'stable' as const,
      preferred: false,
    },
    {
      value: 'ProviderSubscriptionCli',
      label: t('settingsAutomation.accessModeCliLabel'),
      description: t('settingsAutomation.accessModeCliDescription'),
      maturity: 'beta' as const,
      preferred: true,
    },
    {
      value: 'ProviderOAuth',
      label: t('settingsAutomation.accessModeOAuthLabel'),
      description: t('settingsAutomation.accessModeOAuthDescription'),
      maturity: 'experimental' as const,
      preferred: false,
    },
  ]

  const currentAccessModeOption =
    accessModeOptions.find((option) => option.value === formData.ai_provider.access_mode) ?? accessModeOptions[0]

  const formatSurfaceOptionLabel = (surface: ProviderSurfaceSpec): string => {
    const labels = [surface.display_name, placementKindLabel(t, surface.placement_kind)]
    if (surface.preferred_for_product_auth) {
      labels.push(t('featureCapability.preferredPath'))
    }

    const maturity = providerSurfaceMaturity(surface, featureCapabilities)
    if (maturity !== 'stable') {
      labels.push(t(`featureCapability.maturity.${maturity}`))
    }

    const availability = providerSurfaceAvailability(surface, featureCapabilities)
    if (availability !== 'available') {
      labels.push(t(`featureCapability.availability.${availability}`))
    }

    return labels.join(' · ')
  }

  const renderSurfaceStatus = (surface: ProviderSurfaceSpec | undefined, endpointKind: EndpointSurfaceKind) => {
    if (!surface) {
      return null
    }

    const feature = findFeatureCapability(featureCapabilities, surface.surface_id)
    const maturity = providerSurfaceMaturity(surface, featureCapabilities)
    const customSelfHostedEndpoint = usesCustomSelfHostedEndpoint(surface, endpointKind)
    const endpointProbe = endpointProbeResult[endpointKind]
    const availability = surfaceAvailabilityForEndpoint(surface, endpointKind)
    const statusCopyKey = surfaceStatusCopyKeyForEndpoint(surface, endpointKind)
    const setupCopyKey = endpointProbe ? null : (feature?.setup_copy_key ?? null)
    const setupDocsUrl = endpointProbe ? null : (feature?.setup_docs_url ?? null)
    const setupEnvVars = endpointProbe ? [] : (feature?.configuration_env_vars ?? [])

    return (
      <div className="space-y-2 rounded-lg border border-muted bg-surface-muted/80 p-3">
        <div className="flex flex-wrap items-center gap-2">
          <Badge color="default" size="sm">
            {placementKindLabel(t, surface.placement_kind)}
          </Badge>
          <Badge color={maturityBadgeColor(maturity)} size="sm">
            {t(`featureCapability.maturity.${maturity}`)}
          </Badge>
          {surface.preferred_for_product_auth && (
            <Badge color="info" size="sm">
              {t('featureCapability.preferredPath')}
            </Badge>
          )}
          <span className="text-content-secondary text-xs">{t(`featureCapability.availability.${availability}`)}</span>
        </div>
        {statusCopyKey && <p className="text-content-secondary text-xs">{t(statusCopyKey)}</p>}
        {customSelfHostedEndpoint && !endpointProbe && !endpointProbeLoading[endpointKind] && (
          <p className="text-content-secondary text-xs">{t('settingsAutomation.selfHostedCustomEndpointStatus')}</p>
        )}
        {setupCopyKey && (
          <div className="space-y-2 rounded-md border border-muted bg-surface-elevated/70 p-3">
            <p className="text-content-muted text-xs">{t('featureCapability.setupTitle')}</p>
            <p className="text-content-secondary text-xs">{t(setupCopyKey)}</p>
            {setupEnvVars.length > 0 && (
              <div className="space-y-1">
                <p className="text-content-muted text-xs">{t('featureCapability.setupEnvVars')}</p>
                <div className="flex flex-wrap gap-2">
                  {setupEnvVars.map((envVar) => (
                    <Badge key={envVar} color="default" size="sm">
                      <code>{envVar}</code>
                    </Badge>
                  ))}
                </div>
              </div>
            )}
            {setupDocsUrl && (
              <a
                href={setupDocsUrl}
                target="_blank"
                rel="noreferrer"
                className="inline-flex text-accent text-xs underline"
              >
                {t('featureCapability.openSetupDocs')}
              </a>
            )}
          </div>
        )}
      </div>
    )
  }

  const showDirectApiFields = (surface: ProviderSurfaceSpec | undefined): boolean =>
    (surface?.execution_kind ?? 'direct_http') === 'direct_http'

  const showManagedHttpFields = (surface: ProviderSurfaceSpec | undefined): boolean =>
    surface?.execution_kind === 'managed_http'

  const showSubprocessFields = (surface: ProviderSurfaceSpec | undefined): boolean =>
    surface?.execution_kind === 'subprocess_cli'

  const handleSavedProfileSelection = (profileId: string) => {
    const nextProfile = savedProfiles.find((profile) => profile.profile_id === profileId) ?? null
    if (nextProfile) {
      setProfileNameDraft(nextProfile.name)
    }
    onSelectAiProviderProfile(profileId || null)
  }

  const handleSaveCurrentProfile = () => {
    const nextName = profileNameDraft.trim() || activeSavedProfile?.name || ''
    if (!nextName) {
      return
    }
    setProfileNameDraft(nextName)
    onSaveAiProviderProfile(nextName)
  }

  const handleDeleteCurrentProfile = () => {
    if (!activeSavedProfile) {
      return
    }
    setProfileNameDraft('')
    onDeleteAiProviderProfile(activeSavedProfile.profile_id)
  }

  const saveProfileDisabled = !profileNameDraft.trim() && !activeSavedProfile?.name

  const handleProviderWizardSelect = (provider: ProviderDef, apiKey: string) => {
    onAiProviderChange('access_mode', 'ProviderApiKey')
    onAiProviderChange('llm_provider', provider.tier === 'local' ? 'Local' : 'Remote')
    onProviderSurfaceChange('llm_api', provider.surfaceId)
    if (apiKey) {
      onExternalApiChange('llm_api', 'api_key_masked', apiKey)
    }
    onExternalApiChange('llm_api', 'model', provider.defaultModel)
  }

  return (
    <div className="space-y-6">
      <ProviderWizard onSelect={handleProviderWizardSelect} />

      <Card variant="default" padding="lg">
        <CardTitle sticky>{t('settingsAutomation.title')}</CardTitle>
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
        <CardTitle sticky>{t('settingsAutomation.sandboxTitle')}</CardTitle>
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
                <option value="Permissive">{t('settingsAutomation.sandboxProfilePermissive')}</option>
                <option value="Standard">{t('settingsAutomation.sandboxProfileStandard')}</option>
                <option value="Strict">{t('settingsAutomation.sandboxProfileStrict')}</option>
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
        <CardTitle sticky>{t('settingsAutomation.aiTitle')}</CardTitle>
        <div className="space-y-4">
          <div className="space-y-3 rounded-lg border border-muted p-4">
            <div>
              <label htmlFor="settings-ai-access-mode" className={form.label}>
                {t('settingsAutomation.accessModeLabel')}
              </label>
              <Select
                id="settings-ai-access-mode"
                value={formData.ai_provider.access_mode}
                onChange={(e) => onAiProviderChange('access_mode', e.target.value)}
              >
                {accessModeOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
                {!accessModeOptions.some((option) => option.value === formData.ai_provider.access_mode) && (
                  <option value={formData.ai_provider.access_mode}>{currentAccessModeOption.label}</option>
                )}
              </Select>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <Badge color={maturityBadgeColor(currentAccessModeOption.maturity)} size="sm">
                {t(`featureCapability.maturity.${currentAccessModeOption.maturity}`)}
              </Badge>
              {currentAccessModeOption.preferred && (
                <Badge color="info" size="sm">
                  {t('featureCapability.preferredPath')}
                </Badge>
              )}
            </div>
            <p className="text-content-secondary text-sm">{currentAccessModeOption.description}</p>
          </div>

          <div className="space-y-3 rounded-lg border border-muted p-4">
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div className="space-y-1">
                <p className={`${typography.weight.medium} text-content-strong text-sm`}>
                  {t('settingsAutomation.savedProfilesTitle')}
                </p>
                <p className="text-content-secondary text-sm">{t('settingsAutomation.savedProfilesDescription')}</p>
              </div>
              {activeSavedProfile ? (
                <Badge color="info" size="sm">
                  {t('settingsAutomation.savedProfilesActiveBadge')}
                </Badge>
              ) : null}
            </div>

            <div>
              <label htmlFor="settings-ai-provider-profile" className={form.label}>
                {t('settingsAutomation.savedProfilesSelectLabel')}
              </label>
              <Select
                id="settings-ai-provider-profile"
                value={formData.ai_provider.active_profile_id ?? ''}
                onChange={(e) => handleSavedProfileSelection(e.target.value)}
              >
                <option value="">{t('settingsAutomation.savedProfilesCustomOption')}</option>
                {savedProfiles.map((profile) => (
                  <option key={profile.profile_id} value={profile.profile_id}>
                    {profile.name}
                  </option>
                ))}
              </Select>
            </div>

            <div className="grid grid-cols-1 gap-3 md:grid-cols-[minmax(0,1fr)_auto_auto]">
              <div>
                <label htmlFor="settings-ai-provider-profile-name" className={form.label}>
                  {t('settingsAutomation.savedProfilesNameLabel')}
                </label>
                <Input
                  id="settings-ai-provider-profile-name"
                  type="text"
                  value={profileNameDraft}
                  onChange={(e) => setProfileNameDraft(e.target.value)}
                  placeholder={t('settingsAutomation.savedProfilesNamePlaceholder')}
                />
              </div>
              <div className="flex items-end">
                <Button
                  type="button"
                  variant="secondary"
                  onClick={handleSaveCurrentProfile}
                  disabled={saveProfileDisabled}
                >
                  {activeSavedProfile
                    ? t('settingsAutomation.savedProfilesUpdateAction')
                    : t('settingsAutomation.savedProfilesSaveAction')}
                </Button>
              </div>
              <div className="flex items-end">
                <Button
                  type="button"
                  variant="danger"
                  onClick={handleDeleteCurrentProfile}
                  disabled={!activeSavedProfile}
                >
                  {t('settingsAutomation.savedProfilesDeleteAction')}
                </Button>
              </div>
            </div>

            <p className="text-content-secondary text-xs">
              {activeSavedProfile
                ? t('settingsAutomation.savedProfilesSelectedHint', { name: activeSavedProfile.name })
                : t('settingsAutomation.savedProfilesCustomHint')}
            </p>
          </div>

          {showOcrRemoteSection && currentOcrSurface && (
            <div className="space-y-3 rounded-lg border border-muted bg-surface-muted/80 p-4">
              <div className="space-y-1">
                <p className={`${typography.weight.medium} text-content-strong text-sm`}>
                  OCR {t('settingsAutomation.providerSurfaceLabel')}
                </p>
                <p className="text-content-secondary text-sm">
                  {t('settingsAutomation.pathSummarySurface', { surface: currentOcrSurface.display_name })}
                </p>
              </div>

              <div>
                <label htmlFor="settings-ocr-provider-surface" className="mb-1 block text-content-secondary text-xs">
                  {t('settingsAutomation.providerSurfaceLabel')}
                </label>
                <Select
                  id="settings-ocr-provider-surface"
                  value={
                    formData.ai_provider.ocr_api?.surface_id ?? resolveProviderSurface('ocr_api')?.surface_id ?? ''
                  }
                  disabled={providerSurfaceOptions.ocr_api.length === 0}
                  onChange={(e) => onProviderSurfaceChange('ocr_api', e.target.value)}
                >
                  {providerSurfaceOptions.ocr_api.length === 0 ? (
                    <option value="">{t('settingsAutomation.noCompatibleProviderSurface')}</option>
                  ) : (
                    providerSurfaceOptions.ocr_api.map((surface) => (
                      <option key={surface.surface_id} value={surface.surface_id}>
                        {formatSurfaceOptionLabel(surface)}
                      </option>
                    ))
                  )}
                </Select>
              </div>

              {renderSurfaceStatus(currentOcrSurface, 'ocr_api')}

              {currentOcrStrategy !== 'none' && (
                <div className="rounded-md bg-surface-elevated/70 p-3 text-content-secondary text-xs">
                  {currentOcrStrategy === 'multimodal_llm'
                    ? t('settingsAutomation.ocrStrategyMultimodal')
                    : t('settingsAutomation.ocrStrategyVisionApi')}
                </div>
              )}
            </div>
          )}

          {showLlmSurfaceSection && currentLlmSurface && (
            <div className="space-y-3 rounded-lg border border-muted bg-surface-muted/80 p-4">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="space-y-1">
                  <p className={`${typography.weight.medium} text-content-strong text-sm`}>
                    LLM {t('settingsAutomation.providerSurfaceLabel')}
                  </p>
                  <p className="text-content-secondary text-sm">
                    {t('settingsAutomation.pathSummarySurface', { surface: currentLlmSurface.display_name })}
                  </p>
                </div>
                <div className="flex flex-wrap items-center gap-2">
                  <Badge color={maturityBadgeColor(currentLlmMaturity)} size="sm">
                    {t(`featureCapability.maturity.${currentLlmMaturity}`)}
                  </Badge>
                  {currentLlmSurface.preferred_for_product_auth && (
                    <Badge color="info" size="sm">
                      {t('featureCapability.preferredPath')}
                    </Badge>
                  )}
                  <Badge color={currentLlmAvailability === 'available' ? 'success' : 'warning'} size="sm">
                    {t(`featureCapability.availability.${currentLlmAvailability}`)}
                  </Badge>
                </div>
              </div>

              <div>
                <label htmlFor="settings-llm-provider-surface" className="mb-1 block text-content-secondary text-xs">
                  {t('settingsAutomation.providerSurfaceLabel')}
                </label>
                <Select
                  id="settings-llm-provider-surface"
                  value={
                    formData.ai_provider.llm_api?.surface_id ?? resolveProviderSurface('llm_api')?.surface_id ?? ''
                  }
                  disabled={providerSurfaceOptions.llm_api.length === 0}
                  onChange={(e) => onProviderSurfaceChange('llm_api', e.target.value)}
                >
                  {providerSurfaceOptions.llm_api.length === 0 ? (
                    <option value="">{t('settingsAutomation.noCompatibleProviderSurface')}</option>
                  ) : (
                    providerSurfaceOptions.llm_api.map((surface) => (
                      <option key={surface.surface_id} value={surface.surface_id}>
                        {formatSurfaceOptionLabel(surface)}
                      </option>
                    ))
                  )}
                </Select>
              </div>

              <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
                <div className="space-y-1">
                  <p className="text-content-muted text-xs">{t('settingsAutomation.pathExecutionLabel')}</p>
                  <p className="text-content-secondary text-sm">
                    {executionKindLabel(t, currentLlmSurface.execution_kind)}
                  </p>
                </div>
                <div className="space-y-1">
                  <p className="text-content-muted text-xs">{t('settingsAutomation.pathPlacementLabel')}</p>
                  <p className="text-content-secondary text-sm">
                    {placementKindLabel(t, currentLlmSurface.placement_kind)}
                  </p>
                  <p className="text-content-muted text-xs">
                    {placementKindDescription(t, currentLlmSurface.placement_kind)}
                  </p>
                </div>
                <div className="space-y-1">
                  <p className="text-content-muted text-xs">{t('settingsAutomation.pathRequirementsLabel')}</p>
                  <div className="flex flex-wrap gap-2">
                    {currentLlmRequirements.length > 0 ? (
                      currentLlmRequirements.map((requirement) => (
                        <Badge key={requirement} color="default" size="sm">
                          {requirementLabel(t, requirement)}
                        </Badge>
                      ))
                    ) : (
                      <span className="text-content-secondary text-sm">
                        {t('settingsAutomation.pathRequirementsNone')}
                      </span>
                    )}
                  </div>
                </div>
              </div>

              {currentLlmStatusCopyKey && (
                <div className="space-y-1">
                  <p className="text-content-muted text-xs">{t('settingsAutomation.pathNextStepLabel')}</p>
                  <p className="text-content-secondary text-sm">{t(currentLlmStatusCopyKey)}</p>
                </div>
              )}

              {currentLlmPreferredCli?.showPreferredCliCta && currentLlmPreferredCli.preferredCliSurface && (
                <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-muted bg-surface-elevated/70 p-3">
                  <div className="space-y-1">
                    <p className={`${typography.weight.medium} text-content-strong text-sm`}>
                      {t('settingsAutomation.preferredCliTitle')}
                    </p>
                    <p className="text-content-secondary text-sm">
                      {t('settingsAutomation.preferredCliDescription', {
                        surface: currentLlmPreferredCli.preferredCliSurface.display_name,
                      })}
                    </p>
                  </div>
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    onClick={() =>
                      handleSwitchToPreferredCli(
                        currentLlmPreferredCli.endpointKind,
                        currentLlmPreferredCli.preferredCliSurface,
                      )
                    }
                  >
                    {t('settingsAutomation.switchToPreferredCli')}
                  </Button>
                </div>
              )}
            </div>
          )}

          <div className="space-y-3 rounded-lg border border-muted bg-surface-muted/80 p-4">
            <div className="space-y-1">
              <p className={`${typography.weight.medium} text-content-strong text-sm`}>
                {t('settingsAutomation.activeRoutingTitle')}
              </p>
              <p className="text-content-secondary text-sm">{t('settingsAutomation.activeRoutingDescription')}</p>
            </div>
            <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
              <div className="space-y-2 rounded-md border border-muted/70 bg-surface-elevated/70 p-3">
                <div className="flex items-center justify-between gap-2">
                  <span className={`${typography.weight.medium} text-content text-sm`}>
                    {t('settingsAutomation.ocrProvider')}
                  </span>
                  <Badge color={formData.ai_provider.ocr_provider === 'Remote' ? 'info' : 'default'} size="sm">
                    {formData.ai_provider.ocr_provider === 'Remote'
                      ? t('settingsAutomation.providerRemote')
                      : t('settingsAutomation.providerLocal')}
                  </Badge>
                </div>
                <p className="text-content-secondary text-sm">{activeOcrPathSummary}</p>
              </div>
              <div className="space-y-2 rounded-md border border-muted/70 bg-surface-elevated/70 p-3">
                <div className="flex items-center justify-between gap-2">
                  <div className="flex items-center gap-2">
                    <span className={`${typography.weight.medium} text-content text-sm`}>
                      {t('settingsAutomation.llmProvider')}
                    </span>
                    {llmProviderLocked && (
                      <Badge color="warning" size="sm">
                        {t('settingsAutomation.providerSelectionLockedBadge')}
                      </Badge>
                    )}
                  </div>
                  <Badge color={effectiveLlmProvider === 'Remote' ? 'info' : 'default'} size="sm">
                    {effectiveLlmProvider === 'Remote'
                      ? t('settingsAutomation.providerRemote')
                      : t('settingsAutomation.providerLocal')}
                  </Badge>
                </div>
                <p className="text-content-secondary text-sm">{activeLlmPathSummary}</p>
                {llmProviderLocked && llmProviderLockReason && (
                  <p className="text-content-muted text-xs">{llmProviderLockReason}</p>
                )}
              </div>
            </div>
          </div>

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
                <option value="Local">{t('settingsAutomation.providerLocal')}</option>
                <option value="Remote">{t('settingsAutomation.providerRemote')}</option>
              </Select>
            </div>
            <div>
              <label htmlFor="settings-llm-provider" className={form.label}>
                {t('settingsAutomation.llmProvider')}
              </label>
              {llmProviderLocked && (
                <div className="mb-2 flex items-center gap-2">
                  <Badge color="warning" size="sm">
                    {t('settingsAutomation.providerSelectionLockedBadge')}
                  </Badge>
                  <span className="text-content-secondary text-xs">{llmProviderLockReason}</span>
                </div>
              )}
              <Select
                id="settings-llm-provider"
                value={isCliAccessMode ? 'Remote' : formData.ai_provider.llm_provider}
                disabled={llmProviderLocked}
                onChange={(e) => onAiProviderChange('llm_provider', e.target.value)}
              >
                <option value="Local">{t('settingsAutomation.providerLocal')}</option>
                <option value="Remote">{t('settingsAutomation.providerRemote')}</option>
              </Select>
              {llmProviderLocked && (
                <p className="mt-1 text-content-secondary text-xs">
                  {isCliAccessMode
                    ? t('settingsAutomation.cliModeProviderSummary')
                    : t('settingsAutomation.oauthLlmProviderPinned')}
                </p>
              )}
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
              <option value="PiiFilterStrict">{t('settingsAutomation.dataPolicyStrict')}</option>
              <option value="PiiFilterStandard">{t('settingsAutomation.dataPolicyStandard')}</option>
              <option value="AllowFiltered">{t('settingsAutomation.dataPolicyAllowFiltered')}</option>
            </Select>
          </div>

          <ToggleRow
            label={t('settingsAutomation.allowUnredactedExternalOcr')}
            description={t('settingsAutomation.allowUnredactedExternalOcrDescription')}
            checked={formData.ai_provider.allow_unredacted_external_ocr}
            onChange={(value) => onAiProviderChange('allow_unredacted_external_ocr', value)}
          />

          <div className="space-y-3 rounded-lg border border-muted p-4">
            <h4 className={`${typography.weight.medium} text-content-strong text-sm`}>
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
            <h4 className={`${typography.weight.medium} text-content-strong text-sm`}>
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
            <h4 className={`${typography.weight.medium} text-content-strong text-sm`}>
              {t('settingsAutomation.ocrValidationTitle')}
            </h4>
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

          {isOAuthAccessMode &&
            oauthPanels.map((panel) => (
              <OAuthConnectionPanel
                key={`${panel.endpointKind}:${panel.oauthSurface.surface_id}`}
                providerId={panel.oauthSurface.vendor_id}
                providerName={
                  panel.oauthSurface.provider_type === 'OpenAi' ? 'OpenAI' : panel.oauthSurface.display_name
                }
                oauthSurface={panel.oauthSurface}
                preferredCliSurface={panel.preferredCliSurface}
                featureSnapshot={featureCapabilities}
                secretBackendCapabilities={secretBackendCapabilities}
                onUsePreferredCli={
                  panel.showPreferredCliCta
                    ? () => handleSwitchToPreferredCli(panel.endpointKind, panel.preferredCliSurface)
                    : undefined
                }
              />
            ))}

          {showOcrRemoteSection && (
            <div className="space-y-3 rounded-lg border border-muted p-4">
              <h4 className={`${typography.weight.medium} text-content-strong text-sm`}>
                OCR {t('settingsAutomation.externalApi')}
              </h4>
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
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
                {showDirectApiFields(currentOcrSurface) && (
                  <>
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
                    {!currentOcrUsesNoAuth ? (
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
                    ) : (
                      <div className="rounded-md bg-surface-muted/80 p-3 text-content-secondary text-xs md:col-span-2">
                        {placementKindDescription(t, currentOcrSurface?.placement_kind)}
                      </div>
                    )}
                    {currentOcrUsesNoAuth && (
                      <div className="rounded-md bg-surface-elevated/70 p-3 text-content-secondary text-xs md:col-span-2">
                        {t('settingsAutomation.noAuthSurfaceDescription')}
                      </div>
                    )}
                    {!currentOcrUsesNoAuth && supportsProjectionToggle(formData.ai_provider.ocr_api) && (
                      <div className="md:col-span-2">
                        <ToggleRow
                          label={t('settingsAutomation.secretProjectionEnabled')}
                          description={t('settingsAutomation.secretProjectionEnabledDescription')}
                          checked={formData.ai_provider.ocr_api?.projection_enabled ?? false}
                          onChange={(value) => onExternalApiChange('ocr_api', 'projection_enabled', value)}
                        />
                      </div>
                    )}
                  </>
                )}
                {currentOcrSupportsModelSelection ? (
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
                    {(modelCompatibilityNotice.ocr_api || currentOcrModelSupport === false) && (
                      <p className="mt-1 text-semantic-warning text-xs">
                        {modelCompatibilityNotice.ocr_api ?? t('settingsAutomation.ocrModelUnsupported')}
                      </p>
                    )}
                  </div>
                ) : (
                  <div className="rounded-md bg-surface-muted/80 p-3 text-content-secondary text-xs">
                    {t('settingsAutomation.modelSelectionUnsupportedSurface')}
                  </div>
                )}
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

          {showLlmSurfaceSection && (
            <div className="space-y-3 rounded-lg border border-muted p-4">
              <h4 className={`${typography.weight.medium} text-content-strong text-sm`}>
                LLM {t('settingsAutomation.externalApi')}
              </h4>
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                {showDirectApiFields(currentLlmSurface) && (
                  <>
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
                    {!currentLlmUsesNoAuth ? (
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
                    ) : (
                      <div className="rounded-md bg-surface-muted/80 p-3 text-content-secondary text-xs md:col-span-2">
                        {placementKindDescription(t, currentLlmSurface?.placement_kind)}
                      </div>
                    )}
                    {currentLlmUsesNoAuth && (
                      <div className="rounded-md bg-surface-elevated/70 p-3 text-content-secondary text-xs md:col-span-2">
                        {t('settingsAutomation.noAuthSurfaceDescription')}
                      </div>
                    )}
                    {!currentLlmUsesNoAuth && supportsProjectionToggle(formData.ai_provider.llm_api) && (
                      <div className="md:col-span-2">
                        <ToggleRow
                          label={t('settingsAutomation.secretProjectionEnabled')}
                          description={t('settingsAutomation.secretProjectionEnabledDescription')}
                          checked={formData.ai_provider.llm_api?.projection_enabled ?? false}
                          onChange={(value) => onExternalApiChange('llm_api', 'projection_enabled', value)}
                        />
                      </div>
                    )}
                  </>
                )}
                {showManagedHttpFields(currentLlmSurface) && (
                  <div className="rounded-lg border border-muted bg-surface-muted/80 p-3 md:col-span-2">
                    <p className="text-content-secondary text-sm">
                      {t('settingsAutomation.managedOAuthSurfaceDescription')}
                    </p>
                  </div>
                )}
                {showSubprocessFields(currentLlmSurface) && (
                  <div className="rounded-lg border border-muted bg-surface-muted/80 p-3 md:col-span-2">
                    <p className="text-content-secondary text-sm">
                      {t('settingsAutomation.subprocessSurfaceDescription')}
                    </p>
                    {currentLlmSurface?.subprocess_transport?.executable_candidates?.length ? (
                      <p className="mt-2 text-content-muted text-xs">
                        {t('settingsAutomation.subprocessExecutableHint', {
                          executables: currentLlmSurface.subprocess_transport.executable_candidates.join(', '),
                        })}
                      </p>
                    ) : null}
                  </div>
                )}
                {currentLlmSupportsModelSelection ? (
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
                    {(modelCompatibilityNotice.llm_api || currentLlmModelSupport === false) && (
                      <p className="mt-1 text-semantic-warning text-xs">
                        {modelCompatibilityNotice.llm_api ?? t('settingsAutomation.llmModelUnsupported')}
                      </p>
                    )}
                  </div>
                ) : (
                  <div className="rounded-md bg-surface-muted/80 p-3 text-content-secondary text-xs">
                    {t('settingsAutomation.modelSelectionUnsupportedSurface')}
                  </div>
                )}
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
              {showDirectApiFields(currentLlmSurface) && modelCatalogNotice.llm_api && (
                <p className="text-content-secondary text-xs">{modelCatalogNotice.llm_api}</p>
              )}
              {showDirectApiFields(currentLlmSurface) &&
                !canDiscoverModels('llm_api') &&
                !modelCatalogNotice.llm_api && (
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
