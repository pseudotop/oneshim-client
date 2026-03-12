/**
 *
 */
import type { ReactNode } from 'react'
import { colors, radius, typography } from '../styles/tokens'
import { statColorMap } from '../styles/variants'
import { cn } from '../utils/cn'
import { Card } from './ui'

export interface StatCardProps {
  title: string
  value: string
  icon: ReactNode
  color: keyof typeof statColorMap
  'data-testid'?: string
}

export default function StatCard({ title, value, icon, color, 'data-testid': testId }: StatCardProps) {
  return (
    <Card variant="default" padding="md" data-testid={testId}>
      <div className="flex items-center space-x-3">
        <div className={cn('p-2', radius.md, statColorMap[color])}>{icon}</div>
        <div>
          <div className={cn(typography.stat.large, colors.text.primary)}>{value}</div>
          <div className={cn(typography.body, colors.text.secondary)}>{title}</div>
        </div>
      </div>
    </Card>
  )
}
