import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { NavLink } from 'react-router-dom'
import { Focus, Clock, MessageSquare, Zap, TrendingUp, TrendingDown, ArrowRight } from 'lucide-react'
import { Card, CardHeader, CardTitle, CardContent } from './ui/Card'
import { Spinner } from './ui/Spinner'
import { fetchFocusMetrics, FocusMetricsResponse } from '../api/client'
import { formatDuration } from '../utils/formatters'

/** 스파크라인 컴포넌트 (간단한 7일 트렌드) */
function Sparkline({ data, color }: { data: number[]; color: string }) {
  if (data.length === 0) return null

  const max = Math.max(...data, 1)
  const min = Math.min(...data, 0)
  const range = max - min || 1
  const width = 80
  const height = 24
  const points = data.map((value, i) => {
    const x = (i / (data.length - 1 || 1)) * width
    const y = height - ((value - min) / range) * height
    return `${x},${y}`
  }).join(' ')

  return (
    <svg width={width} height={height} className="inline-block ml-2">
      <polyline
        fill="none"
        stroke={color}
        strokeWidth="2"
        points={points}
      />
    </svg>
  )
}

/** 원형 게이지 컴포넌트 */
function CircularGauge({ value, max = 100, size = 80 }: { value: number; max?: number; size?: number }) {
  const percentage = Math.min(value / max, 1)
  const circumference = 2 * Math.PI * 35
  const strokeDashoffset = circumference * (1 - percentage)

  // 점수에 따른 색상
  const getColor = (score: number) => {
    if (score >= 70) return '#10b981' // green-500
    if (score >= 40) return '#f59e0b' // amber-500
    return '#ef4444' // red-500
  }

  const color = getColor(value)

  return (
    <svg width={size} height={size} viewBox="0 0 80 80">
      {/* 배경 원 */}
      <circle
        cx="40"
        cy="40"
        r="35"
        fill="none"
        stroke="currentColor"
        strokeWidth="6"
        className="text-slate-200 dark:text-slate-700"
      />
      {/* 진행 원 */}
      <circle
        cx="40"
        cy="40"
        r="35"
        fill="none"
        stroke={color}
        strokeWidth="6"
        strokeLinecap="round"
        strokeDasharray={circumference}
        strokeDashoffset={strokeDashoffset}
        transform="rotate(-90 40 40)"
        className="transition-all duration-500"
      />
      {/* 점수 텍스트 */}
      <text
        x="40"
        y="40"
        textAnchor="middle"
        dominantBaseline="middle"
        className="fill-slate-900 dark:fill-white text-lg font-bold"
      >
        {Math.round(value)}
      </text>
    </svg>
  )
}

export default function FocusWidget() {
  const { t } = useTranslation()
  const [data, setData] = useState<FocusMetricsResponse | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    fetchFocusMetrics()
      .then(setData)
      .catch((e) => setError(e.message))
      .finally(() => setLoading(false))
  }, [])

  if (loading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>
            <Focus className="w-5 h-5 inline mr-2" />
            {t('focus.title')}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex justify-center items-center h-32">
            <Spinner size="md" />
          </div>
        </CardContent>
      </Card>
    )
  }

  if (error || !data) {
    return (
      <Card variant="danger">
        <CardHeader>
          <CardTitle>
            <Focus className="w-5 h-5 inline mr-2" />
            {t('focus.title')}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-red-500">{error || t('common.error')}</p>
        </CardContent>
      </Card>
    )
  }

  const today = data.today
  const historyScores = data.history.map(m => m.focus_score)
  const avgScore = historyScores.length > 0
    ? historyScores.reduce((a, b) => a + b, 0) / historyScores.length
    : 0
  const trend = today.focus_score - avgScore

  return (
    <Card variant="interactive">
      <CardHeader>
        <div className="flex justify-between items-center">
          <CardTitle>
            <Focus className="w-5 h-5 inline mr-2" />
            {t('focus.title')}
          </CardTitle>
          <NavLink
            to="/focus"
            className="text-sm text-teal-600 dark:text-teal-400 hover:underline flex items-center gap-1"
          >
            {t('common.more')}
            <ArrowRight className="w-4 h-4" />
          </NavLink>
        </div>
      </CardHeader>
      <CardContent>
        <div className="flex items-center gap-6">
          {/* 집중도 점수 게이지 */}
          <div className="flex flex-col items-center">
            <CircularGauge value={today.focus_score} />
            <span className="text-xs text-slate-500 dark:text-slate-400 mt-1">
              {t('focus.score')}
            </span>
          </div>

          {/* 메트릭 그리드 */}
          <div className="flex-1 grid grid-cols-2 gap-3">
            {/* 깊은 작업 시간 */}
            <div className="flex items-center gap-2">
              <Clock className="w-4 h-4 text-blue-500" />
              <div>
                <p className="text-sm font-medium text-slate-900 dark:text-white">
                  {formatDuration(today.deep_work_secs)}
                </p>
                <p className="text-xs text-slate-500 dark:text-slate-400">
                  {t('focus.deepWork')}
                </p>
              </div>
            </div>

            {/* 소통 시간 */}
            <div className="flex items-center gap-2">
              <MessageSquare className="w-4 h-4 text-purple-500" />
              <div>
                <p className="text-sm font-medium text-slate-900 dark:text-white">
                  {formatDuration(today.communication_secs)}
                </p>
                <p className="text-xs text-slate-500 dark:text-slate-400">
                  {t('focus.communication')}
                </p>
              </div>
            </div>

            {/* 중단 횟수 */}
            <div className="flex items-center gap-2">
              <Zap className="w-4 h-4 text-amber-500" />
              <div>
                <p className="text-sm font-medium text-slate-900 dark:text-white">
                  {today.interruption_count}{t('focus.times')}
                </p>
                <p className="text-xs text-slate-500 dark:text-slate-400">
                  {t('focus.interruptions')}
                </p>
              </div>
            </div>

            {/* 트렌드 */}
            <div className="flex items-center gap-2">
              {trend >= 0 ? (
                <TrendingUp className="w-4 h-4 text-green-500" />
              ) : (
                <TrendingDown className="w-4 h-4 text-red-500" />
              )}
              <div>
                <p className={`text-sm font-medium ${trend >= 0 ? 'text-green-600 dark:text-green-400' : 'text-red-600 dark:text-red-400'}`}>
                  {trend >= 0 ? '+' : ''}{trend.toFixed(1)}
                </p>
                <p className="text-xs text-slate-500 dark:text-slate-400">
                  {t('focus.trend')}
                </p>
              </div>
            </div>
          </div>

          {/* 7일 스파크라인 */}
          <div className="hidden lg:flex flex-col items-center">
            <Sparkline
              data={[...historyScores].reverse()}
              color={trend >= 0 ? '#10b981' : '#ef4444'}
            />
            <span className="text-xs text-slate-500 dark:text-slate-400 mt-1">
              {t('focus.weeklyTrend')}
            </span>
          </div>
        </div>
      </CardContent>
    </Card>
  )
}
