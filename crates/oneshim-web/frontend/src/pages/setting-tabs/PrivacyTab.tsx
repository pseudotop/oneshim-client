import { useSettingsFormContext } from '../settings/SettingsFormContext'
import PrivacySettings from './PrivacySettings'

export default function PrivacyTab() {
  const { form } = useSettingsFormContext()

  return (
    <div id="section-privacy">
      <PrivacySettings privacy={form.formData!.privacy} onChange={form.handlePrivacyChange} />
    </div>
  )
}
