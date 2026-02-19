/**
 * 스케줄 설정 섹션 컴포넌트
 *
 * 활성 시간/요일, 화면 잠금/배터리 절약 시 일시정지 설정
 */
import { useTranslation } from 'react-i18next'
import { Card, CardTitle, Input } from '../../components/ui'
import type { ScheduleSettings as ScheduleSettingsType } from '../../api/client'
import ToggleRow from './ToggleRow'

interface ScheduleSettingsProps {
  schedule: ScheduleSettingsType
  onChange: (field: keyof ScheduleSettingsType, value: boolean | number | string[]) => void
}

export default function ScheduleSettings({ schedule, onChange }: ScheduleSettingsProps) {
  const { t } = useTranslation()

  return (
    <Card variant="default" padding="lg">
      <CardTitle className="mb-4">{t('settings.scheduleTitle')}</CardTitle>
      <div className="space-y-4">
        <ToggleRow
          label={t('settings.scheduleEnabled')}
          description={t('settings.scheduleEnabledDesc')}
          checked={schedule.active_hours_enabled}
          onChange={(v) => onChange('active_hours_enabled', v)}
        />

        <div className={`space-y-4 ${!schedule.active_hours_enabled ? 'opacity-50 pointer-events-none' : ''}`}>
          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
                {t('settings.startHour')}
              </label>
              <Input
                type="number"
                min={0}
                max={23}
                value={schedule.active_start_hour}
                onChange={(e) => onChange('active_start_hour', parseInt(e.target.value) || 9)}
              />
            </div>
            <div>
              <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
                {t('settings.endHour')}
              </label>
              <Input
                type="number"
                min={0}
                max={23}
                value={schedule.active_end_hour}
                onChange={(e) => onChange('active_end_hour', parseInt(e.target.value) || 18)}
              />
            </div>
          </div>

          <div>
            <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
              {t('settings.activeDays')}
            </label>
            <div className="flex flex-wrap gap-2">
              {(['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'] as const).map((day) => {
                const labels: Record<string, string> = { Mon: t('settings.days.Mon'), Tue: t('settings.days.Tue'), Wed: t('settings.days.Wed'), Thu: t('settings.days.Thu'), Fri: t('settings.days.Fri'), Sat: t('settings.days.Sat'), Sun: t('settings.days.Sun') }
                const isActive = schedule.active_days.includes(day)
                return (
                  <button
                    key={day}
                    type="button"
                    onClick={() => {
                      const newDays = isActive
                        ? schedule.active_days.filter(d => d !== day)
                        : [...schedule.active_days, day]
                      onChange('active_days', newDays)
                    }}
                    className={`px-3 py-1 rounded-full text-sm font-medium transition-colors ${
                      isActive
                        ? 'bg-teal-500 text-white'
                        : 'bg-slate-200 dark:bg-slate-700 text-slate-600 dark:text-slate-400 hover:bg-slate-300 dark:hover:bg-slate-600'
                    }`}
                  >
                    {labels[day]}
                  </button>
                )
              })}
            </div>
          </div>
        </div>

        <div className="pt-4 border-t border-slate-300 dark:border-slate-700 space-y-4">
          <ToggleRow
            label={t('settings.pauseOnLock')}
            description={t('settings.pauseOnLockDesc')}
            checked={schedule.pause_on_screen_lock}
            onChange={(v) => onChange('pause_on_screen_lock', v)}
          />
          <ToggleRow
            label={t('settings.pauseOnBattery')}
            description={t('settings.pauseOnBatteryDesc')}
            checked={schedule.pause_on_battery_saver}
            onChange={(v) => onChange('pause_on_battery_saver', v)}
          />
        </div>
      </div>
    </Card>
  )
}
