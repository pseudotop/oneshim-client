import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { fetchHeatmap, type HeatmapResponse } from '../api/client'

interface ActivityHeatmapProps {
  days?: number
  className?: string
}
const HOUR_LABELS = Array.from({ length: 24 }, (_, i) => i.toString().padStart(2, '0'))
const DAY_LABELS_EN = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']

function getColor(ratio: number): string {
  if (ratio === 0) return 'bg-hover'
  if (ratio < 0.2) return 'bg-green-100 dark:bg-green-900'
  if (ratio < 0.4) return 'bg-green-200 dark:bg-green-800'
  if (ratio < 0.6) return 'bg-green-300 dark:bg-green-700'
  if (ratio < 0.8) return 'bg-green-400 dark:bg-green-600'
  return 'bg-green-500 dark:bg-green-500'
}

export function ActivityHeatmap({ days = 7, className = '' }: ActivityHeatmapProps) {
  const { t, i18n } = useTranslation()
  const dayLabels = i18n.language === 'en' ? DAY_LABELS_EN : (t('heatmap.days', { returnObjects: true }) as string[])
  const { data, isLoading, error } = useQuery<HeatmapResponse>({
    queryKey: ['heatmap', days],
    queryFn: () => fetchHeatmap(days),
    refetchInterval: 60000, // 1 min refresh
  })

  if (isLoading) {
    return (
      <div className={`rounded-lg p-4 transition-colors ${className}`}>
        <h3 className="mb-4 font-semibold text-content text-lg">{t('heatmap.title')}</h3>
        <div className="flex h-48 items-center justify-center">
          <div className="h-8 w-8 animate-spin rounded-full border-teal-500 border-b-2"></div>
        </div>
      </div>
    )
  }

  if (error || !data) {
    return (
      <div className={`rounded-lg p-4 transition-colors ${className}`}>
        <h3 className="mb-4 font-semibold text-content text-lg">{t('heatmap.title')}</h3>
        <div className="py-8 text-center text-red-500">{t('heatmap.loadError')}</div>
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
    <div className={`rounded-lg p-6 transition-colors ${className}`}>
      <div className="mb-4 flex items-center justify-between">
        <h3 className="font-semibold text-content text-lg">{t('heatmap.title')}</h3>
        <span className="text-accent-blue text-sm">
          {data.from_date} ~ {data.to_date}
        </span>
      </div>

      {/* UI note */}
      <div className="mb-1 ml-8 flex">
        {HOUR_LABELS.map((hour, i) => (
          <div
            key={hour}
            className="flex-1 text-center text-content-tertiary text-xs"
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
            <div className="w-8 pr-2 text-right text-content-secondary text-xs">{dayLabels[dayIndex]}</div>
            {/* UI note */}
            {row.map((value, hourIndex) => {
              const ratio = value / maxValue
              return (
                <div
                  key={HOUR_LABELS[hourIndex]}
                  className={`h-4 flex-1 rounded-sm ${getColor(ratio)} cursor-pointer transition-colors hover:ring-2 hover:ring-brand-signal`}
                  title={`${dayLabels[dayIndex]} ${HOUR_LABELS[hourIndex]}:00 - ${t('heatmap.activity')}: ${value}`}
                />
              )
            })}
          </div>
        ))}
      </div>

      {/* UI note */}
      <div className="mt-4 flex items-center justify-end gap-2">
        <span className="text-content-secondary text-xs">{t('heatmap.low')}</span>
        <div className="flex gap-0.5">
          <div className="h-3 w-3 rounded-sm bg-hover" />
          <div className="h-3 w-3 rounded-sm bg-green-100 dark:bg-green-900" />
          <div className="h-3 w-3 rounded-sm bg-green-200 dark:bg-green-800" />
          <div className="h-3 w-3 rounded-sm bg-green-300 dark:bg-green-700" />
          <div className="h-3 w-3 rounded-sm bg-green-400 dark:bg-green-600" />
          <div className="h-3 w-3 rounded-sm bg-green-500 dark:bg-green-500" />
        </div>
        <span className="text-content-secondary text-xs">{t('heatmap.high')}</span>
      </div>
    </div>
  )
}
