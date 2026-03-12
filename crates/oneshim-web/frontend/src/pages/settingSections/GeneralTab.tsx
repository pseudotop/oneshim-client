import { useTranslation } from 'react-i18next'
import type {
  AppSettings,
  NotificationSettings as NotificationSettingsType,
  ScheduleSettings as ScheduleSettingsType,
  UpdateAction,
  UpdateStatus,
} from '../../api/client'
import LanguageSelector from '../../components/LanguageSelector'
import { Button, Card, CardTitle, Input } from '../../components/ui'
import { DEFAULT_WEB_PORT } from '../../constants'
import { form } from '../../styles/tokens'
import NotificationSettings from './NotificationSettings'
import ScheduleSettings from './ScheduleSettings'
import ToggleRow from './ToggleRow'
import type { SettingsFormTabProps } from './types'

interface GeneralTabProps extends SettingsFormTabProps {
  updateStatus?: UpdateStatus
  updateActionPending: boolean
  onRootChange: (field: 'web_port' | 'allow_external', value: number | boolean) => void
  onNotificationChange: (field: keyof NotificationSettingsType, value: number | boolean) => void
  onScheduleChange: (field: keyof ScheduleSettingsType, value: boolean | number | string[]) => void
  onUpdateChange: (field: keyof AppSettings['update'], value: boolean | number) => void
  onUpdateAction: (action: UpdateAction) => void
}

export default function GeneralTab({
  formData,
  updateStatus,
  updateActionPending,
  onRootChange,
  onNotificationChange,
  onScheduleChange,
  onUpdateChange,
  onUpdateAction,
}: GeneralTabProps) {
  const { t } = useTranslation()

  return (
    <div className="space-y-6">
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('settings.language')}</CardTitle>
        <LanguageSelector />
      </Card>

      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('settings.webTitle')}</CardTitle>
        <div className="grid grid-cols-1 gap-6 md:grid-cols-2">
          <div>
            <label htmlFor="settings-web-port" className={form.label}>
              {t('settings.portLabel')}
            </label>
            <Input
              id="settings-web-port"
              type="number"
              min={1024}
              max={65535}
              value={formData.web_port}
              onChange={(e) => onRootChange('web_port', parseInt(e.target.value, 10) || DEFAULT_WEB_PORT)}
            />
            <p className={form.helper}>{t('settings.portRestart')}</p>
          </div>
          <div className="flex items-center">
            <label className="flex cursor-pointer items-center">
              <input
                type="checkbox"
                checked={formData.allow_external}
                onChange={(e) => onRootChange('allow_external', e.target.checked)}
                className={form.checkboxInline}
              />
              <div>
                <span className="text-content-strong">{t('settings.allowExternal')}</span>
                <p className="text-content-secondary text-xs">{t('settings.allowExternalDesc')}</p>
              </div>
            </label>
          </div>
        </div>
      </Card>

      <div id="section-notification">
        <NotificationSettings notification={formData.notification} onChange={onNotificationChange} />
      </div>

      <div id="section-schedule">
        <ScheduleSettings schedule={formData.schedule} onChange={onScheduleChange} />
      </div>

      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('settings.updateTitle')}</CardTitle>
        <div className="space-y-4">
          <ToggleRow
            label={t('settings.updateEnabled')}
            description={t('settings.updateEnabledDesc')}
            checked={formData.update.enabled}
            onChange={(value) => onUpdateChange('enabled', value)}
          />

          <ToggleRow
            label={t('settings.updateAutoInstall')}
            description={t('settings.updateAutoInstallDesc')}
            checked={formData.update.auto_install}
            onChange={(value) => onUpdateChange('auto_install', value)}
          />

          <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
            <div>
              <label htmlFor="settings-update-interval" className={form.label}>
                {t('settings.updateIntervalHours')}
              </label>
              <Input
                id="settings-update-interval"
                type="number"
                min={1}
                max={168}
                value={formData.update.check_interval_hours}
                onChange={(e) => onUpdateChange('check_interval_hours', parseInt(e.target.value, 10) || 24)}
              />
            </div>
            <div className="flex items-end">
              <label className="flex cursor-pointer items-center">
                <input
                  type="checkbox"
                  checked={formData.update.include_prerelease}
                  onChange={(e) => onUpdateChange('include_prerelease', e.target.checked)}
                  className={form.checkboxInline}
                />
                <div>
                  <span className="text-content-strong">{t('settings.updateIncludePrerelease')}</span>
                  <p className="text-content-secondary text-xs">{t('settings.updateIncludePrereleaseDesc')}</p>
                </div>
              </label>
            </div>
          </div>

          <div className="mt-2 rounded-lg border border-muted bg-surface-inset p-4">
            <div className="font-medium text-content text-sm">{t('settings.updateRuntimeStatus')}</div>
            <div className="mt-1 text-content-strong text-sm">
              {updateStatus?.message ?? t('settings.updateStatusUnavailable')}
            </div>
            {updateStatus?.pending && (
              <div className="mt-2 space-y-1 text-content-secondary text-xs">
                <div>
                  {t('settings.updateCurrentVersion')}: {updateStatus.pending.current_version}
                </div>
                <div>
                  {t('settings.updateLatestVersion')}: {updateStatus.pending.latest_version}
                </div>
                <a
                  href={updateStatus.pending.release_url}
                  target="_blank"
                  rel="noreferrer"
                  className="text-accent-teal underline"
                >
                  {t('settings.updateReleaseNote')}
                </a>
              </div>
            )}
            <div className="mt-4 flex flex-wrap gap-2">
              <Button
                type="button"
                variant="secondary"
                size="sm"
                isLoading={updateActionPending}
                onClick={() => onUpdateAction('CheckNow')}
              >
                {t('settings.updateCheckNow')}
              </Button>
              <Button
                type="button"
                variant="primary"
                size="sm"
                isLoading={updateActionPending}
                onClick={() => onUpdateAction('Approve')}
              >
                {t('settings.updateApproveNow')}
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                isLoading={updateActionPending}
                onClick={() => onUpdateAction('Defer')}
              >
                {t('settings.updateDefer')}
              </Button>
            </div>
          </div>
        </div>
      </Card>
    </div>
  )
}
