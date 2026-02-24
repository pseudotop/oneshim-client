/**
 * 토글 행 공용 컴포넌트
 *
 * 설정 섹션 내 체크박스 토글 행 (라벨 + 설명 + 체크박스)
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
