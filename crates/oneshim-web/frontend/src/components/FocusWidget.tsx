import { ArrowRight, Clock, Focus, MessageSquare, TrendingDown, TrendingUp, Zap } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { NavLink } from 'react-router-dom'
import { type FocusMetricsResponse, fetchFocusMetrics } from '../api/client'
import { colors, dataViz, motion } from '../styles/tokens'
import { cn } from '../utils/cn'
import { formatDuration } from '../utils/formatters'
import { Card, CardContent, CardHeader, CardTitle } from './ui/Card'
import { Spinner } from './ui/Spinner'

function Sparkline({ data, color }: { data: number[]; color: string }) {
  if (data.length === 0) return null

  const max = Math.max(...data, 1)
  const min = Math.min(...data, 0)
  const range = max - min || 1
  const width = 80
  const height = 24
  const points = data
    .map((value, i) => {
      const x = (i / (data.length - 1 || 1)) * width
      const y = height - ((value - min) / range) * height
      return `${x},${y}`
    })
    .join(' ')

  return (
    <svg width={width} height={height} className="ml-2 inline-block" aria-hidden="true">
      <polyline fill="none" stroke={color} strokeWidth="2" points={points} />
    </svg>
  )
}

function CircularGauge({ value, max = 100, size = 80 }: { value: number; max?: number; size?: number }) {
  const percentage = Math.min(value / max, 1)
  const circumference = 2 * Math.PI * 35
  const strokeDashoffset = circumference * (1 - percentage)

  const getColor = (score: number) => {
    if (score >= 70) return dataViz.stroke.good
    if (score >= 40) return dataViz.stroke.warning
    return dataViz.stroke.critical
  }

  const color = getColor(value)

  return (
    <svg width={size} height={size} viewBox="0 0 80 80" aria-hidden="true">
      {/* UI note */}
      <circle cx="40" cy="40" r="35" fill="none" stroke="currentColor" strokeWidth="6" className="text-surface-muted" />
      {/* UI note */}
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
        className={`transition-all ${motion.slow}`}
      />
      {/* UI note */}
      <text x="40" y="40" textAnchor="middle" dominantBaseline="middle" className="fill-content font-bold text-lg">
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
      .catch((e: unknown) => {
        if (e instanceof TypeError && e.message.toLowerCase().includes('fetch')) {
          setError(t('errors.agentUnreachable'))
        } else if (e instanceof Error) {
          setError(`${e.message} — ${t('errors.agentCheck')}`)
        } else {
          setError(t('errors.unknownFetchError'))
        }
      })
      .finally(() => setLoading(false))
  }, [])

  if (loading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>
            <Focus className="mr-2 inline h-5 w-5" />
            {t('focus.title')}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex h-32 items-center justify-center">
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
            <Focus className="mr-2 inline h-5 w-5" />
            {t('focus.title')}
          </CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-semantic-error">{error || t('common.error')}</p>
        </CardContent>
      </Card>
    )
  }

  const today = data.today
  const historyScores = data.history.map((m) => m.focus_score)
  const avgScore = historyScores.length > 0 ? historyScores.reduce((a, b) => a + b, 0) / historyScores.length : 0
  const trend = today.focus_score - avgScore

  return (
    <Card variant="interactive">
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle>
            <Focus className="mr-2 inline h-5 w-5" />
            {t('focus.title')}
          </CardTitle>
          <NavLink to="/focus" className={cn('flex items-center gap-1 text-sm hover:underline', colors.primary.text)}>
            {t('common.more')}
            <ArrowRight className="h-4 w-4" />
          </NavLink>
        </div>
      </CardHeader>
      <CardContent>
        <div className="flex items-center gap-6">
          {/* UI note */}
          <div className="flex flex-col items-center">
            <CircularGauge value={today.focus_score} />
            <span className={cn('mt-1 text-xs', colors.text.tertiary)}>{t('focus.score')}</span>
          </div>

          {/* UI note */}
          <div className="grid flex-1 grid-cols-2 gap-3">
            {/* UI note */}
            <div className="flex items-center gap-2">
              <Clock className={cn('h-4 w-4', colors.accent.blue)} />
              <div>
                <p className={cn('font-medium text-sm', colors.text.primary)}>{formatDuration(today.deep_work_secs)}</p>
                <p className={cn('text-xs', colors.text.tertiary)}>{t('focus.deepWork')}</p>
              </div>
            </div>

            {/* UI note */}
            <div className="flex items-center gap-2">
              <MessageSquare className={cn('h-4 w-4', colors.accent.purple)} />
              <div>
                <p className={cn('font-medium text-sm', colors.text.primary)}>
                  {formatDuration(today.communication_secs)}
                </p>
                <p className={cn('text-xs', colors.text.tertiary)}>{t('focus.communication')}</p>
              </div>
            </div>

            {/* UI note */}
            <div className="flex items-center gap-2">
              <Zap className={cn('h-4 w-4', colors.accent.amber)} />
              <div>
                <p className={cn('font-medium text-sm', colors.text.primary)}>
                  {today.interruption_count}
                  {t('focus.times')}
                </p>
                <p className={cn('text-xs', colors.text.tertiary)}>{t('focus.interruptions')}</p>
              </div>
            </div>

            {/* UI note */}
            <div className="flex items-center gap-2">
              {trend >= 0 ? (
                <TrendingUp className={cn('h-4 w-4', colors.accent.green)} />
              ) : (
                <TrendingDown className={cn('h-4 w-4', colors.accent.red)} />
              )}
              <div>
                <p className={cn('font-medium text-sm', trend >= 0 ? colors.accent.green : colors.accent.red)}>
                  {trend >= 0 ? '+' : ''}
                  {trend.toFixed(1)}
                </p>
                <p className={cn('text-xs', colors.text.tertiary)}>{t('focus.trend')}</p>
              </div>
            </div>
          </div>

          {/* UI note */}
          <div className="hidden flex-col items-center lg:flex">
            <Sparkline
              data={[...historyScores].reverse()}
              color={trend >= 0 ? dataViz.stroke.good : dataViz.stroke.critical}
            />
            <span className={cn('mt-1 text-xs', colors.text.tertiary)}>{t('focus.weeklyTrend')}</span>
          </div>
        </div>
      </CardContent>
    </Card>
  )
}
