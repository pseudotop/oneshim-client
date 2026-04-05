import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
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

interface DailyStat {
  day: string
  total: number
  acted: number
  suggestion_type: string
  source: string
}

interface DayAggregate {
  day: string
  total: number
  acted: number
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
  const { t } = useTranslation()
  const [stats, setStats] = useState<StatsData | null>(null)
  const [dailyTrends, setDailyTrends] = useState<DayAggregate[]>([])

  useEffect(() => {
    let cancelled = false
    ;(async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core')
        const [data, daily] = await Promise.all([
          invoke<StatsData>('get_suggestion_stats'),
          invoke<DailyStat[]>('get_suggestion_daily_stats', { days: 7 }),
        ])
        if (cancelled) return
        setStats(data)
        // Aggregate daily rows by day (sum across types/sources)
        const map = new Map<string, DayAggregate>()
        for (const row of daily) {
          const existing = map.get(row.day)
          if (existing) {
            existing.total += row.total
            existing.acted += row.acted
          } else {
            map.set(row.day, { day: row.day, total: row.total, acted: row.acted })
          }
        }
        // Sort ascending by day and take last 7
        const sorted = Array.from(map.values())
          .sort((a, b) => a.day.localeCompare(b.day))
          .slice(-7)
        setDailyTrends(sorted)
      } catch (e) {
        console.warn('Failed to load stats:', e)
      }
    })()
    return () => {
      cancelled = true
    }
  }, [])

  if (!stats) return <p className="text-content-secondary text-xs p-4">{t('common.loading', 'Loading...')}</p>
  if (stats.total === 0) return <p className="text-content-secondary text-xs p-4">{t('suggestionStats.noData', 'No data yet')}</p>

  const entries = [
    { key: 'accepted', label: t('suggestionStats.accepted', 'Accepted'), count: stats.accepted },
    { key: 'rejected', label: t('suggestionStats.rejected', 'Rejected'), count: stats.rejected },
    { key: 'deferred', label: t('suggestionStats.snoozed', 'Snoozed'), count: stats.deferred },
    { key: 'pending', label: t('suggestionStats.pending', 'Pending'), count: stats.pending },
  ]

  return (
    <div className="flex flex-col gap-3 p-3">
      <div className="text-center">
        <div className="text-2xl font-bold text-brand">{stats.acceptance_rate}%</div>
        <div className="text-[10px] text-content-secondary">{t('suggestionStats.acceptanceRate', 'Acceptance Rate')}</div>
      </div>
      <div className="text-[10px] text-content-secondary text-center">{t('suggestionStats.totalSuggestions', '{{count}} total suggestions', { count: stats.total })}</div>
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
          <div className="text-[10px] text-content-secondary font-medium pt-1">{t('suggestionStats.typeDistribution', 'Type Distribution')}</div>
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
          <div className="text-[10px] text-content-secondary font-medium pt-1">{t('suggestionStats.sourceQuality', 'Source Quality')}</div>
          <div className="flex flex-col gap-1">
            {stats.by_source.map(({ source, count, acceptance_rate }) => (
              <div key={source} className="flex items-center justify-between">
                <span className="text-[10px] text-content-secondary w-20 truncate" title={source}>
                  {source}
                </span>
                <span className="text-[10px] text-content-primary">{t('suggestionStats.countTotal', '{{count}} total', { count })}</span>
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

      {/* Daily Trends */}
      {dailyTrends.length > 0 &&
        (() => {
          const maxTotal = Math.max(...dailyTrends.map((d) => d.total), 1)
          return (
            <>
              <div className="text-[10px] text-content-secondary font-medium pt-1">{t('suggestionStats.dailyTrends', 'Daily Trends (7d)')}</div>
              <div className="flex flex-col gap-1">
                {dailyTrends.map(({ day, total, acted }) => (
                  <div key={day} className="flex items-center gap-2">
                    <span className="text-[10px] text-content-secondary w-14 tabular-nums">{day.slice(5)}</span>
                    <div className="flex-1 h-3 rounded-full bg-content-inverse/5 overflow-hidden relative">
                      <div
                        className="absolute inset-y-0 left-0 rounded-full bg-brand/30 transition-all"
                        style={{ width: `${(total / maxTotal) * 100}%` }}
                      />
                      <div
                        className="absolute inset-y-0 left-0 rounded-full bg-brand transition-all"
                        style={{ width: `${(acted / maxTotal) * 100}%` }}
                      />
                    </div>
                    <span className="text-[10px] text-content-primary w-10 text-right tabular-nums">
                      {acted}/{total}
                    </span>
                  </div>
                ))}
              </div>
              <div className="flex items-center gap-3 justify-center text-[9px] text-content-secondary">
                <span className="flex items-center gap-1">
                  <span className="inline-block w-2 h-2 rounded-full bg-brand" />
                  {t('suggestionStats.acted', 'Acted')}
                </span>
                <span className="flex items-center gap-1">
                  <span className="inline-block w-2 h-2 rounded-full bg-brand/30" />
                  {t('suggestionStats.total', 'Total')}
                </span>
              </div>
            </>
          )
        })()}
    </div>
  )
}
