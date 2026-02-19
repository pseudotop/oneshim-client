/**
 * 리포트 페이지
 *
 * 주간/월간 활동 리포트 및 생산성 지표 표시
 */
import { useState, type CSSProperties } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import {
  BarChart,
  Bar,
  LineChart,
  Line,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  PieChart,
  Pie,
  Cell,
} from 'recharts'
import { BarChart3 } from 'lucide-react'
import { fetchReport, ReportPeriod, ReportResponse } from '../api/client'
import { Card, CardTitle, Button, Spinner, Badge, Input, EmptyState } from '../components/ui'
import { formatDuration } from '../utils/formatters'
import { useTheme } from '../contexts/ThemeContext'

// 차트 색상
const COLORS = ['#14b8a6', '#3b82f6', '#8b5cf6', '#f59e0b', '#ef4444', '#10b981', '#6366f1', '#ec4899']

// 생산성 점수 색상
function getScoreColor(score: number): string {
  if (score >= 80) return 'text-green-500'
  if (score >= 60) return 'text-teal-500'
  if (score >= 40) return 'text-yellow-500'
  return 'text-red-500'
}

// 추세 아이콘
function TrendIndicator({ trend }: { trend: number }) {
  if (trend > 5) {
    return <span className="text-green-500">↑ {trend.toFixed(1)}%</span>
  }
  if (trend < -5) {
    return <span className="text-red-500">↓ {Math.abs(trend).toFixed(1)}%</span>
  }
  return <span className="text-slate-500">→ {trend.toFixed(1)}%</span>
}

export default function Reports() {
  const { t } = useTranslation()
  const { theme } = useTheme()
  const [period, setPeriod] = useState<ReportPeriod>('week')
  const [customFrom, setCustomFrom] = useState('')
  const [customTo, setCustomTo] = useState('')

  // 차트 툴팁 스타일 (테마 대응)
  const tooltipStyle = {
    backgroundColor: theme === 'dark' ? '#1e293b' : '#ffffff',
    border: theme === 'dark' ? 'none' : '1px solid #e2e8f0',
    borderRadius: '0.5rem',
    color: theme === 'dark' ? '#e2e8f0' : '#334155',
  }

  const { data: report, isLoading, error, refetch } = useQuery({
    queryKey: ['report', period, customFrom, customTo],
    queryFn: () =>
      fetchReport({
        period,
        from: period === 'custom' ? customFrom : undefined,
        to: period === 'custom' ? customTo : undefined,
      }),
    enabled: period !== 'custom' || (!!customFrom && !!customTo),
  })

  const handlePeriodChange = (newPeriod: ReportPeriod) => {
    setPeriod(newPeriod)
  }

  const handleCustomSearch = () => {
    if (customFrom && customTo) {
      refetch()
    }
  }

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Spinner size="lg" className="text-teal-500" />
        <span className="ml-3 text-slate-600 dark:text-slate-400">{t('common.loading')}</span>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {/* 헤더 */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <h1 className="text-2xl font-bold text-slate-900 dark:text-white">{t('reports.title')}</h1>

        {/* 기간 선택 */}
        <div className="flex flex-wrap items-center gap-2">
          <Button
            variant={period === 'week' ? 'primary' : 'secondary'}
            size="sm"
            onClick={() => handlePeriodChange('week')}
          >
            {t('reports.week')}
          </Button>
          <Button
            variant={period === 'month' ? 'primary' : 'secondary'}
            size="sm"
            onClick={() => handlePeriodChange('month')}
          >
            {t('reports.month')}
          </Button>
          <Button
            variant={period === 'custom' ? 'primary' : 'secondary'}
            size="sm"
            onClick={() => handlePeriodChange('custom')}
          >
            {t('reports.custom')}
          </Button>

          {period === 'custom' && (
            <div className="flex items-center gap-2 ml-2">
              <Input
                type="date"
                inputSize="sm"
                value={customFrom}
                onChange={(e) => setCustomFrom(e.target.value)}
                className="w-36"
              />
              <span className="text-slate-500">~</span>
              <Input
                type="date"
                inputSize="sm"
                value={customTo}
                onChange={(e) => setCustomTo(e.target.value)}
                className="w-36"
              />
              <Button variant="primary" size="sm" onClick={handleCustomSearch}>
                {t('reports.generate')}
              </Button>
            </div>
          )}
        </div>
      </div>

      {error && (
        <Card variant="danger" padding="md">
          <p className="text-red-400">{t('reports.error')}</p>
        </Card>
      )}

      {!report && !isLoading && !error && (
        <EmptyState
          icon={<BarChart3 className="w-8 h-8" />}
          title={t('emptyState.reports.title')}
          description={t('emptyState.reports.description')}
        />
      )}

      {report && <ReportContent report={report} t={t} tooltipStyle={tooltipStyle} theme={theme} />}
    </div>
  )
}

interface ReportContentProps {
  report: ReportResponse
  t: (key: string) => string
  tooltipStyle: CSSProperties
  theme: string
}

function ReportContent({ report, t, tooltipStyle, theme }: ReportContentProps) {
  return (
    <>
      {/* 리포트 제목 및 기간 */}
      <Card padding="md">
        <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
          <div>
            <h2 className="text-xl font-bold text-slate-900 dark:text-white">{report.title}</h2>
            <p className="text-sm text-slate-500 dark:text-slate-400">
              {report.from_date} ~ {report.to_date} ({report.days} {t('reports.days')})
            </p>
          </div>
          <div className="flex items-center gap-4">
            {/* 생산성 점수 */}
            <div className="text-center">
              <p className={`text-4xl font-bold ${getScoreColor(report.productivity.score)}`}>
                {report.productivity.score.toFixed(0)}
              </p>
              <p className="text-xs text-slate-500 dark:text-slate-400">{t('reports.productivityScore')}</p>
            </div>
            {/* 추세 */}
            <div className="text-center">
              <p className="text-xl font-semibold">
                <TrendIndicator trend={report.productivity.trend} />
              </p>
              <p className="text-xs text-slate-500 dark:text-slate-400">{t('reports.trend')}</p>
            </div>
          </div>
        </div>
      </Card>

      {/* 요약 통계 카드 */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <Card padding="md">
          <p className="text-sm text-slate-500 dark:text-slate-400">{t('reports.activeTime')}</p>
          <p className="text-2xl font-bold text-slate-900 dark:text-white">
            {formatDuration(report.total_active_secs)}
          </p>
        </Card>
        <Card padding="md">
          <p className="text-sm text-slate-500 dark:text-slate-400">{t('reports.idleTime')}</p>
          <p className="text-2xl font-bold text-slate-900 dark:text-white">
            {formatDuration(report.total_idle_secs)}
          </p>
        </Card>
        <Card padding="md">
          <p className="text-sm text-slate-500 dark:text-slate-400">{t('reports.captures')}</p>
          <p className="text-2xl font-bold text-slate-900 dark:text-white">
            {report.total_captures.toLocaleString()}
          </p>
        </Card>
        <Card padding="md">
          <p className="text-sm text-slate-500 dark:text-slate-400">{t('reports.events')}</p>
          <p className="text-2xl font-bold text-slate-900 dark:text-white">
            {report.total_events.toLocaleString()}
          </p>
        </Card>
      </div>

      {/* 생산성 지표 */}
      <Card padding="md">
        <CardTitle>{t('reports.productivityMetrics')}</CardTitle>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mt-4">
          <div>
            <p className="text-sm text-slate-500 dark:text-slate-400">{t('reports.activeRatio')}</p>
            <p className="text-xl font-semibold text-slate-900 dark:text-white">
              {report.productivity.active_ratio.toFixed(1)}%
            </p>
          </div>
          <div>
            <p className="text-sm text-slate-500 dark:text-slate-400">{t('reports.peakHour')}</p>
            <p className="text-xl font-semibold text-slate-900 dark:text-white">
              {report.productivity.peak_hour}:00
            </p>
          </div>
          <div>
            <p className="text-sm text-slate-500 dark:text-slate-400">{t('reports.topApp')}</p>
            <p className="text-xl font-semibold text-slate-900 dark:text-white truncate">
              {report.productivity.top_app || '-'}
            </p>
          </div>
          <div>
            <p className="text-sm text-slate-500 dark:text-slate-400">{t('reports.avgCpu')}</p>
            <p className="text-xl font-semibold text-slate-900 dark:text-white">
              {report.avg_cpu.toFixed(1)}%
            </p>
          </div>
        </div>
      </Card>

      {/* 일별 활동 차트 */}
      <Card padding="md">
        <CardTitle>{t('reports.dailyActivity')}</CardTitle>
        <div className="h-64 mt-4">
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={report.daily_stats}>
              <XAxis
                dataKey="date"
                tickFormatter={(d) => d.slice(5)}
                tick={{ fill: '#94a3b8', fontSize: 12 }}
              />
              <YAxis tick={{ fill: '#94a3b8', fontSize: 12 }} />
              <Tooltip
                contentStyle={tooltipStyle}
                labelStyle={{ color: theme === 'dark' ? '#e2e8f0' : '#334155' }}
                formatter={(value: number) => [value.toLocaleString(), '']}
              />
              <Bar dataKey="events" name={t('reports.events')} fill="#14b8a6" />
              <Bar dataKey="captures" name={t('reports.captures')} fill="#3b82f6" />
            </BarChart>
          </ResponsiveContainer>
        </div>
      </Card>

      {/* 시간대별 활동 차트 */}
      <Card padding="md">
        <CardTitle>{t('reports.hourlyActivity')}</CardTitle>
        <div className="h-48 mt-4">
          <ResponsiveContainer width="100%" height="100%">
            <LineChart data={report.hourly_activity}>
              <XAxis
                dataKey="hour"
                tickFormatter={(h) => `${h}:00`}
                tick={{ fill: '#94a3b8', fontSize: 12 }}
              />
              <YAxis tick={{ fill: '#94a3b8', fontSize: 12 }} />
              <Tooltip
                contentStyle={tooltipStyle}
                labelFormatter={(h) => `${h}:00`}
              />
              <Line type="monotone" dataKey="activity" stroke="#14b8a6" strokeWidth={2} dot={false} />
            </LineChart>
          </ResponsiveContainer>
        </div>
      </Card>

      {/* 앱 사용량 (테이블 + 파이 차트) */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* 앱 테이블 */}
        <Card padding="md">
          <CardTitle>{t('reports.appUsage')}</CardTitle>
          <div className="mt-4 space-y-2">
            {report.app_stats.map((app, idx) => (
              <div
                key={app.name}
                className="flex items-center justify-between p-2 rounded bg-slate-100 dark:bg-slate-700/50"
              >
                <div className="flex items-center gap-3">
                  <div
                    className="w-3 h-3 rounded-full"
                    style={{ backgroundColor: COLORS[idx % COLORS.length] }}
                  />
                  <span className="font-medium text-slate-900 dark:text-white truncate max-w-[150px]">
                    {app.name}
                  </span>
                </div>
                <div className="flex items-center gap-4 text-sm">
                  <span className="text-slate-500 dark:text-slate-400">
                    {formatDuration(app.duration_secs)}
                  </span>
                  <Badge color="primary" size="sm">
                    {app.percentage.toFixed(1)}%
                  </Badge>
                </div>
              </div>
            ))}
            {report.app_stats.length === 0 && (
              <p className="text-center text-slate-500 dark:text-slate-400 py-4">{t('common.noData')}</p>
            )}
          </div>
        </Card>

        {/* 파이 차트 */}
        <Card padding="md">
          <CardTitle>{t('reports.appDistribution')}</CardTitle>
          <div className="h-64 mt-4">
            {report.app_stats.length > 0 ? (
              <ResponsiveContainer width="100%" height="100%">
                <PieChart>
                  <Pie
                    data={report.app_stats}
                    dataKey="duration_secs"
                    nameKey="name"
                    cx="50%"
                    cy="50%"
                    outerRadius={80}
                    label={({ name, percentage }) => `${name.slice(0, 10)} (${percentage.toFixed(0)}%)`}
                    labelLine={false}
                  >
                    {report.app_stats.map((_, index) => (
                      <Cell key={`cell-${index}`} fill={COLORS[index % COLORS.length]} />
                    ))}
                  </Pie>
                  <Tooltip
                    contentStyle={tooltipStyle}
                    formatter={(value: number) => formatDuration(value)}
                  />
                </PieChart>
              </ResponsiveContainer>
            ) : (
              <div className="flex items-center justify-center h-full text-slate-500 dark:text-slate-400">
                {t('common.noData')}
              </div>
            )}
          </div>
        </Card>
      </div>

      {/* CPU/메모리 일별 추이 */}
      <Card padding="md">
        <CardTitle>{t('reports.systemMetrics')}</CardTitle>
        <div className="h-48 mt-4">
          <ResponsiveContainer width="100%" height="100%">
            <LineChart data={report.daily_stats}>
              <XAxis
                dataKey="date"
                tickFormatter={(d) => d.slice(5)}
                tick={{ fill: '#94a3b8', fontSize: 12 }}
              />
              <YAxis domain={[0, 100]} tick={{ fill: '#94a3b8', fontSize: 12 }} />
              <Tooltip
                contentStyle={tooltipStyle}
                labelStyle={{ color: theme === 'dark' ? '#e2e8f0' : '#334155' }}
                formatter={(value: number) => [`${value.toFixed(1)}%`, '']}
              />
              <Line
                type="monotone"
                dataKey="cpu_avg"
                name="CPU"
                stroke="#f59e0b"
                strokeWidth={2}
                dot={false}
              />
              <Line
                type="monotone"
                dataKey="memory_avg"
                name={t('reports.memory')}
                stroke="#8b5cf6"
                strokeWidth={2}
                dot={false}
              />
            </LineChart>
          </ResponsiveContainer>
        </div>
        <div className="flex justify-center gap-6 mt-2 text-sm">
          <div className="flex items-center gap-2">
            <div className="w-3 h-3 rounded-full bg-amber-500" />
            <span className="text-slate-600 dark:text-slate-400">CPU</span>
          </div>
          <div className="flex items-center gap-2">
            <div className="w-3 h-3 rounded-full bg-violet-500" />
            <span className="text-slate-600 dark:text-slate-400">{t('reports.memory')}</span>
          </div>
        </div>
      </Card>
    </>
  )
}
