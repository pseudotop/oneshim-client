import type { PrivacySettings as PrivacySettingsType } from '../../api/client'
import PrivacySettings from './PrivacySettings'
import type { SettingsFormTabProps } from './types'

interface PrivacyTabProps extends SettingsFormTabProps {
  onPrivacyChange: (field: keyof PrivacySettingsType, value: boolean | string | string[]) => void
}

export default function PrivacyTab({ formData, onPrivacyChange }: PrivacyTabProps) {
  return (
    <div id="section-privacy">
      <PrivacySettings privacy={formData.privacy} onChange={onPrivacyChange} />
    </div>
  )
}
