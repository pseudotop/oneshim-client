/**
 *
 */
import { ReactNode } from 'react'
import { cn } from '../utils/cn'
import { Card } from './ui'
import { statColorMap } from '../styles/variants'
import { colors, radius, typography } from '../styles/tokens'

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
        <div className={cn('p-2', radius.md, statColorMap[color])}>
          {icon}
        </div>
        <div>
          <div className={cn(typography.stat.large, colors.text.primary)}>{value}</div>
          <div className={cn(typography.body, colors.text.secondary)}>{title}</div>
        </div>
      </div>
    </Card>
  )
}
