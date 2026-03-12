/**
 * Monitoring settings tab: monitoring controls, OCR validation, capture triggers, scene action overrides.
 */
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  type AppSettings,
  type MonitorControlSettings,
  type OcrValidationSettings as OcrValidationSettingsType,
  type SceneActionOverrideSettings as SceneActionOverrideSettingsType,
  fetchSettings,
  updateSettings,
} from '../../api/client'
import { Button, Card, CardTitle, Input, Spinner } from '../../components/ui'
import { useToast } from '../../hooks/useToast'
import { colors, form } from '../../styles/tokens'
import { cn } from '../../utils/cn'
import ToggleRow from './ToggleRow'

export default function MonitoringTab() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const toast = useToast()

  const toDateTimeLocalValue = (value: string | null | undefined): string => {
    if (!value) return ''
    const d = new Date(value)
    if (Number.isNaN(d.getTime())) return ''
    const pad = (n: number) => String(n).padStart(2, '0')
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
  }

  const toRfc3339OrNull = (value: string): string | null => {
    if (!value.trim()) return null
    const d = new Date(value)
    if (Number.isNaN(d.getTime())) return null
    return d.toISOString()
  }

  const { data: settings, isLoading: settingsLoading } = useQuery({
    queryKey: ['settings'],
    queryFn: fetchSettings,
  })

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

  const handleChange = (field: keyof AppSettings, value: number | boolean) => {
    if (formData) {
      setFormData({ ...formData, [field]: value })
    }
  }

  const handleMonitorChange = (field: keyof MonitorControlSettings, value: boolean) => {
    if (formData) {
      setFormData({
        ...formData,
        monitor: { ...formData.monitor, [field]: value },
      })
    }
  }

  const handleOcrValidationChange = (field: keyof OcrValidationSettingsType, value: boolean | number) => {
    if (formData) {
      setFormData({
        ...formData,
        ai_provider: {
          ...formData.ai_provider,
          ocr_validation: {
            ...formData.ai_provider.ocr_validation,
            [field]: value,
          },
        },
      })
    }
  }

  const handleSceneActionOverrideChange = (
    field: keyof SceneActionOverrideSettingsType,
    value: boolean | string | null,
  ) => {
    if (formData) {
      setFormData({
        ...formData,
        ai_provider: {
          ...formData.ai_provider,
          scene_action_override: {
            ...formData.ai_provider.scene_action_override,
            [field]: value,
          },
        },
      })
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
      {/* Data Collection */}
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('settings.collectionTitle')}</CardTitle>
        <div className="space-y-4">
          <label className="flex cursor-pointer items-center justify-between">
            <div>
              <span className="text-content-strong">{t('settings.captureEnabled')}</span>
              <p className="text-content-secondary text-xs">{t('settings.captureEnabledDesc')}</p>
            </div>
            <input
              type="checkbox"
              checked={formData.capture_enabled}
              onChange={(e) => handleChange('capture_enabled', e.target.checked)}
              className={form.checkbox}
            />
          </label>

          <div className="grid grid-cols-1 gap-4 pt-4 md:grid-cols-3">
            <div>
              <label htmlFor="settings-idle-threshold" className={form.label}>
                {t('settings.idleThresholdSecs')}
              </label>
              <Input
                id="settings-idle-threshold"
                type="number"
                min={60}
                max={3600}
                step={60}
                value={formData.idle_threshold_secs}
                onChange={(e) => handleChange('idle_threshold_secs', parseInt(e.target.value, 10) || 300)}
              />
            </div>
            <div>
              <label htmlFor="settings-metrics-interval" className={form.label}>
                {t('settings.metricsIntervalSecs')}
              </label>
              <Input
                id="settings-metrics-interval"
                type="number"
                min={1}
                max={60}
                value={formData.metrics_interval_secs}
                onChange={(e) => handleChange('metrics_interval_secs', parseInt(e.target.value, 10) || 5)}
              />
            </div>
            <div>
              <label htmlFor="settings-process-interval" className={form.label}>
                {t('settings.processIntervalSecs')}
              </label>
              <Input
                id="settings-process-interval"
                type="number"
                min={5}
                max={300}
                value={formData.process_interval_secs}
                onChange={(e) => handleChange('process_interval_secs', parseInt(e.target.value, 10) || 10)}
              />
            </div>
          </div>
        </div>
      </Card>

      {/* Monitoring Control */}
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('settings.monitorTitle')}</CardTitle>
        <div className="space-y-4">
          <ToggleRow
            label={t('settings.processMonitoring')}
            description={t('settings.processMonitoringDesc')}
            checked={formData.monitor.process_monitoring}
            onChange={(v) => handleMonitorChange('process_monitoring', v)}
          />
          <ToggleRow
            label={t('settings.inputActivity')}
            description={t('settings.inputActivityDesc')}
            checked={formData.monitor.input_activity}
            onChange={(v) => handleMonitorChange('input_activity', v)}
          />
          <ToggleRow
            label={t('settings.privacyMode')}
            description={t('settings.privacyModeDesc')}
            checked={formData.monitor.privacy_mode}
            onChange={(v) => handleMonitorChange('privacy_mode', v)}
          />
        </div>
      </Card>

      {/* OCR Validation */}
      <Card variant="default" padding="lg">
        <div className="space-y-3">
          <CardTitle>{t('settingsAutomation.ocrValidationTitle')}</CardTitle>
          <ToggleRow
            label={t('settingsAutomation.ocrValidationEnabled')}
            description={t('settingsAutomation.ocrValidationEnabledDescription')}
            checked={formData.ai_provider.ocr_validation.enabled}
            onChange={(v) => handleOcrValidationChange('enabled', v)}
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
                onChange={(e) => handleOcrValidationChange('min_confidence', Number(e.target.value))}
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
                onChange={(e) => handleOcrValidationChange('max_invalid_ratio', Number(e.target.value))}
              />
            </div>
          </div>
        </div>
      </Card>

      {/* Scene Action Override */}
      <Card variant="default" padding="lg">
        <div className="space-y-3">
          <CardTitle>{t('settingsAutomation.sceneActionOverrideTitle')}</CardTitle>
          <ToggleRow
            label={t('settingsAutomation.sceneActionOverrideEnabled')}
            description={t('settingsAutomation.sceneActionOverrideEnabledDescription')}
            checked={formData.ai_provider.scene_action_override.enabled}
            onChange={(v) => handleSceneActionOverrideChange('enabled', v)}
          />
          <div
            className={`grid grid-cols-1 gap-3 md:grid-cols-2 ${!formData.ai_provider.scene_action_override.enabled ? 'pointer-events-none opacity-50' : ''}`}
          >
            <div className="md:col-span-2">
              <label
                htmlFor="settings-scene-override-reason"
                className="mb-1 block text-content-secondary text-xs"
              >
                {t('settingsAutomation.sceneActionOverrideReason')}
              </label>
              <Input
                id="settings-scene-override-reason"
                type="text"
                value={formData.ai_provider.scene_action_override.reason}
                onChange={(e) => handleSceneActionOverrideChange('reason', e.target.value)}
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
                onChange={(e) => handleSceneActionOverrideChange('approved_by', e.target.value)}
                placeholder={t('settingsAutomation.sceneActionOverrideApprovedByPlaceholder')}
              />
            </div>
            <div>
              <label
                htmlFor="settings-scene-override-expires"
                className="mb-1 block text-content-secondary text-xs"
              >
                {t('settingsAutomation.sceneActionOverrideExpiresAt')}
              </label>
              <Input
                id="settings-scene-override-expires"
                type="datetime-local"
                value={toDateTimeLocalValue(formData.ai_provider.scene_action_override.expires_at)}
                onChange={(e) => handleSceneActionOverrideChange('expires_at', toRfc3339OrNull(e.target.value))}
              />
            </div>
          </div>
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
