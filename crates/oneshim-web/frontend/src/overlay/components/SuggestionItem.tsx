import type { SuggestionViewDto } from '../types'

interface SuggestionItemProps {
  item: SuggestionViewDto
  onAction: (id: string, action: 'accept' | 'reject' | 'defer') => void
}

const priorityClasses: Record<string, string> = {
  critical: 'bg-semantic-error/20 text-semantic-error',
  high: 'bg-semantic-warning/20 text-semantic-warning',
  medium: 'bg-brand/20 text-brand',
  low: 'bg-content-secondary/20 text-content-secondary',
}

export function SuggestionItem({ item, onAction }: SuggestionItemProps) {
  const badgeClass = priorityClasses[item.priority] ?? priorityClasses.low

  return (
    <div className="px-4 py-3 border-b border-content-inverse/5">
      <div className="flex items-start justify-between gap-2">
        <span className="text-sm font-medium text-content leading-tight">
          {item.title}
        </span>
        <span className={`shrink-0 inline-flex items-center px-1.5 py-0.5 text-[10px] font-semibold rounded ${badgeClass}`}>
          {item.priority}
        </span>
      </div>
      <p className="text-xs text-content-secondary mt-1 line-clamp-2">
        {item.body}
      </p>
      <div className="flex items-center gap-1.5 mt-2">
        <button
          onClick={() => onAction(item.id, 'accept')}
          className="rounded-md px-2 py-1 text-xs bg-semantic-success/15 text-semantic-success hover:bg-semantic-success/25 transition-colors"
        >
          Accept
        </button>
        <button
          onClick={() => onAction(item.id, 'reject')}
          className="rounded-md px-2 py-1 text-xs bg-semantic-error/15 text-semantic-error hover:bg-semantic-error/25 transition-colors"
        >
          Reject
        </button>
        <button
          onClick={() => onAction(item.id, 'defer')}
          className="rounded-md px-2 py-1 text-xs bg-content-inverse/10 text-content-secondary hover:bg-content-inverse/15 transition-colors"
        >
          Later
        </button>
        <span className="ml-auto text-[10px] text-content-tertiary">
          {item.source}
        </span>
      </div>
    </div>
  )
}
