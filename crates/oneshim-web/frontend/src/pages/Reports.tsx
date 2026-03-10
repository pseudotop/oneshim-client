/**
 *
 */

import { useQuery } from '@tanstack/react-query'
import { BarChart3 } from 'lucide-react'
import { type CSSProperties, useState } from 'react'
import { useTranslation } from 'react-i18next'
import {
  Bar,
  BarChart,
  Cell,
  Line,
  LineChart,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'
import { fetchReport, type ReportPeriod, type ReportResponse } from '../api/client'
import { Badge, Button, Card, CardTitle, EmptyState, Input, Spinner } from '../components/ui'
import { useTheme } from '../contexts/ThemeContext'
import { colors, typography } from '../styles/tokens'
import { cn } from '../utils/cn'
import { formatDuration } from '../utils/formatters'

const COLORS = ['#14b8a6', '#3b82f6', '#8b5cf6', '#f59e0b', '#ef4444', '#10b981', '#6366f1', '#ec4899']

function getScoreColor(score: number): string {
  if (score >= 80) return 'text-accent-green'
  if (score >= 60) return 'text-accent-teal'
  if (score >= 40) return 'text-semantic-warning'
  return 'text-accent-red'
}

function TrendIndicator({ trend }: { trend: number }) {
  if (trend > 5) {
    return <span className="text-accent-green">↑ {trend.toFixed(1)}%</span>
  }
  if (trend < -5) {
    return <span className="text-accent-red">↓ {Math.abs(trend).toFixed(1)}%</span>
  }
  return <span className="text-content-tertiary">→ {trend.toFixed(1)}%</span>
}

export default function Reports() {
  const { t } = useTranslation()
  const { theme } = useTheme()
  const [period, setPeriod] = useState<ReportPeriod>('week')
  const [customFrom, setCustomFrom] = useState('')
  const [customTo, setCustomTo] = useState('')

  const tooltipStyle = {
    backgroundColor: theme === 'dark' ? '#1e293b' : '#ffffff',
    border: theme === 'dark' ? 'none' : '1px solid #e2e8f0',
    borderRadius: '0.5rem',
    color: theme === 'dark' ? '#e2e8f0' : '#334155',
  }

  const {
    data: report,
    isLoading,
    error,
    refetch,
  } = useQuery({
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
      <div className="flex h-64 items-center justify-center">
        <Spinner size="lg" className="text-accent-teal" />
        <span className="ml-3 text-content-secondary">{t('common.loading')}</span>
      </div>
    )
  }

  return (
    <div className="min-h-full space-y-6 p-6">
      {/* UI note */}
      <div className="flex flex-col justify-between gap-4 md:flex-row md:items-center">
        <h1 className={cn(typography.h1, colors.text.primary)}>{t('reports.title')}</h1>

        {/* UI note */}
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
            <div className="ml-2 flex items-center gap-2">
              <Input
                type="date"
                inputSize="sm"
                value={customFrom}
                onChange={(e) => setCustomFrom(e.target.value)}
                className="w-36"
              />
              <span className="text-content-tertiary">~</span>
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
          icon={<BarChart3 className="h-8 w-8" />}
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
      {/* UI note */}
      <Card padding="md">
        <div className="flex flex-col justify-between gap-4 md:flex-row md:items-center">
          <div>
            <h2 className="font-bold text-content text-xl">{report.title}</h2>
            <p className="text-content-secondary text-sm">
              {report.from_date} ~ {report.to_date} ({report.days} {t('reports.days')})
            </p>
          </div>
          <div className="flex items-center gap-4">
            {/* UI note */}
            <div className="text-center">
              <p className={`font-bold text-4xl ${getScoreColor(report.productivity.score)}`}>
                {report.productivity.score.toFixed(0)}
              </p>
              <p className="text-content-secondary text-xs">{t('reports.productivityScore')}</p>
            </div>
            {/* UI note */}
            <div className="text-center">
              <p className="font-semibold text-xl">
                <TrendIndicator trend={report.productivity.trend} />
              </p>
              <p className="text-content-secondary text-xs">{t('reports.trend')}</p>
            </div>
          </div>
        </div>
      </Card>

      {/* UI note */}
      <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
        <Card padding="md">
          <p className="text-content-secondary text-sm">{t('reports.activeTime')}</p>
          <p className="font-bold text-2xl text-content">{formatDuration(report.total_active_secs)}</p>
        </Card>
        <Card padding="md">
          <p className="text-content-secondary text-sm">{t('reports.idleTime')}</p>
          <p className="font-bold text-2xl text-content">{formatDuration(report.total_idle_secs)}</p>
        </Card>
        <Card padding="md">
          <p className="text-content-secondary text-sm">{t('reports.captures')}</p>
          <p className="font-bold text-2xl text-content">{report.total_captures.toLocaleString()}</p>
        </Card>
        <Card padding="md">
          <p className="text-content-secondary text-sm">{t('reports.events')}</p>
          <p className="font-bold text-2xl text-content">{report.total_events.toLocaleString()}</p>
        </Card>
      </div>

      {/* UI note */}
      <Card id="section-focus" padding="md">
        <CardTitle>{t('reports.productivityMetrics')}</CardTitle>
        <div className="mt-4 grid grid-cols-2 gap-4 md:grid-cols-4">
          <div>
            <p className="text-content-secondary text-sm">{t('reports.activeRatio')}</p>
            <p className="font-semibold text-content text-xl">{report.productivity.active_ratio.toFixed(1)}%</p>
          </div>
          <div>
            <p className="text-content-secondary text-sm">{t('reports.peakHour')}</p>
            <p className="font-semibold text-content text-xl">{report.productivity.peak_hour}:00</p>
          </div>
          <div>
            <p className="text-content-secondary text-sm">{t('reports.topApp')}</p>
            <p className="truncate font-semibold text-content text-xl">{report.productivity.top_app || '-'}</p>
          </div>
          <div>
            <p className="text-content-secondary text-sm">{t('reports.avgCpu')}</p>
            <p className="font-semibold text-content text-xl">{report.avg_cpu.toFixed(1)}%</p>
          </div>
        </div>
      </Card>

      {/* UI note */}
      <Card id="section-activity" padding="md">
        <CardTitle>{t('reports.dailyActivity')}</CardTitle>
        <div className="mt-4 h-64">
          <ResponsiveContainer width="100%" height="100%">
            <BarChart data={report.daily_stats}>
              <XAxis dataKey="date" tickFormatter={(d) => d.slice(5)} tick={{ fill: '#94a3b8', fontSize: 12 }} />
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

      {/* UI note */}
      <Card padding="md">
        <CardTitle>{t('reports.hourlyActivity')}</CardTitle>
        <div className="mt-4 h-48">
          <ResponsiveContainer width="100%" height="100%">
            <LineChart data={report.hourly_activity}>
              <XAxis dataKey="hour" tickFormatter={(h) => `${h}:00`} tick={{ fill: '#94a3b8', fontSize: 12 }} />
              <YAxis tick={{ fill: '#94a3b8', fontSize: 12 }} />
              <Tooltip contentStyle={tooltipStyle} labelFormatter={(h) => `${h}:00`} />
              <Line type="monotone" dataKey="activity" stroke="#14b8a6" strokeWidth={2} dot={false} />
            </LineChart>
          </ResponsiveContainer>
        </div>
      </Card>

      {/* UI note */}
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
        {/* UI note */}
        <Card padding="md">
          <CardTitle>{t('reports.appUsage')}</CardTitle>
          <div className="mt-4 space-y-2">
            {report.app_stats.map((app, idx) => (
              <div key={app.name} className="flex items-center justify-between rounded bg-hover/50 p-2">
                <div className="flex items-center gap-3">
                  <div className="h-3 w-3 rounded-full" style={{ backgroundColor: COLORS[idx % COLORS.length] }} />
                  <span className="max-w-[150px] truncate font-medium text-content">{app.name}</span>
                </div>
                <div className="flex items-center gap-4 text-sm">
                  <span className="text-content-secondary">{formatDuration(app.duration_secs)}</span>
                  <Badge color="primary" size="sm">
                    {app.percentage.toFixed(1)}%
                  </Badge>
                </div>
              </div>
            ))}
            {report.app_stats.length === 0 && (
              <p className="py-4 text-center text-content-secondary">{t('common.noData')}</p>
            )}
          </div>
        </Card>

        {/* UI note */}
        <Card padding="md">
          <CardTitle>{t('reports.appDistribution')}</CardTitle>
          <div className="mt-4 h-64">
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
                    {report.app_stats.map((stat, index) => (
                      <Cell key={stat.name} fill={COLORS[index % COLORS.length]} />
                    ))}
                  </Pie>
                  <Tooltip contentStyle={tooltipStyle} formatter={(value: number) => formatDuration(value)} />
                </PieChart>
              </ResponsiveContainer>
            ) : (
              <div className="flex h-full items-center justify-center text-content-secondary">{t('common.noData')}</div>
            )}
          </div>
        </Card>
      </div>

      {/* UI note */}
      <Card id="section-export" padding="md">
        <CardTitle>{t('reports.systemMetrics')}</CardTitle>
        <div className="mt-4 h-48">
          <ResponsiveContainer width="100%" height="100%">
            <LineChart data={report.daily_stats}>
              <XAxis dataKey="date" tickFormatter={(d) => d.slice(5)} tick={{ fill: '#94a3b8', fontSize: 12 }} />
              <YAxis domain={[0, 100]} tick={{ fill: '#94a3b8', fontSize: 12 }} />
              <Tooltip
                contentStyle={tooltipStyle}
                labelStyle={{ color: theme === 'dark' ? '#e2e8f0' : '#334155' }}
                formatter={(value: number) => [`${value.toFixed(1)}%`, '']}
              />
              <Line type="monotone" dataKey="cpu_avg" name="CPU" stroke="#f59e0b" strokeWidth={2} dot={false} />
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
        <div className="mt-2 flex justify-center gap-6 text-sm">
          <div className="flex items-center gap-2">
            <div className="h-3 w-3 rounded-full bg-amber-500" />
            <span className="text-content-secondary">CPU</span>
          </div>
          <div className="flex items-center gap-2">
            <div className="h-3 w-3 rounded-full bg-violet-500" />
            <span className="text-content-secondary">{t('reports.memory')}</span>
          </div>
        </div>
      </Card>
    </>
  )
}
