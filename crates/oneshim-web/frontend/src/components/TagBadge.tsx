/**
 * 태그 배지 컴포넌트
 *
 * 단일 태그를 색상과 함께 표시하며 삭제 버튼 옵션 제공
 */
import { cn } from '../utils/cn'

interface TagBadgeProps {
  /** 태그 이름 */
  name: string
  /** 태그 색상 (hex) */
  color: string
  /** 삭제 버튼 표시 여부 */
  onRemove?: () => void
  /** 클릭 핸들러 */
  onClick?: () => void
  /** 선택됨 상태 */
  selected?: boolean
  /** 크기 */
  size?: 'sm' | 'md'
}

export function TagBadge({
  name,
  color,
  onRemove,
  onClick,
  selected = false,
  size = 'md',
}: TagBadgeProps) {
  // hex 색상을 배경과 테두리에 사용
  const bgColor = `${color}20` // 투명도 추가
  const borderColor = selected ? color : `${color}60`

  const sizeClasses = {
    sm: 'px-1.5 py-0.5 text-xs',
    md: 'px-2 py-1 text-sm',
  }

  return (
    <span
      className={cn(
        'inline-flex items-center gap-1 rounded-full font-medium border transition-all',
        sizeClasses[size],
        onClick && 'cursor-pointer hover:opacity-80',
        selected && 'ring-2 ring-offset-1'
      )}
      style={{
        backgroundColor: bgColor,
        borderColor: borderColor,
        color: color,
        ...(selected && { ringColor: color }),
      }}
      onClick={onClick}
    >
      <span
        className="w-2 h-2 rounded-full"
        style={{ backgroundColor: color }}
      />
      {name}
      {onRemove && (
        <button
          type="button"
          className="ml-0.5 hover:opacity-70 focus:outline-none"
          onClick={(e) => {
            e.stopPropagation()
            onRemove()
          }}
          aria-label={`${name} 태그 삭제`}
        >
          <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      )}
    </span>
  )
}

/** 기본 태그 색상 팔레트 */
export const TAG_COLORS = [
  '#3b82f6', // blue
  '#ef4444', // red
  '#22c55e', // green
  '#f59e0b', // amber
  '#8b5cf6', // violet
  '#ec4899', // pink
  '#14b8a6', // teal
  '#f97316', // orange
  '#6366f1', // indigo
  '#84cc16', // lime
]

/** 랜덤 태그 색상 선택 */
export function getRandomTagColor(): string {
  return TAG_COLORS[Math.floor(Math.random() * TAG_COLORS.length)]
}
