/**
 * General settings tab: language, notifications, schedule, updates, web dashboard.
 */
import { DEFAULT_WEB_PORT } from '../../constants'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  type AppSettings,
  type NotificationSettings as NotificationSettingsType,
  type ScheduleSettings as ScheduleSettingsType,
  fetchSettings,
  fetchUpdateStatus,
  postUpdateAction,
  type UpdateAction,
  type UpdateStatus,
  updateSettings,
} from '../../api/client'
import LanguageSelector from '../../components/LanguageSelector'
import { Button, Card, CardTitle, Input, Spinner } from '../../components/ui'
import { useToast } from '../../hooks/useToast'
import { colors, form } from '../../styles/tokens'
import { cn } from '../../utils/cn'
import NotificationSettings from './NotificationSettings'
import ScheduleSettings from './ScheduleSettings'
import ToggleRow from './ToggleRow'

export default function GeneralTab() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const toast = useToast()

  const { data: settings, isLoading: settingsLoading } = useQuery({
    queryKey: ['settings'],
    queryFn: fetchSettings,
  })

  const { data: updateStatus } = useQuery<UpdateStatus>({
    queryKey: ['update-status'],
    queryFn: fetchUpdateStatus,
    refetchInterval: 15000,
    retry: 1,
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

  const updateActionMutation = useMutation({
    mutationFn: (action: UpdateAction) => postUpdateAction(action),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['update-status'] })
      toast.show('success', t('settings.updateActionSuccess'))
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

  const handleNotificationChange = (field: keyof NotificationSettingsType, value: number | boolean) => {
    if (formData) {
      setFormData({
        ...formData,
        notification: { ...formData.notification, [field]: value },
      })
    }
  }

  const handleScheduleChange = (field: keyof ScheduleSettingsType, value: boolean | number | string[]) => {
    if (formData) {
      setFormData({
        ...formData,
        schedule: { ...formData.schedule, [field]: value },
      })
    }
  }

  const handleUpdateChange = (field: keyof AppSettings['update'], value: boolean | number) => {
    if (formData) {
      setFormData({
        ...formData,
        update: { ...formData.update, [field]: value },
      })
    }
  }

  const updateSectionDirty = Boolean(
    formData && settings && JSON.stringify(formData.update) !== JSON.stringify(settings.update),
  )

  const saveUpdateSection = () => {
    if (!formData) {
      return
    }

    const normalizedInterval = Math.max(1, Math.min(168, formData.update.check_interval_hours))
    mutation.mutate({
      ...formData,
      update: {
        ...formData.update,
        check_interval_hours: normalizedInterval,
      },
    })
  }

  if (settingsLoading) {
    return (
      <div className="flex h-64 items-center justify-center">
        <Spinner size="lg" className={colors.primary.text} />
        <span className={cn('ml-3', colors.text.secondary)}>{t('common.loading')}</span>
      </div>
    )
  }

  return (
    <>
      {/* Language selector */}
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('settings.language')}</CardTitle>
        <LanguageSelector />
      </Card>

      {formData && (
        <form onSubmit={handleSubmit} className="space-y-6">
          {/* Web Dashboard */}
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
                  onChange={(e) => handleChange('web_port', parseInt(e.target.value, 10) || DEFAULT_WEB_PORT)}
                />
                <p className={form.helper}>{t('settings.portRestart')}</p>
              </div>
              <div className="flex items-center">
                <label className="flex cursor-pointer items-center">
                  <input
                    type="checkbox"
                    checked={formData.allow_external}
                    onChange={(e) => handleChange('allow_external', e.target.checked)}
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

          {/* Notifications */}
          <div id="section-notification">
            <NotificationSettings notification={formData.notification} onChange={handleNotificationChange} />
          </div>

          {/* Schedule */}
          <div id="section-schedule">
            <ScheduleSettings schedule={formData.schedule} onChange={handleScheduleChange} />
          </div>

          {/* Updates */}
          <Card variant="default" padding="lg">
            <CardTitle className="mb-4">{t('settings.updateTitle')}</CardTitle>
            <div className="space-y-4">
              <ToggleRow
                label={t('settings.updateEnabled')}
                description={t('settings.updateEnabledDesc')}
                checked={formData.update.enabled}
                onChange={(v) => handleUpdateChange('enabled', v)}
              />

              <ToggleRow
                label={t('settings.updateAutoInstall')}
                description={t('settings.updateAutoInstallDesc')}
                checked={formData.update.auto_install}
                onChange={(v) => handleUpdateChange('auto_install', v)}
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
                    onChange={(e) => handleUpdateChange('check_interval_hours', parseInt(e.target.value, 10) || 24)}
                  />
                </div>
                <div className="flex items-end">
                  <label className="flex cursor-pointer items-center">
                    <input
                      type="checkbox"
                      checked={formData.update.include_prerelease}
                      onChange={(e) => handleUpdateChange('include_prerelease', e.target.checked)}
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
                    isLoading={updateActionMutation.isPending}
                    onClick={() => updateActionMutation.mutate('CheckNow')}
                  >
                    {t('settings.updateCheckNow')}
                  </Button>
                  <Button
                    type="button"
                    variant="primary"
                    size="sm"
                    isLoading={updateActionMutation.isPending}
                    onClick={() => updateActionMutation.mutate('Approve')}
                  >
                    {t('settings.updateApproveNow')}
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    isLoading={updateActionMutation.isPending}
                    onClick={() => updateActionMutation.mutate('Defer')}
                  >
                    {t('settings.updateDefer')}
                  </Button>
                </div>

                <div className="mt-4 flex justify-end">
                  <Button
                    type="button"
                    variant="primary"
                    size="sm"
                    isLoading={mutation.isPending}
                    disabled={!updateSectionDirty || mutation.isPending}
                    onClick={saveUpdateSection}
                  >
                    {t('settings.saveSettings')}
                  </Button>
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
      )}
    </>
  )
}
