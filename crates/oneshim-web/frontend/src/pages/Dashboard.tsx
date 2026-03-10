/**
 *
 */

import { useQuery } from '@tanstack/react-query'
import { BarChart3, Camera, Clock, Monitor, Moon } from 'lucide-react'
import { useCallback, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { fetchHourlyMetrics, fetchProcesses, fetchSummary } from '../api/client'
import { ActivityHeatmap } from '../components/ActivityHeatmap'
import AppUsageChart from '../components/AppUsageChart'
import DateRangePicker from '../components/DateRangePicker'
import FocusWidget from '../components/FocusWidget'
import MetricsChart from '../components/MetricsChart'
import ProcessList from '../components/ProcessList'
import StatCard from '../components/StatCard'
import UpdatePanel from '../components/UpdatePanel'
import { Badge, Card, CardTitle, EmptyState, Spinner } from '../components/ui'
import { type ConnectionStatus, useSSE } from '../hooks/useSSE'
import { colors, typography } from '../styles/tokens'
import { cn } from '../utils/cn'
import { formatDuration } from '../utils/formatters'

function toApiDate(dateInput?: string): string {
  if (!dateInput) {
    return new Date().toISOString().split('T')[0]
  }

  const parsed = new Date(dateInput)
  if (Number.isNaN(parsed.getTime())) {
    return new Date().toISOString().split('T')[0]
  }

  return parsed.toISOString().split('T')[0]
}

function ConnectionIndicator({ status, t }: { status: ConnectionStatus; t: (key: string) => string }) {
  const statusConfig = {
    connecting: { color: colors.status.connecting, textKey: 'dashboard.connecting' },
    connected: { color: colors.status.connected, textKey: 'dashboard.connected' },
    disconnected: { color: colors.status.disconnected, textKey: 'dashboard.disconnected' },
    error: { color: colors.status.error, textKey: 'dashboard.error' },
  }
  const config = statusConfig[status]

  return (
    <div className={cn('flex items-center space-x-2', typography.body)}>
      <span className={cn('h-2 w-2 rounded-full', config.color, status === 'connected' && 'animate-pulse')} />
      <span className={colors.text.secondary}>{t(config.textKey)}</span>
    </div>
  )
}

export default function Dashboard() {
  const { t } = useTranslation()
  const [dateRange, setDateRange] = useState<{ from?: string; to?: string }>({})

  const { status, latestMetrics, idleState, metricsHistory } = useSSE()

  const handleRangeChange = useCallback((from: string | undefined, to: string | undefined) => {
    setDateRange({ from, to })
  }, [])

  const selectedDate = toApiDate(dateRange.from)

  const { data: summary, isLoading: summaryLoading } = useQuery({
    queryKey: ['summary', selectedDate],
    queryFn: () => fetchSummary(selectedDate),
  })

  const { data: hourlyMetrics } = useQuery({
    queryKey: ['hourlyMetrics'],
    queryFn: () => fetchHourlyMetrics(24),
  })

  const { data: processes } = useQuery({
    queryKey: ['processes'],
    queryFn: () => fetchProcesses(undefined, undefined, 5),
  })

  if (summaryLoading) {
    return (
      <div className="flex h-64 items-center justify-center">
        <Spinner size="lg" className={colors.primary.text} />
        <span className={cn('ml-3', colors.text.secondary)}>{t('common.loading')}</span>
      </div>
    )
  }

  if (
    !latestMetrics &&
    !summary?.events_logged &&
    !summary?.frames_captured &&
    (summary?.total_active_secs ?? 0) === 0
  ) {
    return (
      <EmptyState
        icon={<Monitor className="h-8 w-8" />}
        title={t('emptyState.dashboard.title')}
        description={t('emptyState.dashboard.description')}
      />
    )
  }

  return (
    <div className="min-h-full space-y-6 p-6">
      {/* UI note */}
      <div id="section-overview" className="flex flex-col justify-between gap-4 md:flex-row md:items-center">
        <div className="flex items-center space-x-4">
          <h1 className={cn(typography.h1, colors.text.primary)}>{t('dashboard.title')}</h1>
          <ConnectionIndicator status={status} t={t} />
        </div>
        <DateRangePicker onRangeChange={handleRangeChange} />
      </div>

      {/* UI note */}
      {latestMetrics && (
        <Card variant="highlight" padding="md">
          <div className="flex items-center justify-between">
            <div className="flex items-center space-x-2">
              <span className={cn('h-2 w-2 animate-pulse rounded-full', colors.primary.signal)} />
              <span className={cn('font-medium text-sm', colors.primary.text)}>
                {t('dashboard.realtimeMonitoring')}
              </span>
              {idleState?.is_idle && (
                <Badge color="warning" size="sm">
                  {t('dashboard.idle')} {Math.floor((idleState.idle_secs || 0) / 60)}
                  {t('dashboard.minutes')}
                </Badge>
              )}
            </div>
            <div className={cn('text-xs', colors.text.secondary)}>
              {new Date(latestMetrics.timestamp).toLocaleTimeString()}
            </div>
          </div>
          <div className="mt-3 grid grid-cols-2 gap-4 md:grid-cols-4">
            <div>
              <div className={cn(typography.stat.large, colors.accent.teal)}>{latestMetrics.cpu_usage.toFixed(1)}%</div>
              <div className={cn('text-xs', colors.text.secondary)}>{t('dashboard.cpu')}</div>
            </div>
            <div>
              <div className={cn(typography.stat.large, colors.accent.blue)}>
                {latestMetrics.memory_percent.toFixed(1)}%
              </div>
              <div className={cn('text-xs', colors.text.secondary)}>{t('dashboard.memory')}</div>
            </div>
            <div>
              <div className={cn(typography.stat.large, colors.accent.purple)}>
                {(latestMetrics.memory_used / 1024 / 1024 / 1024).toFixed(1)} GB
              </div>
              <div className={cn('text-xs', colors.text.secondary)}>{t('dashboard.usedMemory')}</div>
            </div>
            <div>
              <div className={cn(typography.stat.large, colors.accent.slate)}>{metricsHistory.length}</div>
              <div className={cn('text-xs', colors.text.secondary)}>{t('dashboard.collectedData')}</div>
            </div>
          </div>
        </Card>
      )}

      {/* UI note */}
      <div id="section-focus">
        <FocusWidget />
      </div>

      <div id="section-updates">
        <UpdatePanel compact />
      </div>

      {/* UI note */}
      <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
        <StatCard
          title={t('dashboard.activeTime')}
          value={formatDuration(summary?.total_active_secs ?? 0)}
          icon={<Clock className="h-5 w-5" />}
          color="teal"
        />
        <StatCard
          title={t('dashboard.idleTime')}
          value={formatDuration(summary?.total_idle_secs ?? 0)}
          icon={<Moon className="h-5 w-5" />}
          color="slate"
        />
        <StatCard
          title={t('dashboard.captures')}
          value={summary?.frames_captured?.toLocaleString() ?? '0'}
          icon={<Camera className="h-5 w-5" />}
          color="blue"
        />
        <StatCard
          title={t('dashboard.events')}
          value={summary?.events_logged?.toLocaleString() ?? '0'}
          icon={<BarChart3 className="h-5 w-5" />}
          color="purple"
        />
      </div>

      {/* UI note */}
      <Card id="section-metrics" variant="default" padding="lg">
        <CardTitle className="mb-4">{t('dashboard.cpuMemory24h')}</CardTitle>
        <MetricsChart data={hourlyMetrics ?? []} />
      </Card>

      {/* UI note */}
      <div id="section-processes" className="grid grid-cols-1 gap-6 lg:grid-cols-2">
        {/* UI note */}
        <Card variant="default" padding="lg">
          <CardTitle className="mb-4">{t('dashboard.appUsageTime')}</CardTitle>
          <AppUsageChart apps={summary?.top_apps ?? []} />
        </Card>

        {/* UI note */}
        <Card variant="default" padding="lg">
          <CardTitle className="mb-4">{t('dashboard.recentProcesses')}</CardTitle>
          {processes && processes.length > 0 ? (
            <ProcessList snapshot={processes[0]} />
          ) : (
            <div className={cn(colors.text.secondary, 'py-8 text-center')}>{t('common.noData')}</div>
          )}
        </Card>
      </div>

      {/* UI note */}
      <div id="section-heatmap">
        <ActivityHeatmap days={7} className={colors.surface.elevated} />
      </div>

      {/* UI note */}
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('dashboard.systemStatus')}</CardTitle>
        <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
          <div className="text-center">
            <div className={cn(typography.stat.hero, colors.accent.teal)}>{summary?.cpu_avg?.toFixed(1) ?? '0'}%</div>
            <div className={cn('text-sm', colors.text.secondary)}>{t('dashboard.avgCpu')}</div>
          </div>
          <div className="text-center">
            <div className={cn(typography.stat.hero, colors.accent.blue)}>
              {summary?.memory_avg_percent?.toFixed(1) ?? '0'}%
            </div>
            <div className={cn('text-sm', colors.text.secondary)}>{t('dashboard.avgMemory')}</div>
          </div>
          <div className="text-center">
            <div className={cn(typography.stat.hero, colors.accent.purple)}>{summary?.top_apps?.length ?? 0}</div>
            <div className={cn('text-sm', colors.text.secondary)}>{t('dashboard.appsUsed')}</div>
          </div>
          <div className="text-center">
            <div className={cn(typography.stat.hero, colors.accent.green)}>
              {(
                ((summary?.total_active_secs ?? 0) /
                  ((summary?.total_active_secs ?? 0) + (summary?.total_idle_secs ?? 1))) *
                100
              ).toFixed(0)}
              %
            </div>
            <div className={cn('text-sm', colors.text.secondary)}>{t('dashboard.activityRatio')}</div>
          </div>
        </div>
      </Card>
    </div>
  )
}
