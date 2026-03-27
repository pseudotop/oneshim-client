import { useTranslation } from 'react-i18next'
import type { MonitorControlSettings } from '../../api/client'
import { Card, CardTitle, Input } from '../../components/ui'
import { form } from '../../styles/tokens'
import ToggleRow from './ToggleRow'
import type { SettingsFormTabProps } from './types'

interface MonitoringTabProps extends SettingsFormTabProps {
  onRootChange: (
    field: 'capture_enabled' | 'idle_threshold_secs' | 'metrics_interval_secs' | 'process_interval_secs',
    value: boolean | number,
  ) => void
  onMonitorChange: (field: keyof MonitorControlSettings, value: boolean) => void
}

export default function MonitoringTab({ formData, onRootChange, onMonitorChange }: MonitoringTabProps) {
  const { t } = useTranslation()

  return (
    <div className="space-y-6">
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
              onChange={(e) => onRootChange('capture_enabled', e.target.checked)}
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
                onChange={(e) => onRootChange('idle_threshold_secs', parseInt(e.target.value, 10) || 300)}
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
                onChange={(e) => onRootChange('metrics_interval_secs', parseInt(e.target.value, 10) || 5)}
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
                onChange={(e) => onRootChange('process_interval_secs', parseInt(e.target.value, 10) || 10)}
              />
            </div>
          </div>
        </div>
      </Card>

      <Card id="section-monitoring" variant="default" padding="lg">
        <CardTitle className="mb-4">{t('settings.monitorTitle')}</CardTitle>
        <div className="space-y-4">
          <ToggleRow
            label={t('settings.processMonitoring')}
            description={t('settings.processMonitoringDesc')}
            checked={formData.monitor.process_monitoring}
            onChange={(value) => onMonitorChange('process_monitoring', value)}
          />
          <ToggleRow
            label={t('settings.inputActivity')}
            description={t('settings.inputActivityDesc')}
            checked={formData.monitor.input_activity}
            onChange={(value) => onMonitorChange('input_activity', value)}
          />
          <ToggleRow
            label={t('settings.privacyMode')}
            description={t('settings.privacyModeDesc')}
            checked={formData.monitor.privacy_mode}
            onChange={(value) => onMonitorChange('privacy_mode', value)}
          />
        </div>
      </Card>
    </div>
  )
}
