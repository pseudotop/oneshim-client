import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { fetchHeatmap, type HeatmapResponse } from '../api/client'
import { motion, typography } from '../styles/tokens'
import { cn } from '../utils/cn'

interface ActivityHeatmapProps {
  days?: number
  className?: string
}
const HOUR_LABELS = Array.from({ length: 24 }, (_, i) => i.toString().padStart(2, '0'))

function getColor(ratio: number): string {
  if (ratio === 0) return 'bg-hover'
  if (ratio < 0.2) return 'bg-brand-signal/10'
  if (ratio < 0.4) return 'bg-brand-signal/25'
  if (ratio < 0.6) return 'bg-brand-signal/40'
  if (ratio < 0.8) return 'bg-brand-signal/60'
  return 'bg-brand-signal/80'
}

export function ActivityHeatmap({ days = 7, className = '' }: ActivityHeatmapProps) {
  const { t } = useTranslation()
  const dayLabels = t('heatmap.days', { returnObjects: true }) as string[]
  const { data, isLoading, error } = useQuery<HeatmapResponse>({
    queryKey: ['heatmap', days],
    queryFn: () => fetchHeatmap(days),
    refetchInterval: 60000, // 1 min refresh
  })

  if (isLoading) {
    return (
      <div className={cn('rounded-lg p-4', motion.colors, className)}>
        <h3 className={cn('mb-4 text-content', typography.h3)}>{t('heatmap.title')}</h3>
        <div className="flex h-48 items-center justify-center">
          <div className="h-8 w-8 animate-spin rounded-full border-brand-signal border-b-2" />
        </div>
      </div>
    )
  }

  if (error || !data) {
    return (
      <div className={cn('rounded-lg p-4', motion.colors, className)}>
        <h3 className={cn('mb-4 text-content', typography.h3)}>{t('heatmap.title')}</h3>
        <div className="py-8 text-center text-semantic-error">{t('heatmap.loadError')}</div>
      </div>
    )
  }

  const grid: number[][] = Array.from({ length: 7 }, () => Array(24).fill(0))
  for (const cell of data.cells) {
    if (cell.day < 7 && cell.hour < 24) {
      grid[cell.day][cell.hour] = cell.value
    }
  }

  const maxValue = data.max_value || 1

  return (
    <div className={cn('rounded-lg p-6', motion.colors, className)}>
      <div className="mb-4 flex items-center justify-between">
        <h3 className={cn('text-content', typography.h3)}>{t('heatmap.title')}</h3>
        <span className={cn('text-brand-text', typography.body)}>
          {data.from_date} ~ {data.to_date}
        </span>
      </div>

      {/* UI note */}
      <div className="mb-1 ml-8 flex">
        {HOUR_LABELS.map((hour, i) => (
          <div
            key={hour}
            className={cn('flex-1 text-center text-content-tertiary', typography.caption)}
            style={{ visibility: i % 3 === 0 ? 'visible' : 'hidden' }}
          >
            {hour}
          </div>
        ))}
      </div>

      {/* UI note */}
      <div className="flex flex-col gap-0.5">
        {grid.map((row, dayIndex) => (
          <div key={dayLabels[dayIndex]} className="flex items-center gap-0.5">
            {/* UI note */}
            <div className={cn('w-8 pr-2 text-right text-content-secondary', typography.caption)}>{dayLabels[dayIndex]}</div>
            {/* UI note */}
            {row.map((value, hourIndex) => {
              const ratio = value / maxValue
              return (
                <div
                  key={HOUR_LABELS[hourIndex]}
                  className={cn('h-4 flex-1 rounded-sm cursor-pointer hover:ring-2 hover:ring-brand-signal', motion.colors, getColor(ratio))}
                  title={`${dayLabels[dayIndex]} ${HOUR_LABELS[hourIndex]}:00 - ${t('heatmap.activity')}: ${value}`}
                />
              )
            })}
          </div>
        ))}
      </div>

      {/* UI note */}
      <div className="mt-4 flex items-center justify-end gap-2">
        <span className={cn('text-content-secondary', typography.caption)}>{t('heatmap.low')}</span>
        <div className="flex gap-0.5">
          <div className="h-3 w-3 rounded-sm bg-hover" />
          <div className="h-3 w-3 rounded-sm bg-brand-signal/10" />
          <div className="h-3 w-3 rounded-sm bg-brand-signal/25" />
          <div className="h-3 w-3 rounded-sm bg-brand-signal/40" />
          <div className="h-3 w-3 rounded-sm bg-brand-signal/60" />
          <div className="h-3 w-3 rounded-sm bg-brand-signal/80" />
        </div>
        <span className={cn('text-content-secondary', typography.caption)}>{t('heatmap.high')}</span>
      </div>
    </div>
  )
}
