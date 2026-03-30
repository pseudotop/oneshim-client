import { Bell, Camera, CircleAlert, CircleCheckBig, Monitor } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { DesktopPermissionSnapshot, DesktopPermissionState, MonitorControlSettings } from '../../api/client'
import { Alert, Badge, Card, CardTitle, Checkbox, Input } from '../../components/ui'
import { colors, form, iconSize, typography } from '../../styles/tokens'
import { cn } from '../../utils/cn'
import ToggleRow from './ToggleRow'
import type { SettingsFormTabProps } from './types'

interface MonitoringTabProps extends SettingsFormTabProps {
  permissionStatus?: DesktopPermissionSnapshot | null
  permissionStatusLoading?: boolean
  onRootChange: (
    field: 'capture_enabled' | 'idle_threshold_secs' | 'metrics_interval_secs' | 'process_interval_secs',
    value: boolean | number,
  ) => void
  onMonitorChange: (field: keyof MonitorControlSettings, value: boolean) => void
}

function readNotificationPermissionState(): DesktopPermissionState {
  if (typeof window === 'undefined' || !('Notification' in window)) {
    return 'unavailable'
  }
  return Notification.permission === 'granted' ? 'granted' : 'needs_attention'
}

function badgeColorForState(state: DesktopPermissionState): 'success' | 'warning' | 'default' {
  switch (state) {
    case 'granted':
      return 'success'
    case 'needs_attention':
      return 'warning'
    case 'not_required':
    case 'unavailable':
      return 'default'
  }
}

export default function MonitoringTab({
  formData,
  permissionStatus,
  permissionStatusLoading = false,
  onRootChange,
  onMonitorChange,
}: MonitoringTabProps) {
  const { t } = useTranslation()
  const [notificationState, setNotificationState] = useState<DesktopPermissionState>(() =>
    readNotificationPermissionState(),
  )
  const permissionStateLabels: Record<DesktopPermissionState, string> = {
    granted: t('settings.permissionStateGranted', 'Ready'),
    needs_attention: t('settings.permissionStateNeedsAttention', 'Attention needed'),
    not_required: t('settings.permissionStateNotRequired', 'Not required'),
    unavailable: t('settings.permissionStateUnavailable', 'Unavailable'),
  }

  useEffect(() => {
    const refresh = () => setNotificationState(readNotificationPermissionState())
    refresh()
    window.addEventListener('focus', refresh)
    document.addEventListener('visibilitychange', refresh)
    return () => {
      window.removeEventListener('focus', refresh)
      document.removeEventListener('visibilitychange', refresh)
    }
  }, [])

  const showPermissionSection = permissionStatusLoading || Boolean(permissionStatus)
  const isMac = permissionStatus?.platform === 'macos'
  const isLinux = permissionStatus?.platform === 'linux'
  const permissionRows = isMac
    ? [
        {
          id: 'accessibility',
          icon: <Monitor className={cn(iconSize.base, 'text-brand-text')} />,
          label: t('onboarding.step2Accessibility'),
          description: t(
            'settings.permissionAccessibilityDesc',
            'Needed for focused element tracking and GUI context analysis.',
          ),
          state: permissionStatus?.accessibility.state ?? 'unavailable',
        },
        {
          id: 'screen-capture',
          icon: <Camera className={cn(iconSize.base, 'text-brand-text')} />,
          label: t('settings.permissionScreenRecordingLabel', 'Screen Recording'),
          description: t(
            'settings.permissionScreenCaptureDesc',
            'Needed to capture screenshots for timeline and insights.',
          ),
          state: permissionStatus?.screen_capture.state ?? 'unavailable',
        },
        {
          id: 'notifications',
          icon: <Bell className={cn(iconSize.base, 'text-brand-text')} />,
          label: t('onboarding.step2Notifications'),
          description: t('settings.permissionNotificationsDesc', 'Used for break reminders and coaching prompts.'),
          state: notificationState,
        },
      ]
    : isLinux
      ? [
          {
            id: 'accessibility-service',
            icon: <Monitor className={cn(iconSize.base, 'text-brand-text')} />,
            label: t('settings.permissionLinuxAccessibilityLabel', 'Accessibility Service'),
            description: t(
              'settings.permissionLinuxAccessibilityDesc',
              'AT-SPI session availability controls focused element context on Linux.',
            ),
            state: permissionStatus?.accessibility.state ?? 'unavailable',
          },
          {
            id: 'notifications',
            icon: <Bell className={cn(iconSize.base, 'text-brand-text')} />,
            label: t('onboarding.step2Notifications'),
            description: t('settings.permissionNotificationsDesc', 'Used for break reminders and coaching prompts.'),
            state: notificationState,
          },
        ]
      : []

  const needsAttention = permissionRows.some((row) => row.state === 'needs_attention' || row.state === 'unavailable')

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
            <Checkbox
              checked={formData.capture_enabled}
              onChange={(e) => onRootChange('capture_enabled', e.target.checked)}
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

      {showPermissionSection && (
        <Card variant="default" padding="lg">
          <CardTitle className="mb-1">
            {isMac
              ? t('settings.permissionSectionTitleMac', 'macOS Permissions')
              : isLinux
                ? t('settings.permissionSectionTitleLinux', 'Linux Session Access')
                : t('settings.permissionSectionTitle', 'Desktop Access')}
          </CardTitle>
          <p className="mb-4 text-content-secondary text-sm">
            {isMac
              ? t(
                  'settings.permissionSectionDescMac',
                  'Grant the required macOS permissions so ONESHIM can capture context reliably.',
                )
              : isLinux
                ? t(
                    'settings.permissionSectionDescLinux',
                    'Linux usually does not require extra prompts, but session services still need to be reachable.',
                  )
                : t(
                    'settings.permissionSectionDesc',
                    'Check whether desktop integrations required by ONESHIM are currently reachable.',
                  )}
          </p>

          {permissionStatusLoading && (
            <p className="text-content-secondary text-sm">
              {t('settings.permissionChecking', 'Checking desktop access...')}
            </p>
          )}

          {!permissionStatusLoading && permissionStatus?.platform === 'windows' && (
            <Alert
              variant="info"
              title={t('settings.permissionWindowsTitle', 'Windows access')}
              icon={<CircleCheckBig />}
            >
              {t(
                'settings.permissionWindowsDesc',
                'Windows does not usually require separate OS permissions for accessibility or screen capture. Notifications can still depend on system notification settings.',
              )}
            </Alert>
          )}

          {!permissionStatusLoading && permissionRows.length > 0 && (
            <>
              <div className="space-y-3">
                {permissionRows.map((row) => (
                  <div
                    key={row.id}
                    className="flex items-start justify-between gap-4 rounded-lg border border-muted bg-surface-inset p-4"
                  >
                    <div className="min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="flex h-8 w-8 items-center justify-center rounded-md bg-brand-signal/15">
                          {row.icon}
                        </span>
                        <span className={cn(typography.label, colors.text.primary)}>{row.label}</span>
                      </div>
                      <p className="mt-2 text-content-secondary text-sm">{row.description}</p>
                    </div>
                    <Badge color={badgeColorForState(row.state)} size="md" className="shrink-0">
                      {permissionStateLabels[row.state]}
                    </Badge>
                  </div>
                ))}
              </div>

              {needsAttention && (
                <Alert
                  variant="warning"
                  className="mt-4"
                  title={t('settings.permissionAttentionTitle', 'Action recommended')}
                  icon={<CircleAlert />}
                >
                  {isMac
                    ? t(
                        'settings.permissionAttentionDescMac',
                        'If a required permission is missing, re-open the setup guide and approve it in System Settings.',
                      )
                    : t(
                        'settings.permissionAttentionDesc',
                        'If a service is unavailable, check your desktop session settings and reopen ONESHIM.',
                      )}
                </Alert>
              )}
            </>
          )}
        </Card>
      )}

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
