/**
 * 프라이버시 설정 섹션 컴포넌트
 *
 * 민감 앱 자동 제외, PII 필터 레벨, 제외 앱/패턴 설정
 */
import { useTranslation } from 'react-i18next'
import { Card, CardTitle, Input } from '../../components/ui'
import type { PrivacySettings as PrivacySettingsType } from '../../api/client'
import ToggleRow from './ToggleRow'

interface PrivacySettingsProps {
  privacy: PrivacySettingsType
  onChange: (field: keyof PrivacySettingsType, value: boolean | string | string[]) => void
}

export default function PrivacySettings({ privacy, onChange }: PrivacySettingsProps) {
  const { t } = useTranslation()

  return (
    <Card variant="default" padding="lg">
      <CardTitle className="mb-4">{t('settings.privacyTitle')}</CardTitle>
      <div className="space-y-4">
        <ToggleRow
          label={t('settings.autoExclude')}
          description={t('settings.autoExcludeDesc')}
          checked={privacy.auto_exclude_sensitive}
          onChange={(v) => onChange('auto_exclude_sensitive', v)}
        />

        <div>
          <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
            {t('settings.piiLevel')}
          </label>
          <select
            value={privacy.pii_filter_level}
            onChange={(e) => onChange('pii_filter_level', e.target.value)}
            className="w-full px-3 py-2 rounded-md border border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800 text-slate-900 dark:text-white focus:ring-teal-500 focus:border-teal-500"
          >
            <option value="Off">{t('settings.piiOff')}</option>
            <option value="Basic">{t('settings.piiBasic')}</option>
            <option value="Standard">{t('settings.piiStandard')}</option>
            <option value="Strict">{t('settings.piiStrict')}</option>
          </select>
          <p className="mt-1 text-xs text-slate-600 dark:text-slate-500">{t('settings.piiDesc')}</p>
        </div>

        <div>
          <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
            {t('settings.excludedApps')}
          </label>
          <Input
            type="text"
            value={privacy.excluded_apps.join(', ')}
            onChange={(e) => onChange(
              'excluded_apps',
              e.target.value.split(',').map(s => s.trim()).filter(Boolean)
            )}
            placeholder="1Password, Discord, Slack"
          />
        </div>

        <div>
          <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
            {t('settings.excludedAppPatterns')}
          </label>
          <Input
            type="text"
            value={privacy.excluded_app_patterns.join(', ')}
            onChange={(e) => onChange(
              'excluded_app_patterns',
              e.target.value.split(',').map(s => s.trim()).filter(Boolean)
            )}
            placeholder="*bank*, *wallet*, *crypto*"
          />
          <p className="mt-1 text-xs text-slate-600 dark:text-slate-500">{t('settings.wildcardHint')}</p>
        </div>

        <div>
          <label className="block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2">
            {t('settings.excludedTitlePatterns')}
          </label>
          <Input
            type="text"
            value={privacy.excluded_title_patterns.join(', ')}
            onChange={(e) => onChange(
              'excluded_title_patterns',
              e.target.value.split(',').map(s => s.trim()).filter(Boolean)
            )}
            placeholder="*password*, *secret*, *private*"
          />
        </div>
      </div>
    </Card>
  )
}
