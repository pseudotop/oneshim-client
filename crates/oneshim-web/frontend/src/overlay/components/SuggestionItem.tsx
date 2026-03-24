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
    <div className="border-content-inverse/5 border-b px-4 py-3">
      <div className="flex items-start justify-between gap-2">
        <span className="font-medium text-content text-sm leading-tight">{item.title}</span>
        <span
          className={`inline-flex shrink-0 items-center rounded px-1.5 py-0.5 font-semibold text-[10px] ${badgeClass}`}
        >
          {item.priority}
        </span>
      </div>
      <p className="mt-1 line-clamp-2 text-content-secondary text-xs">{item.body}</p>
      <div className="mt-2 flex items-center gap-1.5">
        <button
          type="button"
          onClick={() => onAction(item.id, 'accept')}
          className="rounded-md bg-semantic-success/15 px-2 py-1 text-semantic-success text-xs transition-colors hover:bg-semantic-success/25"
        >
          Accept
        </button>
        <button
          type="button"
          onClick={() => onAction(item.id, 'reject')}
          className="rounded-md bg-semantic-error/15 px-2 py-1 text-semantic-error text-xs transition-colors hover:bg-semantic-error/25"
        >
          Reject
        </button>
        <button
          type="button"
          onClick={() => onAction(item.id, 'defer')}
          className="rounded-md bg-content-inverse/10 px-2 py-1 text-content-secondary text-xs transition-colors hover:bg-content-inverse/15"
        >
          Later
        </button>
        <span className="ml-auto text-[10px] text-content-tertiary">{item.source}</span>
      </div>
    </div>
  )
}
