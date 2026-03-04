/**
 *
 */
import { useState, useCallback } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { Clock, Moon, Camera, BarChart3, Monitor } from 'lucide-react'
import { fetchSummary, fetchHourlyMetrics, fetchProcesses } from '../api/client'
import MetricsChart from '../components/MetricsChart'
import AppUsageChart from '../components/AppUsageChart'
import ProcessList from '../components/ProcessList'
import StatCard from '../components/StatCard'
import FocusWidget from '../components/FocusWidget'
import UpdatePanel from '../components/UpdatePanel'
import DateRangePicker from '../components/DateRangePicker'
import { ActivityHeatmap } from '../components/ActivityHeatmap'
import { useSSE, ConnectionStatus } from '../hooks/useSSE'
import { Card, CardTitle, Badge, Spinner, EmptyState } from '../components/ui'
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
      <span className={cn('w-2 h-2 rounded-full', config.color, status === 'connected' && 'animate-pulse')} />
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
      <div className="flex items-center justify-center h-64">
        <Spinner size="lg" className={colors.primary.text} />
        <span className={cn('ml-3', colors.text.secondary)}>{t('common.loading')}</span>
      </div>
    )
  }

  if (!latestMetrics && !summary?.events_logged && !summary?.frames_captured && (summary?.total_active_secs ?? 0) === 0) {
    return (
      <EmptyState
        icon={<Monitor className="w-8 h-8" />}
        title={t('emptyState.dashboard.title')}
        description={t('emptyState.dashboard.description')}
      />
    )
  }

  return (
    <div className="h-full overflow-y-auto p-6 space-y-6">
      {/* UI note */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
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
              <span className={cn('w-2 h-2 rounded-full animate-pulse', colors.primary.signal)} />
              <span className={cn('text-sm font-medium', colors.primary.text)}>{t('dashboard.realtimeMonitoring')}</span>
              {idleState?.is_idle && (
                <Badge color="warning" size="sm">
                  {t('dashboard.idle')} {Math.floor((idleState.idle_secs || 0) / 60)}{t('dashboard.minutes')}
                </Badge>
              )}
            </div>
            <div className={cn('text-xs', colors.text.secondary)}>
              {new Date(latestMetrics.timestamp).toLocaleTimeString()}
            </div>
          </div>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mt-3">
            <div>
              <div className={cn(typography.stat.large, colors.accent.teal)}>
                {latestMetrics.cpu_usage.toFixed(1)}%
              </div>
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
              <div className={cn(typography.stat.large, colors.accent.slate)}>
                {metricsHistory.length}
              </div>
              <div className={cn('text-xs', colors.text.secondary)}>{t('dashboard.collectedData')}</div>
            </div>
          </div>
        </Card>
      )}

      {/* UI note */}
      <FocusWidget />

      <UpdatePanel compact />

      {/* UI note */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <StatCard
          title={t('dashboard.activeTime')}
          value={formatDuration(summary?.total_active_secs ?? 0)}
          icon={<Clock className="w-5 h-5" />}
          color="teal"
        />
        <StatCard
          title={t('dashboard.idleTime')}
          value={formatDuration(summary?.total_idle_secs ?? 0)}
          icon={<Moon className="w-5 h-5" />}
          color="slate"
        />
        <StatCard
          title={t('dashboard.captures')}
          value={summary?.frames_captured?.toLocaleString() ?? '0'}
          icon={<Camera className="w-5 h-5" />}
          color="blue"
        />
        <StatCard
          title={t('dashboard.events')}
          value={summary?.events_logged?.toLocaleString() ?? '0'}
          icon={<BarChart3 className="w-5 h-5" />}
          color="purple"
        />
      </div>

      {/* UI note */}
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('dashboard.cpuMemory24h')}</CardTitle>
        <MetricsChart data={hourlyMetrics ?? []} />
      </Card>

      {/* UI note */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
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
            <div className={cn(colors.text.secondary, 'text-center py-8')}>{t('common.noData')}</div>
          )}
        </Card>
      </div>

      {/* UI note */}
      <ActivityHeatmap days={7} className={colors.surface.elevated} />

      {/* UI note */}
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('dashboard.systemStatus')}</CardTitle>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <div className="text-center">
            <div className={cn(typography.stat.hero, colors.accent.teal)}>
              {summary?.cpu_avg?.toFixed(1) ?? '0'}%
            </div>
            <div className={cn('text-sm', colors.text.secondary)}>{t('dashboard.avgCpu')}</div>
          </div>
          <div className="text-center">
            <div className={cn(typography.stat.hero, colors.accent.blue)}>
              {summary?.memory_avg_percent?.toFixed(1) ?? '0'}%
            </div>
            <div className={cn('text-sm', colors.text.secondary)}>{t('dashboard.avgMemory')}</div>
          </div>
          <div className="text-center">
            <div className={cn(typography.stat.hero, colors.accent.purple)}>
              {summary?.top_apps?.length ?? 0}
            </div>
            <div className={cn('text-sm', colors.text.secondary)}>{t('dashboard.appsUsed')}</div>
          </div>
          <div className="text-center">
            <div className={cn(typography.stat.hero, colors.accent.green)}>
              {((summary?.total_active_secs ?? 0) / ((summary?.total_active_secs ?? 0) + (summary?.total_idle_secs ?? 1)) * 100).toFixed(0)}%
            </div>
            <div className={cn('text-sm', colors.text.secondary)}>{t('dashboard.activityRatio')}</div>
          </div>
        </div>
      </Card>
    </div>
  )
}
