import { lazy, Suspense, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useSearchParams } from 'react-router-dom'
import type { AppSettings } from '../api/client'
import { Button, Spinner, Tabs } from '../components/ui'
import { useShellLayoutContext } from '../contexts/ShellLayoutContext'
import { colors, typography } from '../styles/tokens'
import { cn } from '../utils/cn'
import { useSettingsData } from './hooks/useSettingsData'
import { useSettingsForm } from './hooks/useSettingsForm'
import { isSettingsTabId, type SettingsTabId } from './settings-utils'

const GeneralTab = lazy(() => import('./setting-tabs/GeneralTab'))
const PrivacyTab = lazy(() => import('./setting-tabs/PrivacyTab'))
const MonitoringTab = lazy(() => import('./setting-tabs/MonitoringTab'))
const AiAutomationTab = lazy(() => import('./setting-tabs/AiAutomationTab'))
const DataStorageTab = lazy(() => import('./setting-tabs/DataStorageTab'))
const CoachingGoalsTab = lazy(() => import('./setting-tabs/CoachingGoalsTab'))
const AudioTab = lazy(() => import('./setting-tabs/AudioTab'))
const AdvancedTab = lazy(() => import('./setting-tabs/AdvancedTab'))

export default function Settings() {
  const { t } = useTranslation()
  const [searchParams, setSearchParams] = useSearchParams()
  const { sidebarCollapsed } = useShellLayoutContext()

  const [activeTab, setActiveTab] = useState<SettingsTabId>(() => {
    const tab = searchParams.get('tab')
    return isSettingsTabId(tab) ? tab : 'general'
  })

  // ---- Data fetching hook (queries + provider catalog) --------------------
  // useSettingsData needs formData for endpoint probes, so we pass it after
  // useSettingsForm initialises it. On the first render formData is null,
  // which is fine — the probe queries are disabled until formData is available.
  const [formDataForProbes, setFormDataForProbes] = useState<AppSettings | null>(null)
  const settingsData = useSettingsData(formDataForProbes)

  // ---- Form state hook (mutations + handlers) -----------------------------
  const form = useSettingsForm(settingsData)

  // Keep the probe-input in sync with form state
  useEffect(() => {
    setFormDataForProbes(form.formData)
  }, [form.formData])

  // ---- URL <-> tab sync ---------------------------------------------------
  useEffect(() => {
    const tab = searchParams.get('tab')
    if (isSettingsTabId(tab) && tab !== activeTab) {
      setActiveTab(tab)
    }
  }, [activeTab, searchParams])

  const handleTabChange = (tab: SettingsTabId) => {
    setActiveTab(tab)
    const nextParams = new URLSearchParams(searchParams)
    nextParams.set('tab', tab)
    setSearchParams(nextParams, { replace: true })
  }

  // ---- Tab definitions ----------------------------------------------------
  const tabs = [
    { id: 'general', label: t('settings.tabs.general') },
    { id: 'privacy', label: t('settings.tabs.privacy') },
    { id: 'monitoring', label: t('settings.tabs.monitoring') },
    { id: 'ai-automation', label: t('settings.tabs.aiAutomation') },
    { id: 'data', label: t('settings.tabs.dataStorage') },
    { id: 'coaching', label: t('settings.tabs.coaching', 'Coaching Goals') },
    { id: 'audio', label: t('settings.tabs.audio', 'Audio') },
    { id: 'advanced', label: t('settings.tabs.advanced', 'Advanced') },
  ]

  // ---- Loading state ------------------------------------------------------
  if (settingsData.settingsLoading || !form.formData) {
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
            <span className="ml-2 font-normal text-base text-content-tertiary">
              {'>'} {tabs.find((tab) => tab.id === activeTab)?.label}
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

      {form.hasUnsavedChanges && (
        <div className="pointer-events-none fixed right-6 bottom-10 z-30 flex justify-end">
          <div className="pointer-events-auto flex items-center gap-4 rounded-xl border border-muted bg-surface-overlay px-4 py-3 shadow-2xl">
            <div className="min-w-0">
              <p className={cn(`${typography.weight.semibold} text-sm`, colors.text.primary)}>
                {t('settings.unsavedChanges')}
              </p>
              <p className={cn('text-xs', colors.text.secondary)}>{t('settings.unsavedChangesHint')}</p>
            </div>
            <Button
              type="button"
              variant="secondary"
              size="lg"
              onClick={form.handleRevertChanges}
              disabled={form.saveMutation.isPending}
            >
              {t('settings.revertChanges')}
            </Button>
            <Button
              data-testid="settings-save-floating"
              type="submit"
              form="settings-form"
              variant="primary"
              size="lg"
              isLoading={form.saveMutation.isPending}
              disabled={form.saveDisabled}
            >
              {form.saveMutation.isPending ? t('settings.saving') : t('settings.saveSettings')}
            </Button>
          </div>
        </div>
      )}

      <form id="settings-form" className="space-y-6" onSubmit={form.handleSubmit}>
        <Suspense
          fallback={
            <div className="flex items-center justify-center py-12">
              <Spinner />
            </div>
          }
        >
          {activeTab === 'general' && (
            <div id="settings-panel-general" role="tabpanel" aria-labelledby="settings-tab-general">
              <GeneralTab
                formData={form.formData}
                updateStatus={settingsData.updateStatus}
                updateActionPending={form.updateActionMutation.isPending}
                onRootChange={(field, value) => form.handleRootChange(field as keyof AppSettings, value)}
                onNotificationChange={form.handleNotificationChange}
                onScheduleChange={form.handleScheduleChange}
                onUpdateChange={form.handleUpdateChange}
                onUpdateAction={(action) => form.updateActionMutation.mutate(action)}
              />
            </div>
          )}

          {activeTab === 'privacy' && (
            <div id="settings-panel-privacy" role="tabpanel" aria-labelledby="settings-tab-privacy">
              <PrivacyTab formData={form.formData} onPrivacyChange={form.handlePrivacyChange} />
            </div>
          )}

          {activeTab === 'monitoring' && (
            <div id="settings-panel-monitoring" role="tabpanel" aria-labelledby="settings-tab-monitoring">
              <MonitoringTab
                formData={form.formData}
                permissionStatus={settingsData.desktopPermissionStatus ?? null}
                permissionStatusError={settingsData.desktopPermissionStatusError}
                permissionStatusLoading={settingsData.desktopPermissionStatusLoading}
                permissionStatusRefreshing={settingsData.desktopPermissionStatusRefreshing}
                notificationPermissionRequesting={form.requestNotificationPermissionMutation.isPending}
                onRootChange={(field, value) => form.handleRootChange(field as keyof AppSettings, value)}
                onMonitorChange={form.handleMonitorChange}
                onRefreshPermissionStatus={
                  settingsData.canQueryDesktopCapabilities
                    ? settingsData.handleRefreshDesktopPermissionStatus
                    : undefined
                }
                onRequestNotificationPermission={
                  settingsData.canQueryDesktopCapabilities
                    ? () => form.requestNotificationPermissionMutation.mutate()
                    : undefined
                }
              />
            </div>
          )}

          {activeTab === 'ai-automation' && (
            <div id="settings-panel-ai-automation" role="tabpanel" aria-labelledby="settings-tab-ai-automation">
              <AiAutomationTab
                formData={form.formData}
                allProviderSurfaces={settingsData.providerCatalog.surfaces}
                providerSurfaceOptions={{
                  ocr_api: form.getCompatibleSurfaceOptions('ocr_api'),
                  llm_api: form.getCompatibleSurfaceOptions('llm_api'),
                }}
                featureCapabilities={settingsData.featureCapabilities}
                secretBackendCapabilities={settingsData.secretBackendCapabilities}
                modelCatalogNotice={form.modelCatalogNotice}
                modelCompatibilityNotice={{
                  ocr_api: form.getModelCompatibilityNotice('ocr_api'),
                  llm_api: form.getModelCompatibilityNotice('llm_api'),
                }}
                modelCatalogLoading={form.modelCatalogLoading}
                endpointProbeResult={{
                  ocr_api: settingsData.ocrEndpointProbe,
                  llm_api: settingsData.llmEndpointProbe,
                }}
                endpointProbeLoading={{
                  ocr_api: settingsData.ocrEndpointProbeLoading,
                  llm_api: settingsData.llmEndpointProbeLoading,
                }}
                onAutomationChange={form.handleAutomationChange}
                onSandboxChange={form.handleSandboxChange}
                onAiProviderChange={form.handleAiProviderChange}
                onOcrValidationChange={form.handleOcrValidationChange}
                onSceneActionOverrideChange={form.handleSceneActionOverrideChange}
                onSceneIntelligenceChange={form.handleSceneIntelligenceChange}
                onExternalApiChange={form.handleExternalApiChange}
                resolveProviderSurface={form.resolveEndpointSurface}
                onProviderSurfaceChange={form.handleProviderSurfaceChange}
                onSelectAiProviderProfile={form.handleSelectAiProviderProfile}
                onSaveAiProviderProfile={form.handleSaveAiProviderProfile}
                onDeleteAiProviderProfile={form.handleDeleteAiProviderProfile}
                onDiscoverModels={form.discoverModels}
                getModelOptions={form.getModelOptions}
                canDiscoverModels={form.canDiscoverModels}
              />
            </div>
          )}

          {activeTab === 'data' && (
            <div id="settings-panel-data" role="tabpanel" aria-labelledby="settings-tab-data">
              <DataStorageTab
                formData={form.formData}
                storageStats={settingsData.storageStats}
                storageLoading={settingsData.storageLoading}
                exportFormat={form.exportFormat}
                exportLoading={form.exportLoading}
                onExportFormatChange={form.setExportFormat}
                onExport={form.handleExport}
                onRootChange={(field, value) => form.handleRootChange(field as keyof AppSettings, value)}
                onTelemetryChange={form.handleTelemetryChange}
              />
            </div>
          )}

          {activeTab === 'coaching' && (
            <div id="settings-panel-coaching" role="tabpanel" aria-labelledby="settings-tab-coaching">
              <CoachingGoalsTab />
            </div>
          )}

          {activeTab === 'audio' && (
            <div id="settings-panel-audio" role="tabpanel" aria-labelledby="settings-tab-audio">
              <AudioTab
                formData={form.formData}
                onAudioChange={(field, value) => {
                  form.setFormData((prev) => {
                    if (!prev) return prev
                    return { ...prev, audio: { ...prev.audio, [field]: value } }
                  })
                }}
              />
            </div>
          )}

          {activeTab === 'advanced' && form.formData && (
            <div id="settings-panel-advanced" role="tabpanel" aria-labelledby="settings-tab-advanced">
              <AdvancedTab
                formData={form.formData}
                onChange={(section, field, value) => {
                  form.setFormData((prev) => {
                    if (!prev) return prev
                    const sectionData = prev[section]
                    if (typeof sectionData === 'object' && sectionData !== null) {
                      return { ...prev, [section]: { ...sectionData, [field]: value } }
                    }
                    return prev
                  })
                }}
              />
            </div>
          )}
        </Suspense>

        <div className="flex justify-end">
          <Button
            data-testid="settings-save"
            type="submit"
            variant="primary"
            size="lg"
            isLoading={form.saveMutation.isPending}
            disabled={form.saveDisabled}
          >
            {form.saveMutation.isPending ? t('settings.saving') : t('settings.saveSettings')}
          </Button>
        </div>
      </form>
    </div>
  )
}
