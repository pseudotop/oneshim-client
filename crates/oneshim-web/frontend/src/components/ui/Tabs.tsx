import { cn } from '../../utils/cn'

export interface Tab {
  id: string
  label: string
  icon?: React.ReactNode
}

export interface TabsProps {
  tabs: Tab[]
  activeTab: string
  onTabChange: (id: string) => void
  className?: string
}

export function Tabs({ tabs, activeTab, onTabChange, className }: TabsProps) {
  return (
    <div
      role="tablist"
      className={cn('flex gap-1 rounded-lg bg-surface-muted p-1', className)}
    >
      {tabs.map((tab) => (
        <button
          key={tab.id}
          role="tab"
          aria-selected={activeTab === tab.id}
          onClick={() => onTabChange(tab.id)}
          className={cn(
            'flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm font-medium transition-colors',
            activeTab === tab.id
              ? 'bg-surface-elevated text-content shadow-sm'
              : 'text-content-secondary hover:text-content hover:bg-surface-elevated/50',
          )}
        >
          {tab.icon}
          {tab.label}
        </button>
      ))}
    </div>
  )
}
