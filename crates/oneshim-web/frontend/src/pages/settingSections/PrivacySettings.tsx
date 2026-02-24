/**
 *
 */
import { useTranslation } from 'react-i18next'
import { Card, CardTitle, Input, Select } from '../../components/ui'
import { form } from '../../styles/tokens'
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
          <label className={form.label}>
            {t('settings.piiLevel')}
          </label>
          <Select
            value={privacy.pii_filter_level}
            onChange={(e) => onChange('pii_filter_level', e.target.value)}
          >
            <option value="Off">{t('settings.piiOff')}</option>
            <option value="Basic">{t('settings.piiBasic')}</option>
            <option value="Standard">{t('settings.piiStandard')}</option>
            <option value="Strict">{t('settings.piiStrict')}</option>
          </Select>
          <p className={form.helper}>{t('settings.piiDesc')}</p>
        </div>

        <div>
          <label className={form.label}>
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
          <label className={form.label}>
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
          <p className={form.helper}>{t('settings.wildcardHint')}</p>
        </div>

        <div>
          <label className={form.label}>
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
