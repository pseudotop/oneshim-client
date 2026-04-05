import { useEffect, useState } from 'react'
import { cn } from '../../utils/cn'

interface TypeCount {
  suggestion_type: string
  count: number
}

interface SourceStats {
  source: string
  count: number
  acceptance_rate: number
}

interface StatsData {
  total: number
  accepted: number
  rejected: number
  deferred: number
  pending: number
  acceptance_rate: number
  by_type: TypeCount[]
  by_source: SourceStats[]
}

const barColors: Record<string, string> = {
  accepted: 'bg-semantic-success',
  rejected: 'bg-semantic-error',
  deferred: 'bg-semantic-warning',
  pending: 'bg-content-secondary',
}

export function SuggestionStats() {
  const [stats, setStats] = useState<StatsData | null>(null)

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core')
        const data = await invoke<StatsData>('get_suggestion_stats')
        if (!cancelled) setStats(data)
      } catch (e) {
        console.warn('Failed to load stats:', e)
      }
    })()
    return () => {
      cancelled = true
    }
  }, [])

  if (!stats) return <p className="text-content-secondary text-xs p-4">Loading...</p>
  if (stats.total === 0) return <p className="text-content-secondary text-xs p-4">No data yet</p>

  const entries = [
    { key: 'accepted', label: 'Accepted', count: stats.accepted },
    { key: 'rejected', label: 'Rejected', count: stats.rejected },
    { key: 'deferred', label: 'Snoozed', count: stats.deferred },
    { key: 'pending', label: 'Pending', count: stats.pending },
  ]

  return (
    <div className="flex flex-col gap-3 p-3">
      <div className="text-center">
        <div className="text-2xl font-bold text-brand">{stats.acceptance_rate}%</div>
        <div className="text-[10px] text-content-secondary">Acceptance Rate</div>
      </div>
      <div className="text-[10px] text-content-secondary text-center">{stats.total} total suggestions</div>
      <div className="flex flex-col gap-1.5">
        {entries.map(({ key, label, count }) => (
          <div key={key} className="flex items-center gap-2">
            <span className="text-[10px] text-content-secondary w-14">{label}</span>
            <div className="flex-1 h-3 rounded-full bg-content-inverse/5 overflow-hidden">
              <div
                className={cn('h-full rounded-full transition-all', barColors[key])}
                style={{ width: `${stats.total > 0 ? (count / stats.total) * 100 : 0}%` }}
              />
            </div>
            <span className="text-[10px] text-content-primary w-6 text-right">{count}</span>
          </div>
        ))}
      </div>

      {/* Type Distribution */}
      {stats.by_type.length > 0 && (
        <>
          <div className="text-[10px] text-content-secondary font-medium pt-1">Type Distribution</div>
          <div className="flex flex-col gap-1">
            {stats.by_type.map(({ suggestion_type, count }) => {
              const maxCount = stats.by_type[0]?.count ?? 1
              return (
                <div key={suggestion_type} className="flex items-center gap-2">
                  <span className="text-[10px] text-content-secondary w-24 truncate" title={suggestion_type}>
                    {suggestion_type}
                  </span>
                  <div className="flex-1 h-2.5 rounded-full bg-content-inverse/5 overflow-hidden">
                    <div
                      className="h-full rounded-full bg-brand/60 transition-all"
                      style={{ width: `${maxCount > 0 ? (count / maxCount) * 100 : 0}%` }}
                    />
                  </div>
                  <span className="text-[10px] text-content-primary w-6 text-right">{count}</span>
                </div>
              )
            })}
          </div>
        </>
      )}

      {/* Source Quality */}
      {stats.by_source.length > 0 && (
        <>
          <div className="text-[10px] text-content-secondary font-medium pt-1">Source Quality</div>
          <div className="flex flex-col gap-1">
            {stats.by_source.map(({ source, count, acceptance_rate }) => (
              <div key={source} className="flex items-center justify-between">
                <span className="text-[10px] text-content-secondary w-20 truncate" title={source}>
                  {source}
                </span>
                <span className="text-[10px] text-content-primary">{count} total</span>
                <span
                  className={cn(
                    'text-[10px] font-medium w-12 text-right',
                    acceptance_rate >= 50 ? 'text-semantic-success' : 'text-content-secondary',
                  )}
                >
                  {acceptance_rate}%
                </span>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  )
}
