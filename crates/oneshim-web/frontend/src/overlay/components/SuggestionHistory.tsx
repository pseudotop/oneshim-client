import { useEffect, useState } from 'react'
import { cn } from '../../utils/cn'
import type { SuggestionHistoryDto } from '../types'

const feedbackBadge: Record<string, { label: string; className: string }> = {
  accepted: { label: 'Accepted', className: 'bg-semantic-success/20 text-semantic-success' },
  rejected: { label: 'Rejected', className: 'bg-semantic-error/20 text-semantic-error' },
  deferred: { label: 'Snoozed', className: 'bg-semantic-warning/20 text-semantic-warning' },
}

export function SuggestionHistory() {
  const [entries, setEntries] = useState<SuggestionHistoryDto[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core')
        const result = await invoke<SuggestionHistoryDto[]>('get_suggestion_history', { limit: 50 })
        if (!cancelled) setEntries(result)
      } catch (e) {
        console.warn('Failed to load history:', e)
      } finally {
        if (!cancelled) setLoading(false)
      }
    })()
    return () => { cancelled = true }
  }, [])

  if (loading) {
    return <p className="text-content-secondary text-xs p-4">Loading...</p>
  }

  if (entries.length === 0) {
    return <p className="text-content-secondary text-xs p-4">No history yet</p>
  }

  const stats = {
    accepted: entries.filter(e => e.feedback === 'accepted').length,
    rejected: entries.filter(e => e.feedback === 'rejected').length,
    deferred: entries.filter(e => e.feedback === 'deferred').length,
    pending: entries.filter(e => !e.feedback).length,
  }

  return (
    <div className="flex flex-col gap-2 p-2">
      <div className="flex gap-3 text-xs text-content-secondary px-2 pb-2 border-b border-border-default">
        <span>{stats.accepted} accepted</span>
        <span>{stats.rejected} rejected</span>
        <span>{stats.deferred} snoozed</span>
        <span>{stats.pending} pending</span>
      </div>
      <ul className="flex flex-col gap-1.5">
        {entries.map(entry => {
          const badge = entry.feedback ? feedbackBadge[entry.feedback] : null
          return (
            <li
              key={entry.id}
              className="px-3 py-2 rounded-lg bg-surface-default/60 text-xs"
            >
              <div className="flex items-center justify-between gap-2">
                <span className="font-medium text-content-primary truncate">
                  {entry.title}
                </span>
                {badge && (
                  <span className={cn('px-1.5 py-0.5 rounded text-[10px] font-medium shrink-0', badge.className)}>
                    {badge.label}
                  </span>
                )}
              </div>
              <p className="text-content-secondary line-clamp-1 mt-0.5">
                {entry.body}
              </p>
            </li>
          )
        })}
      </ul>
    </div>
  )
}
