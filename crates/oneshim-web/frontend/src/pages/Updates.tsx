import { useTranslation } from 'react-i18next'
import { Card, CardTitle } from '../components/ui'
import UpdatePanel from '../components/UpdatePanel'

export default function Updates() {
  const { t } = useTranslation()

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold text-slate-900 dark:text-white">{t('updates.title')}</h1>
      </div>

      <UpdatePanel />

      <Card variant="default" padding="lg">
        <CardTitle className="mb-3">{t('updates.policyTitle')}</CardTitle>
        <ul className="text-sm text-slate-700 dark:text-slate-300 space-y-1">
          <li>{t('updates.policyIntegrity')}</li>
          <li>{t('updates.policySignature')}</li>
          <li>{t('updates.policyRollback')}</li>
        </ul>
      </Card>
    </div>
  )
}
