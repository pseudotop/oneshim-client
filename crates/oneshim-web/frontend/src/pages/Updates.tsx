import { useMutation } from '@tanstack/react-query'
import { RefreshCw, Shield, Clock, RotateCcw } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { postUpdateAction } from '../api/client'
import UpdatePanel from '../components/UpdatePanel'
import { Badge, Button, Card, CardTitle } from '../components/ui'
import { useToast } from '../hooks/useToast'
import { colors, typography } from '../styles/tokens'
import { cn } from '../utils/cn'

declare const __APP_VERSION__: string

const APP_VERSION = typeof __APP_VERSION__ !== 'undefined' ? __APP_VERSION__ : 'v0.3.6'

const FEATURE_ICONS = [RefreshCw, Shield, RotateCcw, Clock] as const

export default function Updates() {
  const { t } = useTranslation()
  const toast = useToast()

  const checkMutation = useMutation({
    mutationFn: () => postUpdateAction('CheckNow'),
    onSuccess: () => {
      toast.show('success', t('updates.checkSuccess'))
    },
    onError: (error: Error) => {
      toast.show('error', error.message)
    },
  })

  const featureKeys = [
    'updates.featureAuto',
    'updates.featureIntegrity',
    'updates.featureRollback',
    'updates.featureMinimal',
  ] as const

  return (
    <div className="min-h-full space-y-6 p-6">
      {/* Header with version badge and check button */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h1 className={cn(typography.h1, colors.text.primary)}>{t('updates.title')}</h1>
          <Badge color="info">{APP_VERSION}</Badge>
        </div>
        <Button
          variant="secondary"
          size="md"
          isLoading={checkMutation.isPending}
          onClick={() => checkMutation.mutate()}
        >
          {checkMutation.isPending ? t('updates.checking') : t('updates.checkNow')}
        </Button>
      </div>

      {/* Current update status */}
      <div id="section-status">
        <UpdatePanel />
      </div>

      {/* Update features card */}
      <Card id="section-features" variant="default" padding="lg">
        <CardTitle className="mb-4">{t('updates.featuresTitle')}</CardTitle>
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
          {featureKeys.map((key, i) => {
            const Icon = FEATURE_ICONS[i]
            return (
              <div key={key} className="flex items-start gap-3 rounded-lg bg-surface-muted p-3">
                <div className="mt-0.5 shrink-0 text-accent-teal">
                  <Icon size={18} />
                </div>
                <span className="text-content-strong text-sm">{t(key)}</span>
              </div>
            )
          })}
        </div>
      </Card>

      {/* Policy card (existing) */}
      <Card id="section-history" variant="default" padding="lg">
        <CardTitle className="mb-3">{t('updates.policyTitle')}</CardTitle>
        <ul className="space-y-1 text-content-strong text-sm">
          <li>{t('updates.policyIntegrity')}</li>
          <li>{t('updates.policySignature')}</li>
          <li>{t('updates.policyRollback')}</li>
        </ul>
      </Card>
    </div>
  )
}
