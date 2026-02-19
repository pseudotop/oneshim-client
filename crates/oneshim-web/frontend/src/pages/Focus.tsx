import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import {
  Focus as FocusIcon,
  Clock,
  MessageSquare,
  Zap,
  TrendingUp,
  TrendingDown,
  ArrowRightLeft,
  Laptop,
  Brain,
} from 'lucide-react'
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
} from 'recharts'
import { Card, CardHeader, CardTitle, CardContent } from '../components/ui/Card'
import { Spinner } from '../components/ui/Spinner'
import { Badge } from '../components/ui/Badge'
import { EmptyState } from '../components/ui'
import { useTheme } from '../contexts/ThemeContext'
import DateRangePicker from '../components/DateRangePicker'
import StatCard from '../components/StatCard'
import {
  fetchFocusMetrics,
  fetchWorkSessions,
  fetchInterruptions,
} from '../api/client'
import { formatDuration } from '../utils/formatters'

/** 카테고리별 색상 */
const CATEGORY_COLORS: Record<string, string> = {
  Development: 'bg-blue-500',
  Communication: 'bg-purple-500',
  Documentation: 'bg-green-500',
  Browser: 'bg-amber-500',
  Design: 'bg-pink-500',
  Media: 'bg-red-500',
  System: 'bg-slate-500',
  Other: 'bg-gray-500',
}

/** 원형 게이지 컴포넌트 */
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
    <svg width={size} height={size} viewBox="0 0 100 100">
      <circle
        cx="50"
        cy="50"
        r="45"
        fill="none"
        stroke="currentColor"
        strokeWidth="8"
        className="text-slate-200 dark:text-slate-700"
      />
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
        className="transition-all duration-700"
      />
      <text
        x="50"
        y="45"
        textAnchor="middle"
        dominantBaseline="middle"
        className="fill-slate-900 dark:fill-white text-2xl font-bold"
      >
        {Math.round(value)}
      </text>
      <text
        x="50"
        y="62"
        textAnchor="middle"
        className="fill-slate-500 dark:fill-slate-400 text-xs"
      >
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

  const { data: metrics, isLoading: metricsLoading, error: metricsError } = useQuery({
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
      <div className="flex justify-center items-center h-64">
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

  // 데이터가 없는 경우 빈 상태 표시
  if (today.focus_score === 0 && sessions.length === 0) {
    return (
      <EmptyState
        icon={<Brain className="w-8 h-8" />}
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

  // 트렌드 계산
  const avgScore =
    metrics.history.length > 0
      ? metrics.history.reduce((a, b) => a + b.focus_score, 0) / metrics.history.length
      : 0
  const trend = today.focus_score - avgScore

  return (
    <div className="space-y-6">
      {/* 헤더 */}
      <div className="flex justify-between items-center">
        <h1 className="text-2xl font-bold text-slate-900 dark:text-white flex items-center gap-2">
          <FocusIcon className="w-7 h-7" />
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

      {/* 메트릭 카드 */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <Card variant="elevated" className="flex flex-col items-center justify-center p-6">
          <CircularGauge value={today.focus_score} />
          <div className="mt-2 flex items-center gap-1">
            {trend >= 0 ? (
              <TrendingUp className="w-4 h-4 text-green-500" />
            ) : (
              <TrendingDown className="w-4 h-4 text-red-500" />
            )}
            <span
              className={`text-sm font-medium ${trend >= 0 ? 'text-green-600 dark:text-green-400' : 'text-red-600 dark:text-red-400'}`}
            >
              {trend >= 0 ? '+' : ''}
              {trend.toFixed(1)}
            </span>
          </div>
        </Card>

        <StatCard
          title={t('focus.deepWork')}
          value={formatDuration(today.deep_work_secs, true)}
          icon={<Clock className="w-5 h-5 text-blue-500" />}
          color="blue"
        />

        <StatCard
          title={t('focus.communication')}
          value={formatDuration(today.communication_secs, true)}
          icon={<MessageSquare className="w-5 h-5 text-purple-500" />}
          color="purple"
        />

        <StatCard
          title={t('focus.interruptions')}
          value={`${today.interruption_count}${t('focus.times')}`}
          icon={<Zap className="w-5 h-5 text-amber-500" />}
          color="teal"
        />
      </div>

      {/* 주간 트렌드 차트 */}
      <Card>
        <CardHeader>
          <CardTitle>{t('focus.weeklyTrend')}</CardTitle>
        </CardHeader>
        <CardContent>
          {historyData.length > 0 ? (
            <div className="h-64">
              <ResponsiveContainer width="100%" height="100%">
                <LineChart data={historyData}>
                  <CartesianGrid strokeDasharray="3 3" className="stroke-slate-200 dark:stroke-slate-700" />
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
            <p className="text-center text-slate-500 dark:text-slate-400 py-8">
              {t('common.noData')}
            </p>
          )}
        </CardContent>
      </Card>

      {/* 작업 세션 + 인터럽션 */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* 작업 세션 목록 */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <Laptop className="w-5 h-5" />
              {t('focus.sessions')}
            </CardTitle>
          </CardHeader>
          <CardContent>
            {sessions.length > 0 ? (
              <div className="space-y-3 max-h-80 overflow-y-auto">
                {sessions.slice(0, 10).map((session) => (
                  <div
                    key={session.id}
                    className="flex items-center justify-between p-3 rounded-lg bg-slate-50 dark:bg-slate-800/50"
                  >
                    <div className="flex items-center gap-3">
                      <div
                        className={`w-3 h-3 rounded-full ${CATEGORY_COLORS[session.category] || CATEGORY_COLORS.Other}`}
                      />
                      <div>
                        <p className="text-sm font-medium text-slate-900 dark:text-white">
                          {session.primary_app}
                        </p>
                        <p className="text-xs text-slate-500 dark:text-slate-400">
                          {session.category}
                        </p>
                      </div>
                    </div>
                    <div className="text-right">
                      <p className="text-sm font-medium text-slate-900 dark:text-white">
                        {formatDuration(session.duration_secs, true)}
                      </p>
                      <Badge color={session.state === 'active' ? 'success' : 'default'} size="sm">
                        {session.state === 'active' ? t('focus.active') : t('focus.completed')}
                      </Badge>
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <p className="text-center text-slate-500 dark:text-slate-400 py-8">
                {t('common.noData')}
              </p>
            )}
          </CardContent>
        </Card>

        {/* 인터럽션 목록 */}
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <ArrowRightLeft className="w-5 h-5" />
              {t('focus.interruptionList')}
            </CardTitle>
          </CardHeader>
          <CardContent>
            {interruptions.length > 0 ? (
              <div className="space-y-3 max-h-80 overflow-y-auto">
                {interruptions.slice(0, 10).map((int) => (
                  <div
                    key={int.id}
                    className="flex items-center justify-between p-3 rounded-lg bg-slate-50 dark:bg-slate-800/50"
                  >
                    <div className="flex items-center gap-2 text-sm">
                      <span className="font-medium text-slate-900 dark:text-white truncate max-w-[80px]">
                        {int.from_app}
                      </span>
                      <ArrowRightLeft className="w-4 h-4 text-slate-400" />
                      <span className="font-medium text-slate-900 dark:text-white truncate max-w-[80px]">
                        {int.to_app}
                      </span>
                    </div>
                    <div className="text-right">
                      <p className="text-xs text-slate-500 dark:text-slate-400">
                        {new Date(int.interrupted_at).toLocaleTimeString([], {
                          hour: '2-digit',
                          minute: '2-digit',
                        })}
                      </p>
                      {int.duration_secs && (
                        <p className="text-xs text-slate-500 dark:text-slate-400">
                          {formatDuration(int.duration_secs, true)}
                        </p>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <p className="text-center text-slate-500 dark:text-slate-400 py-8">
                {t('common.noData')}
              </p>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
