// 빈 상태 안내 컴포넌트

import type { ReactNode } from 'react'

export interface EmptyStateProps {
  /** 중앙 아이콘 */
  icon: ReactNode
  /** 제목 */
  title: string
  /** 설명 */
  description: string
  /** 선택적 행동 버튼 */
  action?: {
    label: string
    onClick: () => void
  }
}

export function EmptyState({ icon, title, description, action }: EmptyStateProps) {
  return (
    <div className="flex flex-col items-center justify-center py-16 px-6">
      <div className="flex items-center justify-center w-16 h-16 rounded-full bg-slate-100 dark:bg-slate-800 text-slate-400 dark:text-slate-500 mb-4">
        {icon}
      </div>
      <h3 className="text-lg font-semibold text-slate-700 dark:text-slate-300 mb-2">
        {title}
      </h3>
      <p className="text-sm text-slate-500 dark:text-slate-400 text-center max-w-md mb-4">
        {description}
      </p>
      {action && (
        <button
          onClick={action.onClick}
          className="px-4 py-2 text-sm font-medium rounded-lg bg-teal-500 text-white hover:bg-teal-600 transition-colors"
        >
          {action.label}
        </button>
      )}
    </div>
  )
}
