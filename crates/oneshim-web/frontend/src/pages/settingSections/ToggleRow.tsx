/**
 * 토글 행 공용 컴포넌트
 *
 * 설정 섹션 내 체크박스 토글 행 (라벨 + 설명 + 체크박스)
 */

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
        <span className="text-slate-700 dark:text-slate-300">{label}</span>
        <p className="text-xs text-slate-600 dark:text-slate-500">{description}</p>
      </div>
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        className="w-5 h-5 rounded bg-slate-900 border-slate-700 text-teal-500 focus:ring-teal-500"
      />
    </label>
  )
}
