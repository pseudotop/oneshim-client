import { useQuery } from '@tanstack/react-query'
import {
  ArrowRightLeft,
  Brain,
  Clock,
  Focus as FocusIcon,
  Laptop,
  MessageSquare,
  TrendingDown,
  TrendingUp,
  Zap,
} from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { CartesianGrid, Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import { fetchFocusMetrics, fetchInterruptions, fetchWorkSessions } from '../api/client'
import DateRangePicker from '../components/DateRangePicker'
import StatCard from '../components/StatCard'
import { EmptyState } from '../components/ui'
import { Badge } from '../components/ui/Badge'
import { Card, CardContent, CardHeader, CardTitle } from '../components/ui/Card'
import { Spinner } from '../components/ui/Spinner'
import { useTheme } from '../contexts/ThemeContext'
import { colors, iconSize, motion, typography } from '../styles/tokens'
import { cn } from '../utils/cn'
import { formatDuration } from '../utils/formatters'

const CATEGORY_COLORS: Record<string, string> = {
  Development: 'bg-blue-500',
  Communication: 'bg-purple-500',
  Documentation: 'bg-green-500',
  Browser: 'bg-amber-500',
  Design: 'bg-pink-500',
  Media: 'bg-red-500',
  System: 'bg-accent-slate',
  Other: 'bg-status-disconnected',
}

function CircularGauge({ value, size = 120 }: { value: number; size?: number }) {
  const { t } = useTranslation()
  const percentage = Math.min(value / 100, 1)
  const circumference = 2 * Math.PI * 45
  const strokeDashoffset = circumference * (1 - percentage)

  const getColor = (score: number) => {
    if (score >= 70) return '#10b981' // green
    if (score >= 40) return '#f59e0b' // amber
    return '#ef4444' // red
  }

  const color = getColor(value)

  return (
    <svg width={size} height={size} viewBox="0 0 100 100" aria-hidden="true">
      <circle cx="50" cy="50" r="45" fill="none" stroke="currentColor" strokeWidth="8" className="text-border-muted" />
      <circle
        cx="50"
        cy="50"
        r="45"
        fill="none"
        stroke={color}
        strokeWidth="8"
        strokeLinecap="round"
        strokeDasharray={circumference}
        strokeDashoffset={strokeDashoffset}
        transform="rotate(-90 50 50)"
        className={`transition-all ${motion.slow}`}
      />
      <text x="50" y="45" textAnchor="middle" dominantBaseline="middle" className="fill-content font-bold text-2xl">
        {Math.round(value)}
      </text>
      <text x="50" y="62" textAnchor="middle" className="fill-content-secondary text-xs">
        {t('focus.score')}
      </text>
    </svg>
  )
}

export default function Focus() {
  const { t } = useTranslation()
  const { theme } = useTheme()
  const isDark = theme === 'dark'
  const [dateRange, setDateRange] = useState<{ from: Date; to: Date }>({
    from: new Date(Date.now() - 24 * 60 * 60 * 1000),
    to: new Date(),
  })

  const {
    data: metrics,
    isLoading: metricsLoading,
    error: metricsError,
  } = useQuery({
    queryKey: ['focusMetrics'],
    queryFn: fetchFocusMetrics,
  })
  const { data: sessions = [], isLoading: sessionsLoading } = useQuery({
    queryKey: ['workSessions', dateRange.from.toISOString(), dateRange.to.toISOString()],
    queryFn: () => fetchWorkSessions(dateRange.from.toISOString(), dateRange.to.toISOString()),
  })
  const { data: interruptions = [], isLoading: interruptionsLoading } = useQuery({
    queryKey: ['interruptions', dateRange.from.toISOString(), dateRange.to.toISOString()],
    queryFn: () => fetchInterruptions(dateRange.from.toISOString(), dateRange.to.toISOString()),
  })
  const loading = metricsLoading || sessionsLoading || interruptionsLoading
  const error = metricsError ? (metricsError instanceof Error ? metricsError.message : String(metricsError)) : null

  if (loading) {
    return (
      <div className="flex h-64 items-center justify-center">
        <Spinner size="lg" />
      </div>
    )
  }

  if (error || !metrics) {
    return (
      <Card variant="danger">
        <CardContent>
          <p className="text-red-500">{error || t('common.error')}</p>
        </CardContent>
      </Card>
    )
  }

  const today = metrics.today

  if (today.focus_score === 0 && sessions.length === 0) {
    return (
      <EmptyState
        icon={<Brain className="h-8 w-8" />}
        title={t('emptyState.focus.title')}
        description={t('emptyState.focus.description')}
      />
    )
  }

  const historyData = [...metrics.history].reverse().map((m) => ({
    date: m.date.slice(5), // MM-DD
    score: m.focus_score,
    deepWork: Math.round(m.deep_work_secs / 60),
    communication: Math.round(m.communication_secs / 60),
  }))

  const avgScore =
    metrics.history.length > 0 ? metrics.history.reduce((a, b) => a + b.focus_score, 0) / metrics.history.length : 0
  const trend = today.focus_score - avgScore

  return (
    <div className="min-h-full space-y-6 p-6">
      {/* UI note */}
      <div className="flex items-center justify-between">
        <h1 className={cn(typography.h1, colors.text.primary, 'flex items-center gap-2')}>
          <FocusIcon className={iconSize.lg} />
          {t('focus.pageTitle')}
        </h1>
        <DateRangePicker
          onRangeChange={(from, to) => {
            if (from && to) {
              setDateRange({ from: new Date(from), to: new Date(to) })
            }
          }}
        />
      </div>

      {/* UI note */}
      <div id="section-score" className="grid grid-cols-2 gap-4 md:grid-cols-4">
        <Card variant="elevated" className="flex flex-col items-center justify-center p-6">
          <CircularGauge value={today.focus_score} />
          <div className="mt-2 flex items-center gap-1">
            {trend >= 0 ? (
              <TrendingUp className="h-4 w-4 text-green-500" />
            ) : (
              <TrendingDown className="h-4 w-4 text-red-500" />
            )}
            <span className={`font-medium text-sm ${trend >= 0 ? 'text-accent-green' : 'text-accent-red'}`}>
              {trend >= 0 ? '+' : ''}
              {trend.toFixed(1)}
            </span>
          </div>
        </Card>

        <StatCard
          title={t('focus.deepWork')}
          value={formatDuration(today.deep_work_secs, true)}
          icon={<Clock className="h-5 w-5 text-blue-500" />}
          color="blue"
        />

        <StatCard
          title={t('focus.communication')}
          value={formatDuration(today.communication_secs, true)}
          icon={<MessageSquare className="h-5 w-5 text-purple-500" />}
          color="purple"
        />

        <StatCard
          title={t('focus.interruptions')}
          value={`${today.interruption_count}${t('focus.times')}`}
          icon={<Zap className="h-5 w-5 text-amber-500" />}
          color="teal"
        />
      </div>

      {/* UI note */}
      <Card id="section-trend">
        <CardHeader>
          <CardTitle>{t('focus.weeklyTrend')}</CardTitle>
        </CardHeader>
        <CardContent>
          {historyData.length > 0 ? (
            <div className="h-64">
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={historyData}>
                  <CartesianGrid strokeDasharray="3 3" className="stroke-border-muted" />
                  <XAxis dataKey="date" className="text-xs" />
                  <YAxis domain={[0, 100]} className="text-xs" />
                  <Tooltip
                    contentStyle={{
                      backgroundColor: isDark ? '#1e293b' : '#ffffff',
                      border: isDark ? 'none' : '1px solid #e2e8f0',
                      borderRadius: '8px',
                      color: isDark ? '#e2e8f0' : '#334155',
                    }}
                    labelStyle={{ color: isDark ? '#94a3b8' : '#64748b' }}
                  />
                  <Line
                    type="monotone"
                    dataKey="score"
                    name={t('focus.score')}
                    stroke="#10b981"
                    strokeWidth={2}
                    dot={{ fill: '#10b981', r: 4 }}
                    activeDot={{ r: 6 }}
                  />
                </LineChart>
              </ResponsiveContainer>
            </div>
          ) : (
            <p className="py-8 text-center text-content-tertiary">{t('common.noData')}</p>
          )}
        </CardContent>
      </Card>

      {/* UI note */}
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
        {/* UI note */}
        <Card id="section-sessions">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Laptop className="h-5 w-5" />
              {t('focus.sessions')}
            </CardTitle>
          </CardHeader>
          <CardContent>
            {sessions.length > 0 ? (
              <div className="max-h-80 space-y-3 overflow-y-auto">
                {sessions.slice(0, 10).map((session) => (
                  <div key={session.id} className="flex items-center justify-between rounded-lg bg-surface-inset p-3">
                    <div className="flex items-center gap-3">
                      <div
                        className={`h-3 w-3 rounded-full ${CATEGORY_COLORS[session.category] || CATEGORY_COLORS.Other}`}
                      />
                      <div>
                        <p className="font-medium text-content text-sm">{session.primary_app}</p>
                        <p className="text-content-tertiary text-xs">{session.category}</p>
                      </div>
                    </div>
                    <div className="text-right">
                      <p className="font-medium text-content text-sm">{formatDuration(session.duration_secs, true)}</p>
                      <Badge color={session.state === 'active' ? 'success' : 'default'} size="sm">
                        {session.state === 'active' ? t('focus.active') : t('focus.completed')}
                      </Badge>
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <p className="py-8 text-center text-content-tertiary">{t('common.noData')}</p>
            )}
          </CardContent>
        </Card>

        {/* UI note */}
        <Card id="section-interruptions">
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <ArrowRightLeft className="h-5 w-5" />
              {t('focus.interruptionList')}
            </CardTitle>
          </CardHeader>
          <CardContent>
            {interruptions.length > 0 ? (
              <div className="max-h-80 space-y-3 overflow-y-auto">
                {interruptions.slice(0, 10).map((int) => (
                  <div key={int.id} className="flex items-center justify-between rounded-lg bg-surface-inset p-3">
                    <div className="flex items-center gap-2 text-sm">
                      <span className="max-w-[80px] truncate font-medium text-content">{int.from_app}</span>
                      <ArrowRightLeft className="h-4 w-4 text-content-muted" />
                      <span className="max-w-[80px] truncate font-medium text-content">{int.to_app}</span>
                    </div>
                    <div className="text-right">
                      <p className="text-content-tertiary text-xs">
                        {new Date(int.interrupted_at).toLocaleTimeString([], {
                          hour: '2-digit',
                          minute: '2-digit',
                        })}
                      </p>
                      {int.duration_secs && (
                        <p className="text-content-tertiary text-xs">{formatDuration(int.duration_secs, true)}</p>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <p className="py-8 text-center text-content-tertiary">{t('common.noData')}</p>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
