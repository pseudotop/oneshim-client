import { useTranslation } from 'react-i18next'
import UpdatePanel from '../components/UpdatePanel'
import { Card, CardTitle } from '../components/ui'
import { colors, typography } from '../styles/tokens'
import { cn } from '../utils/cn'

export default function Updates() {
  const { t } = useTranslation()

  return (
    <div className="min-h-full space-y-6 p-6">
      <div className="flex items-center justify-between">
        <h1 className={cn(typography.h1, colors.text.primary)}>{t('updates.title')}</h1>
      </div>

      <div id="section-status">
        <UpdatePanel />
      </div>

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
