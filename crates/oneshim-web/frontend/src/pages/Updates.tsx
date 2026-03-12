import { Clock, RefreshCw, RotateCcw, Shield } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import UpdatePanel from '../components/UpdatePanel'
import { Badge, Card, CardTitle } from '../components/ui'
import { colors, typography } from '../styles/tokens'
import { cn } from '../utils/cn'

declare const __APP_VERSION__: string

const FEATURE_ICONS = [RefreshCw, Shield, RotateCcw, Clock] as const

export default function Updates() {
  const { t } = useTranslation()
  const featureKeys = [
    'updates.featureAuto',
    'updates.featureIntegrity',
    'updates.featureRollback',
    'updates.featureMinimal',
  ] as const

  return (
    <div className="min-h-full space-y-6 p-6">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h1 className={cn(typography.h1, colors.text.primary)}>{t('updates.title')}</h1>
          <Badge color="info">{__APP_VERSION__}</Badge>
        </div>
      </div>

      <div id="section-status">
        <UpdatePanel />
      </div>

      <Card id="section-features" variant="default" padding="lg">
        <CardTitle className="mb-4">{t('updates.featuresTitle')}</CardTitle>
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
          {featureKeys.map((key, index) => {
            const Icon = FEATURE_ICONS[index]

            return (
              <div key={key} className="flex items-start gap-3 rounded-lg bg-surface-muted p-3">
                <div className="mt-0.5 shrink-0 text-accent-teal">
                  <Icon size={18} aria-hidden="true" />
                </div>
                <span className="text-content-strong text-sm">{t(key)}</span>
              </div>
            )
          })}
        </div>
      </Card>

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
