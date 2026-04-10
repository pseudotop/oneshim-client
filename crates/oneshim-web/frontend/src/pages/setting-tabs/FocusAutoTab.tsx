import { Plus, X } from 'lucide-react'
import { useCallback, useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { AppSettings, FocusScheduleSettings } from '../../api/contracts'
import { Card, CardTitle } from '../../components/ui'
import { colors, form, iconSize, motion, typography } from '../../styles/tokens'
import { cn } from '../../utils/cn'
import { useSettingsFormContext } from '../settings/SettingsFormContext'

const DURATION_PRESETS = [15, 25, 45, 60, 90, 120]
const COOLDOWN_PRESETS = [1, 3, 5, 10, 15]

export default function FocusAutoTab() {
  const { t } = useTranslation()
  const { form: settingsForm } = useSettingsFormContext()
  const formData = settingsForm.formData ?? ({} as AppSettings)
  const focusAuto = formData.focus_auto

  const [newApp, setNewApp] = useState('')

  const updateFocusAuto = useCallback(
    (patch: Partial<typeof focusAuto>) => {
      settingsForm.handleRootChange('focus_auto' as never, { ...focusAuto, ...patch } as never)
    },
    [settingsForm, focusAuto],
  )

  const addApp = useCallback(() => {
    const trimmed = newApp.trim()
    if (!trimmed || focusAuto.trigger_apps.includes(trimmed)) return
    updateFocusAuto({ trigger_apps: [...focusAuto.trigger_apps, trimmed] })
    setNewApp('')
  }, [newApp, focusAuto, updateFocusAuto])

  const removeApp = useCallback(
    (app: string) => {
      updateFocusAuto({ trigger_apps: focusAuto.trigger_apps.filter((a) => a !== app) })
    },
    [focusAuto, updateFocusAuto],
  )

  const addSchedule = useCallback(() => {
    const entry: FocusScheduleSettings = { start: '09:00', end: '12:00', days: [] }
    updateFocusAuto({ trigger_schedules: [...focusAuto.trigger_schedules, entry] })
  }, [focusAuto, updateFocusAuto])

  const removeSchedule = useCallback(
    (index: number) => {
      updateFocusAuto({ trigger_schedules: focusAuto.trigger_schedules.filter((_, i) => i !== index) })
    },
    [focusAuto, updateFocusAuto],
  )

  const updateSchedule = useCallback(
    (index: number, patch: Partial<FocusScheduleSettings>) => {
      const updated = focusAuto.trigger_schedules.map((s, i) => (i === index ? { ...s, ...patch } : s))
      updateFocusAuto({ trigger_schedules: updated })
    },
    [focusAuto, updateFocusAuto],
  )

  return (
    <div className="space-y-6">
      {/* Enable toggle */}
      <label className="flex cursor-pointer items-center justify-between">
        <div>
          <span className={cn('text-sm', colors.text.primary)}>{t('focusAuto.enabled')}</span>
          <p className={cn('text-xs', colors.text.secondary)}>{t('focusAuto.description')}</p>
        </div>
        <input
          type="checkbox"
          className={form.checkbox}
          checked={focusAuto.enabled}
          onChange={(e) => updateFocusAuto({ enabled: e.target.checked })}
        />
      </label>

      {/* Duration + Cooldown */}
      <div className="grid grid-cols-2 gap-4">
        <div>
          <label htmlFor="focus-auto-duration" className={form.label}>
            {t('focusAuto.duration')}
          </label>
          <p className={cn('mb-2 text-xs', colors.text.secondary)}>{t('focusAuto.durationDesc')}</p>
          <select
            id="focus-auto-duration"
            className={cn(
              'w-full rounded-md border border-DEFAULT bg-surface-base px-3 py-2 text-sm',
              colors.text.primary,
            )}
            value={focusAuto.duration_minutes}
            onChange={(e) => updateFocusAuto({ duration_minutes: Number(e.target.value) })}
          >
            {DURATION_PRESETS.map((m) => (
              <option key={m} value={m}>
                {m} {t('focusAuto.minutes')}
              </option>
            ))}
          </select>
        </div>
        <div>
          <label htmlFor="focus-auto-cooldown" className={form.label}>
            {t('focusAuto.cooldown')}
          </label>
          <p className={cn('mb-2 text-xs', colors.text.secondary)}>{t('focusAuto.cooldownDesc')}</p>
          <select
            id="focus-auto-cooldown"
            className={cn(
              'w-full rounded-md border border-DEFAULT bg-surface-base px-3 py-2 text-sm',
              colors.text.primary,
            )}
            value={focusAuto.cooldown_secs / 60}
            onChange={(e) => updateFocusAuto({ cooldown_secs: Number(e.target.value) * 60 })}
          >
            {COOLDOWN_PRESETS.map((m) => (
              <option key={m} value={m}>
                {m} {t('focusAuto.minutes')}
              </option>
            ))}
          </select>
        </div>
      </div>

      {/* Trigger Apps */}
      <Card variant="default" padding="md">
        <CardTitle className="mb-1">{t('focusAuto.triggerApps')}</CardTitle>
        <p className={cn('mb-3 text-xs', colors.text.secondary)}>{t('focusAuto.triggerAppsDesc')}</p>

        <div className="flex flex-wrap gap-2">
          {focusAuto.trigger_apps.map((app) => (
            <span
              key={app}
              className={cn(
                'inline-flex items-center gap-1 rounded-full px-2.5 py-1 text-xs',
                'bg-brand/10 text-brand',
                typography.weight.medium,
              )}
            >
              {app}
              <button
                type="button"
                onClick={() => removeApp(app)}
                className={cn('rounded-full p-0.5 hover:bg-brand/20', motion.colors)}
                aria-label={`${t('common.remove')} ${app}`}
              >
                <X className="h-3 w-3" />
              </button>
            </span>
          ))}
        </div>

        <div className="mt-3 flex gap-2">
          <input
            type="text"
            value={newApp}
            onChange={(e) => setNewApp(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault()
                addApp()
              }
            }}
            placeholder={t('focusAuto.appPlaceholder')}
            className={cn(
              'flex-1 rounded-md border border-DEFAULT bg-surface-base px-3 py-1.5 text-sm',
              colors.text.primary,
              'placeholder:text-content-tertiary',
            )}
          />
          <button
            type="button"
            onClick={addApp}
            disabled={!newApp.trim()}
            className={cn(
              'inline-flex items-center gap-1 rounded-md px-3 py-1.5 text-sm',
              'bg-brand text-white',
              motion.colors,
              'disabled:cursor-not-allowed disabled:opacity-50',
            )}
          >
            <Plus className={iconSize.xs} />
            {t('focusAuto.addApp')}
          </button>
        </div>
      </Card>

      {/* Schedules */}
      <Card variant="default" padding="md">
        <CardTitle className="mb-1">{t('focusAuto.schedules')}</CardTitle>
        <p className={cn('mb-3 text-xs', colors.text.secondary)}>{t('focusAuto.schedulesDesc')}</p>

        <div className="space-y-2">
          {focusAuto.trigger_schedules.map((sched, i) => (
            <div key={`${sched.start}-${sched.end}-${sched.days.join(',')}`} className="flex items-center gap-2">
              <input
                type="time"
                value={sched.start}
                onChange={(e) => updateSchedule(i, { start: e.target.value })}
                className={cn(
                  'rounded-md border border-DEFAULT bg-surface-base px-2 py-1.5 text-sm',
                  colors.text.primary,
                )}
                aria-label={`Schedule ${i + 1} start time`}
              />
              <span className={cn('text-xs', colors.text.secondary)}>–</span>
              <input
                type="time"
                value={sched.end}
                onChange={(e) => updateSchedule(i, { end: e.target.value })}
                className={cn(
                  'rounded-md border border-DEFAULT bg-surface-base px-2 py-1.5 text-sm',
                  colors.text.primary,
                )}
                aria-label={`Schedule ${i + 1} end time`}
              />
              <select
                value={sched.days.length === 0 ? 'everyday' : sched.days.length === 5 ? 'weekdays' : 'custom'}
                onChange={(e) => {
                  const v = e.target.value
                  updateSchedule(i, {
                    days: v === 'everyday' ? [] : v === 'weekdays' ? ['Mon', 'Tue', 'Wed', 'Thu', 'Fri'] : sched.days,
                  })
                }}
                className={cn(
                  'rounded-md border border-DEFAULT bg-surface-base px-2 py-1.5 text-sm',
                  colors.text.primary,
                )}
                aria-label={`Schedule ${i + 1} days`}
              >
                <option value="everyday">{t('focusAuto.everyday')}</option>
                <option value="weekdays">{t('focusAuto.weekdays')}</option>
                <option value="custom">{t('focusAuto.custom')}</option>
              </select>
              <button
                type="button"
                onClick={() => removeSchedule(i)}
                className={cn('rounded p-1 hover:bg-surface-muted', motion.colors)}
                aria-label={`Remove schedule ${i + 1}`}
              >
                <X className={iconSize.sm} />
              </button>
            </div>
          ))}
        </div>

        <button
          type="button"
          onClick={addSchedule}
          className={cn(
            'mt-3 inline-flex items-center gap-1 text-sm',
            'text-brand hover:text-brand-hover',
            motion.colors,
          )}
        >
          <Plus className={iconSize.xs} />
          {t('focusAuto.addSchedule')}
        </button>
      </Card>
    </div>
  )
}
