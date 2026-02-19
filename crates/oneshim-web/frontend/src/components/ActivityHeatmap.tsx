// 활동 히트맵 컴포넌트 (요일 x 시간)
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { fetchHeatmap, HeatmapResponse } from '../api/client'

interface ActivityHeatmapProps {
  days?: number
  className?: string
}
const HOUR_LABELS = Array.from({ length: 24 }, (_, i) => i.toString().padStart(2, '0'))
const DAY_LABELS_EN = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']

// 활동량에 따른 색상 반환 (0-1 비율)
function getColor(ratio: number): string {
  if (ratio === 0) return 'bg-slate-200 dark:bg-slate-700'
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
    refetchInterval: 60000, // 1분마다 갱신
  })

  if (isLoading) {
    return (
      <div className={`rounded-lg p-4 transition-colors ${className}`}>
        <h3 className="text-lg font-semibold text-slate-900 dark:text-white mb-4">{t('heatmap.title')}</h3>
        <div className="flex items-center justify-center h-48">
          <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-teal-500"></div>
        </div>
      </div>
    )
  }

  if (error || !data) {
    return (
      <div className={`rounded-lg p-4 transition-colors ${className}`}>
        <h3 className="text-lg font-semibold text-slate-900 dark:text-white mb-4">{t('heatmap.title')}</h3>
        <div className="text-center text-red-500 py-8">{t('heatmap.loadError')}</div>
      </div>
    )
  }

  // 7x24 그리드 생성
  const grid: number[][] = Array.from({ length: 7 }, () => Array(24).fill(0))
  for (const cell of data.cells) {
    if (cell.day < 7 && cell.hour < 24) {
      grid[cell.day][cell.hour] = cell.value
    }
  }

  const maxValue = data.max_value || 1

  return (
    <div className={`rounded-lg p-6 transition-colors ${className}`}>
      <div className="flex items-center justify-between mb-4">
        <h3 className="text-lg font-semibold text-slate-900 dark:text-white">{t('heatmap.title')}</h3>
        <span className="text-sm text-slate-600 dark:text-slate-400">
          {data.from_date} ~ {data.to_date}
        </span>
      </div>

      {/* 시간 레이블 */}
      <div className="flex ml-8 mb-1">
        {HOUR_LABELS.map((hour, i) => (
          <div
            key={hour}
            className="flex-1 text-center text-xs text-slate-500 dark:text-slate-500"
            style={{ visibility: i % 3 === 0 ? 'visible' : 'hidden' }}
          >
            {hour}
          </div>
        ))}
      </div>

      {/* 히트맵 그리드 */}
      <div className="flex flex-col gap-0.5">
        {grid.map((row, dayIndex) => (
          <div key={dayIndex} className="flex items-center gap-0.5">
            {/* 요일 레이블 */}
            <div className="w-8 text-xs text-slate-600 dark:text-slate-400 text-right pr-2">
              {dayLabels[dayIndex]}
            </div>
            {/* 시간별 셀 */}
            {row.map((value, hourIndex) => {
              const ratio = value / maxValue
              return (
                <div
                  key={hourIndex}
                  className={`flex-1 h-4 rounded-sm ${getColor(ratio)} transition-colors cursor-pointer hover:ring-2 hover:ring-teal-400`}
                  title={`${dayLabels[dayIndex]} ${HOUR_LABELS[hourIndex]}:00 - ${t('heatmap.activity')}: ${value}`}
                />
              )
            })}
          </div>
        ))}
      </div>

      {/* 범례 */}
      <div className="flex items-center justify-end gap-2 mt-4">
        <span className="text-xs text-slate-600 dark:text-slate-400">{t('heatmap.low')}</span>
        <div className="flex gap-0.5">
          <div className="w-3 h-3 rounded-sm bg-slate-200 dark:bg-slate-700" />
          <div className="w-3 h-3 rounded-sm bg-green-100 dark:bg-green-900" />
          <div className="w-3 h-3 rounded-sm bg-green-200 dark:bg-green-800" />
          <div className="w-3 h-3 rounded-sm bg-green-300 dark:bg-green-700" />
          <div className="w-3 h-3 rounded-sm bg-green-400 dark:bg-green-600" />
          <div className="w-3 h-3 rounded-sm bg-green-500 dark:bg-green-500" />
        </div>
        <span className="text-xs text-slate-600 dark:text-slate-400">{t('heatmap.high')}</span>
      </div>
    </div>
  )
}
