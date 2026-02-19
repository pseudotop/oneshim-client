import { useState, useEffect } from 'react'
import { useTranslation } from 'react-i18next'

interface DateRangePickerProps {
  onRangeChange: (from: string | undefined, to: string | undefined) => void
  initialFrom?: string
  initialTo?: string
}

type PresetRange = 'today' | '7days' | '30days' | 'custom'

export default function DateRangePicker({
  onRangeChange,
  initialFrom,
  initialTo,
}: DateRangePickerProps) {
  const { t } = useTranslation()
  const [preset, setPreset] = useState<PresetRange>('today')
  const [customFrom, setCustomFrom] = useState(initialFrom || '')
  const [customTo, setCustomTo] = useState(initialTo || '')

  // 오늘 날짜 계산
  const getToday = () => {
    const now = new Date()
    return now.toISOString().split('T')[0]
  }

  // N일 전 날짜 계산
  const getDaysAgo = (days: number) => {
    const date = new Date()
    date.setDate(date.getDate() - days)
    return date.toISOString().split('T')[0]
  }

  // 프리셋 변경 시 날짜 범위 업데이트
  useEffect(() => {
    let from: string | undefined
    let to: string | undefined

    switch (preset) {
      case 'today':
        from = getToday() + 'T00:00:00Z'
        to = getToday() + 'T23:59:59Z'
        break
      case '7days':
        from = getDaysAgo(7) + 'T00:00:00Z'
        to = getToday() + 'T23:59:59Z'
        break
      case '30days':
        from = getDaysAgo(30) + 'T00:00:00Z'
        to = getToday() + 'T23:59:59Z'
        break
      case 'custom':
        if (customFrom && customTo) {
          from = customFrom + 'T00:00:00Z'
          to = customTo + 'T23:59:59Z'
        }
        break
    }

    onRangeChange(from, to)
  }, [preset, customFrom, customTo, onRangeChange])

  const handlePresetClick = (newPreset: PresetRange) => {
    setPreset(newPreset)
  }

  return (
    <div className="flex items-center space-x-2 flex-wrap gap-2">
      {/* 프리셋 버튼 */}
      <div className="flex space-x-1">
        <button
          onClick={() => handlePresetClick('today')}
          className={`px-3 py-1.5 text-sm rounded-lg transition-colors ${
            preset === 'today'
              ? 'bg-teal-600 text-white'
              : 'bg-slate-200 dark:bg-slate-700 text-slate-700 dark:text-slate-300 hover:bg-slate-300 dark:hover:bg-slate-600'
          }`}
        >
          {t('dateRange.today')}
        </button>
        <button
          onClick={() => handlePresetClick('7days')}
          className={`px-3 py-1.5 text-sm rounded-lg transition-colors ${
            preset === '7days'
              ? 'bg-teal-600 text-white'
              : 'bg-slate-200 dark:bg-slate-700 text-slate-700 dark:text-slate-300 hover:bg-slate-300 dark:hover:bg-slate-600'
          }`}
        >
          {t('dateRange.week')}
        </button>
        <button
          onClick={() => handlePresetClick('30days')}
          className={`px-3 py-1.5 text-sm rounded-lg transition-colors ${
            preset === '30days'
              ? 'bg-teal-600 text-white'
              : 'bg-slate-200 dark:bg-slate-700 text-slate-700 dark:text-slate-300 hover:bg-slate-300 dark:hover:bg-slate-600'
          }`}
        >
          {t('dateRange.month')}
        </button>
        <button
          onClick={() => handlePresetClick('custom')}
          className={`px-3 py-1.5 text-sm rounded-lg transition-colors ${
            preset === 'custom'
              ? 'bg-teal-600 text-white'
              : 'bg-slate-200 dark:bg-slate-700 text-slate-700 dark:text-slate-300 hover:bg-slate-300 dark:hover:bg-slate-600'
          }`}
        >
          {t('dateRange.custom')}
        </button>
      </div>

      {/* 커스텀 날짜 입력 */}
      {preset === 'custom' && (
        <div className="flex items-center space-x-2">
          <input
            type="date"
            value={customFrom}
            onChange={(e) => setCustomFrom(e.target.value)}
            className="bg-white dark:bg-slate-700 border border-slate-300 dark:border-slate-600 rounded-lg px-3 py-1.5 text-sm text-slate-900 dark:text-white focus:outline-none focus:border-teal-500"
          />
          <span className="text-slate-400">~</span>
          <input
            type="date"
            value={customTo}
            onChange={(e) => setCustomTo(e.target.value)}
            className="bg-white dark:bg-slate-700 border border-slate-300 dark:border-slate-600 rounded-lg px-3 py-1.5 text-sm text-slate-900 dark:text-white focus:outline-none focus:border-teal-500"
          />
        </div>
      )}
    </div>
  )
}
