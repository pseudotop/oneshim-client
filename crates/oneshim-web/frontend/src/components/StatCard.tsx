/**
 * 통계 카드 컴포넌트
 *
 * Dashboard에서 주요 지표 표시에 사용
 */
import { ReactNode } from 'react'
import { cn } from '../utils/cn'
import { Card } from './ui'
import { statColorMap } from '../styles/variants'

export interface StatCardProps {
  title: string
  value: string
  icon: ReactNode
  color: keyof typeof statColorMap
}

export default function StatCard({ title, value, icon, color }: StatCardProps) {
  return (
    <Card variant="default" padding="md">
      <div className="flex items-center space-x-3">
        <div className={cn('p-2 rounded-lg', statColorMap[color])}>
          {icon}
        </div>
        <div>
          <div className="text-2xl font-bold text-slate-900 dark:text-white">{value}</div>
          <div className="text-sm text-slate-600 dark:text-slate-400">{title}</div>
        </div>
      </div>
    </Card>
  )
}
