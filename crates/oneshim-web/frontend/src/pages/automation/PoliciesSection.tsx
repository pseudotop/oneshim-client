import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { fetchAutomationContracts, fetchPolicies } from '../../api/client'
import { Card, CardContent, CardHeader, CardTitle } from '../../components/ui/Card'
import { typography } from '../../styles/tokens'

export default function PoliciesSection() {
  const { t } = useTranslation()

  const { data: policies } = useQuery({
    queryKey: ['policies'],
    queryFn: fetchPolicies,
  })

  const { data: contracts } = useQuery({
    queryKey: ['automationContracts'],
    queryFn: fetchAutomationContracts,
  })

  return (
    <Card id="section-policies">
      <CardHeader>
        <CardTitle>{t('automation.policies')}</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="grid grid-cols-1 gap-4 text-sm md:grid-cols-2 lg:grid-cols-3">
          <div>
            <div className="text-content-secondary">{t('automation.automationEnabled')}</div>
            <div className={`mt-1 ${typography.weight.medium} text-content`}>
              {policies?.automation_enabled ? t('automation.enabled') : t('automation.disabled')}
            </div>
          </div>
          <div>
            <div className="text-content-secondary">{t('automation.sandboxProfile')}</div>
            <div className={`mt-1 ${typography.weight.medium} text-content`}>{policies?.sandbox_profile ?? '-'}</div>
          </div>
          <div>
            <div className="text-content-secondary">{t('automation.sandboxEnabled')}</div>
            <div className={`mt-1 ${typography.weight.medium} text-content`}>
              {policies?.sandbox_enabled ? t('automation.enabled') : t('automation.disabled')}
            </div>
          </div>
          <div>
            <div className="text-content-secondary">{t('automation.allowNetwork')}</div>
            <div className={`mt-1 ${typography.weight.medium} text-content`}>
              {policies?.allow_network ? t('automation.enabled') : t('automation.disabled')}
            </div>
          </div>
          <div>
            <div className="text-content-secondary">{t('automation.dataPolicy')}</div>
            <div className={`mt-1 ${typography.weight.medium} text-content`}>
              {policies?.external_data_policy ?? '-'}
            </div>
          </div>
          <div>
            <div className="text-content-secondary">{t('automation.sceneOverride')}</div>
            <div className={`mt-1 ${typography.weight.medium} text-content`}>
              {policies?.scene_action_override_active
                ? t('automation.active')
                : policies?.scene_action_override_enabled
                  ? t('automation.pending')
                  : t('automation.disabled')}
            </div>
          </div>
          <div>
            <div className="text-content-secondary">{t('automation.sceneOverrideExpires')}</div>
            <div className={`mt-1 ${typography.weight.medium} text-content`}>
              {policies?.scene_action_override_expires_at
                ? new Date(policies.scene_action_override_expires_at).toLocaleString()
                : '-'}
            </div>
          </div>
          <div>
            <div className="text-content-secondary">{t('automation.sceneOverrideIssue')}</div>
            <div className={`mt-1 ${typography.weight.medium} text-content`}>
              {policies?.scene_action_override_issue || '-'}
            </div>
          </div>
          <div>
            <div className="text-content-secondary">{t('automation.sceneSchemaVersion')}</div>
            <div className={`mt-1 ${typography.weight.medium} text-content`}>
              {contracts?.scene_schema_version ?? '-'}
            </div>
          </div>
          <div>
            <div className="text-content-secondary">{t('automation.auditSchemaVersion')}</div>
            <div className={`mt-1 ${typography.weight.medium} text-content`}>
              {contracts?.audit_schema_version ?? '-'}
            </div>
          </div>
          <div>
            <div className="text-content-secondary">{t('automation.sceneActionSchemaVersion')}</div>
            <div className={`mt-1 ${typography.weight.medium} text-content`}>
              {contracts?.scene_action_schema_version ?? '-'}
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  )
}
