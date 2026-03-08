import type { ReactNode } from 'react'
import { Button } from './Button'

export interface EmptyStateProps {
  icon: ReactNode
  title: string
  description: string
  action?: {
    label: string
    onClick: () => void
  }
}

export function EmptyState({ icon, title, description, action }: EmptyStateProps) {
  return (
    <div role="region" aria-label="Empty state" className="flex flex-col items-center justify-center px-6 py-16">
      <div className="mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-surface-elevated text-content-muted">
        {icon}
      </div>
      <h3 className="mb-2 font-semibold text-content-strong text-lg">{title}</h3>
      <p className="mb-4 max-w-md text-center text-content-secondary text-sm">{description}</p>
      {action && (
        <Button variant="primary" size="md" onClick={action.onClick}>
          {action.label}
        </Button>
      )}
    </div>
  )
}
