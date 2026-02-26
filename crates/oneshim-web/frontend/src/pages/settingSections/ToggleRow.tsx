/**
 *
 */
import { colors, form } from '../../styles/tokens'

export interface ToggleRowProps {
  label: string
  description: string
  checked: boolean
  onChange: (checked: boolean) => void
}

export default function ToggleRow({ label, description, checked, onChange }: ToggleRowProps) {
  return (
    <label className="flex items-center justify-between cursor-pointer">
      <div>
        <span className={colors.text.secondary}>{label}</span>
        <p className={colors.text.tertiary}>{description}</p>
      </div>
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        className={form.checkbox}
      />
    </label>
  )
}
