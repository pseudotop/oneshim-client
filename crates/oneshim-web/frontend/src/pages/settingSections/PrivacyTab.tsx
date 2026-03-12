/**
 * Privacy settings tab — wraps existing PrivacySettings with self-contained state.
 */
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  type AppSettings,
  type PrivacySettings as PrivacySettingsType,
  fetchSettings,
  updateSettings,
} from '../../api/client'
import { Button, Spinner } from '../../components/ui'
import { useToast } from '../../hooks/useToast'
import { colors } from '../../styles/tokens'
import { cn } from '../../utils/cn'
import PrivacySettings from './PrivacySettings'

export default function PrivacyTab() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const toast = useToast()

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

  const handlePrivacyChange = (field: keyof PrivacySettingsType, value: boolean | string | string[]) => {
    if (formData) {
      setFormData({
        ...formData,
        privacy: { ...formData.privacy, [field]: value },
      })
    }
  }

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (formData) {
      mutation.mutate(formData)
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
      <PrivacySettings privacy={formData.privacy} onChange={handlePrivacyChange} />

      <div className="flex justify-end">
        <Button type="submit" variant="primary" size="lg" isLoading={mutation.isPending}>
          {mutation.isPending ? t('settings.saving') : t('settings.saveSettings')}
        </Button>
      </div>
    </form>
  )
}
