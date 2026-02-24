/**
 *
 */
import { useTranslation } from 'react-i18next'
import { Card, CardTitle, Input } from '../../components/ui'
import { colors, form } from '../../styles/tokens'
import type { NotificationSettings as NotificationSettingsType } from '../../api/client'

interface NotificationSettingsProps {
  notification: NotificationSettingsType
  onChange: (field: keyof NotificationSettingsType, value: number | boolean) => void
}

export default function NotificationSettings({ notification, onChange }: NotificationSettingsProps) {
  const { t } = useTranslation()

  return (
    <Card variant="default" padding="lg">
      <CardTitle className="mb-4">{t('settings.notifTitle')}</CardTitle>

      {/* UI note */}
      <label className={`flex items-center justify-between cursor-pointer mb-6 pb-4 border-b ${form.sectionDivider}`}>
        <div>
          <span className={`${colors.text.secondary} font-medium`}>{t('settings.notifEnabled')}</span>
          <p className={colors.text.tertiary}>{t('settings.notifEnabledDesc')}</p>
        </div>
        <input
          type="checkbox"
          checked={notification.enabled}
          onChange={(e) => onChange('enabled', e.target.checked)}
          className={form.checkbox}
        />
      </label>

      <div className={`space-y-6 ${!notification.enabled ? 'opacity-50 pointer-events-none' : ''}`}>
        {/* idle notification */}
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4 items-start">
          <label className="flex items-center cursor-pointer">
            <input
              type="checkbox"
              checked={notification.idle_notification}
              onChange={(e) => onChange('idle_notification', e.target.checked)}
              className={form.checkboxInline}
            />
            <div>
              <span className={colors.text.secondary}>{t('settings.notifIdle')}</span>
              <p className={colors.text.tertiary}>{t('settings.notifIdleDesc')}</p>
            </div>
          </label>
          <div>
            <label className={form.label}>
              {t('settings.notifIdleThreshold')}
            </label>
            <Input
              type="number"
              min={5}
              max={120}
              value={notification.idle_notification_mins}
              onChange={(e) => onChange('idle_notification_mins', parseInt(e.target.value) || 30)}
              disabled={!notification.idle_notification}
            />
          </div>
        </div>

        {/* UI note */}
        <div className={`grid grid-cols-1 md:grid-cols-2 gap-4 items-start pt-4 border-t ${form.sectionDivider}`}>
          <label className="flex items-center cursor-pointer">
            <input
              type="checkbox"
              checked={notification.long_session_notification}
              onChange={(e) => onChange('long_session_notification', e.target.checked)}
              className={form.checkboxInline}
            />
            <div>
              <span className={colors.text.secondary}>{t('settings.notifLongSession')}</span>
              <p className={colors.text.tertiary}>{t('settings.notifLongSessionDesc')}</p>
            </div>
          </label>
          <div>
            <label className={form.label}>
              {t('settings.notifLongSessionThreshold')}
            </label>
            <Input
              type="number"
              min={30}
              max={240}
              value={notification.long_session_mins}
              onChange={(e) => onChange('long_session_mins', parseInt(e.target.value) || 60)}
              disabled={!notification.long_session_notification}
            />
          </div>
        </div>

        {/* UI note */}
        <div className={`grid grid-cols-1 md:grid-cols-2 gap-4 items-start pt-4 border-t ${form.sectionDivider}`}>
          <label className="flex items-center cursor-pointer">
            <input
              type="checkbox"
              checked={notification.high_usage_notification}
              onChange={(e) => onChange('high_usage_notification', e.target.checked)}
              className={form.checkboxInline}
            />
            <div>
              <span className={colors.text.secondary}>{t('settings.notifHighUsage')}</span>
              <p className={colors.text.tertiary}>{t('settings.notifHighUsageDesc')}</p>
            </div>
          </label>
          <div>
            <label className={form.label}>
              {t('settings.notifHighUsageThreshold')}
            </label>
            <Input
              type="number"
              min={50}
              max={99}
              value={notification.high_usage_threshold}
              onChange={(e) => onChange('high_usage_threshold', parseInt(e.target.value) || 90)}
              disabled={!notification.high_usage_notification}
            />
          </div>
        </div>
      </div>
    </Card>
  )
}
