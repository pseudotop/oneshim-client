import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { typography } from '../../styles/tokens'
import { cn } from '../../utils/cn'
import type { SuggestionHistoryDto } from '../types'

const feedbackBadgeClassName: Record<string, string> = {
  accepted: 'bg-semantic-success/20 text-semantic-success',
  rejected: 'bg-semantic-error/20 text-semantic-error',
  deferred: 'bg-semantic-warning/20 text-semantic-warning',
}

const feedbackBadgeKey: Record<string, string> = {
  accepted: 'suggestions.feedbackAccepted',
  rejected: 'suggestions.feedbackRejected',
  deferred: 'suggestions.feedbackSnoozed',
}

export function SuggestionHistory() {
  const { t } = useTranslation()
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
    return () => {
      cancelled = true
    }
  }, [])

  if (loading) {
    return <p className="text-content-secondary text-xs p-4">{t('common.loading', 'Loading...')}</p>
  }

  if (entries.length === 0) {
    return <p className="text-content-secondary text-xs p-4">{t('suggestions.noHistory', 'No history yet')}</p>
  }

  const stats = {
    accepted: entries.filter((e) => e.feedback === 'accepted').length,
    rejected: entries.filter((e) => e.feedback === 'rejected').length,
    deferred: entries.filter((e) => e.feedback === 'deferred').length,
    pending: entries.filter((e) => !e.feedback).length,
  }

  return (
    <div className="flex flex-col gap-2 p-2">
      <div className="flex gap-3 text-xs text-content-secondary px-2 pb-2 border-b border-border-default">
        <span>
          {stats.accepted} {t('suggestions.statsAccepted', 'accepted')}
        </span>
        <span>
          {stats.rejected} {t('suggestions.statsRejected', 'rejected')}
        </span>
        <span>
          {stats.deferred} {t('suggestions.statsSnoozed', 'snoozed')}
        </span>
        <span>
          {stats.pending} {t('suggestions.statsPending', 'pending')}
        </span>
      </div>
      <ul className="flex flex-col gap-1.5">
        {entries.map((entry) => {
          const badgeClass = entry.feedback ? feedbackBadgeClassName[entry.feedback] : null
          const badgeKey = entry.feedback ? feedbackBadgeKey[entry.feedback] : null
          return (
            <li key={entry.id} className="px-3 py-2 rounded-lg bg-surface-default/60 text-xs">
              <div className="flex items-center justify-between gap-2">
                <span className={cn(typography.weight.medium, 'text-content-primary truncate')}>{entry.title}</span>
                {badgeClass && badgeKey && (
                  <span
                    className={cn('px-1.5 py-0.5 rounded text-[10px] shrink-0', typography.weight.medium, badgeClass)}
                  >
                    {t(badgeKey)}
                  </span>
                )}
              </div>
              <p className="text-content-secondary line-clamp-1 mt-0.5">{entry.body}</p>
            </li>
          )
        })}
      </ul>
    </div>
  )
}
