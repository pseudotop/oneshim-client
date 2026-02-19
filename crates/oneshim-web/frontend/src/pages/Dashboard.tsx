/**
 * 대시보드 페이지
 *
 * 시스템 요약, 실시간 메트릭, 활동 차트 표시
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
import DateRangePicker from '../components/DateRangePicker'
import { ActivityHeatmap } from '../components/ActivityHeatmap'
import { useSSE, ConnectionStatus } from '../hooks/useSSE'
import { Card, CardTitle, Badge, Spinner, EmptyState } from '../components/ui'
import { formatDuration, formatDate } from '../utils/formatters'

// 연결 상태 표시 컴포넌트
function ConnectionIndicator({ status, t }: { status: ConnectionStatus; t: (key: string) => string }) {
  const statusConfig = {
    connecting: { color: 'bg-yellow-500', textKey: 'dashboard.connecting' },
    connected: { color: 'bg-green-500', textKey: 'dashboard.connected' },
    disconnected: { color: 'bg-slate-500', textKey: 'dashboard.disconnected' },
    error: { color: 'bg-red-500', textKey: 'dashboard.error' },
  }
  const config = statusConfig[status]

  return (
    <div className="flex items-center space-x-2 text-sm">
      <span className={`w-2 h-2 rounded-full ${config.color} ${status === 'connected' ? 'animate-pulse' : ''}`} />
      <span className="text-slate-600 dark:text-slate-400">{t(config.textKey)}</span>
    </div>
  )
}

export default function Dashboard() {
  const { t } = useTranslation()
  const [dateRange, setDateRange] = useState<{ from?: string; to?: string }>({})

  // 실시간 이벤트 훅
  const { status, latestMetrics, idleState, metricsHistory } = useSSE()

  const handleRangeChange = useCallback((from: string | undefined, to: string | undefined) => {
    setDateRange({ from, to })
  }, [])

  const selectedDate = formatDate(dateRange.from)

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
        <Spinner size="lg" className="text-teal-500" />
        <span className="ml-3 text-slate-600 dark:text-slate-400">{t('common.loading')}</span>
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
    <div className="space-y-6">
      {/* 헤더 */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4">
        <div className="flex items-center space-x-4">
          <h1 className="text-2xl font-bold text-slate-900 dark:text-white">{t('dashboard.title')}</h1>
          <ConnectionIndicator status={status} t={t} />
        </div>
        <DateRangePicker onRangeChange={handleRangeChange} />
      </div>

      {/* 실시간 메트릭 (SSE 연결 시) */}
      {latestMetrics && (
        <Card variant="highlight" padding="md">
          <div className="flex items-center justify-between">
            <div className="flex items-center space-x-2">
              <span className="w-2 h-2 bg-teal-500 dark:bg-teal-400 rounded-full animate-pulse" />
              <span className="text-sm font-medium text-teal-700 dark:text-teal-300">{t('dashboard.realtimeMonitoring')}</span>
              {idleState?.is_idle && (
                <Badge color="warning" size="sm">
                  {t('dashboard.idle')} {Math.floor((idleState.idle_secs || 0) / 60)}{t('dashboard.minutes')}
                </Badge>
              )}
            </div>
            <div className="text-xs text-slate-600 dark:text-slate-400">
              {new Date(latestMetrics.timestamp).toLocaleTimeString()}
            </div>
          </div>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mt-3">
            <div>
              <div className="text-2xl font-bold text-teal-600 dark:text-teal-400">
                {latestMetrics.cpu_usage.toFixed(1)}%
              </div>
              <div className="text-xs text-slate-600 dark:text-slate-400">{t('dashboard.cpu')}</div>
            </div>
            <div>
              <div className="text-2xl font-bold text-blue-600 dark:text-blue-400">
                {latestMetrics.memory_percent.toFixed(1)}%
              </div>
              <div className="text-xs text-slate-600 dark:text-slate-400">{t('dashboard.memory')}</div>
            </div>
            <div>
              <div className="text-2xl font-bold text-purple-600 dark:text-purple-400">
                {(latestMetrics.memory_used / 1024 / 1024 / 1024).toFixed(1)} GB
              </div>
              <div className="text-xs text-slate-600 dark:text-slate-400">{t('dashboard.usedMemory')}</div>
            </div>
            <div>
              <div className="text-2xl font-bold text-slate-700 dark:text-slate-300">
                {metricsHistory.length}
              </div>
              <div className="text-xs text-slate-600 dark:text-slate-400">{t('dashboard.collectedData')}</div>
            </div>
          </div>
        </Card>
      )}

      {/* 집중도 위젯 */}
      <FocusWidget />

      {/* 통계 카드 */}
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

      {/* 메트릭 차트 */}
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('dashboard.cpuMemory24h')}</CardTitle>
        <MetricsChart data={hourlyMetrics ?? []} />
      </Card>

      {/* 하단 그리드 */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* 앱 사용 시간 */}
        <Card variant="default" padding="lg">
          <CardTitle className="mb-4">{t('dashboard.appUsageTime')}</CardTitle>
          <AppUsageChart apps={summary?.top_apps ?? []} />
        </Card>

        {/* 프로세스 Top 5 */}
        <Card variant="default" padding="lg">
          <CardTitle className="mb-4">{t('dashboard.recentProcesses')}</CardTitle>
          {processes && processes.length > 0 ? (
            <ProcessList snapshot={processes[0]} />
          ) : (
            <div className="text-slate-600 dark:text-slate-400 text-center py-8">{t('common.noData')}</div>
          )}
        </Card>
      </div>

      {/* 활동 히트맵 */}
      <ActivityHeatmap days={7} className="bg-slate-100 dark:bg-slate-800" />

      {/* 시스템 상태 */}
      <Card variant="default" padding="lg">
        <CardTitle className="mb-4">{t('dashboard.systemStatus')}</CardTitle>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <div className="text-center">
            <div className="text-3xl font-bold text-teal-600 dark:text-teal-400">
              {summary?.cpu_avg?.toFixed(1) ?? '0'}%
            </div>
            <div className="text-sm text-slate-600 dark:text-slate-400">{t('dashboard.avgCpu')}</div>
          </div>
          <div className="text-center">
            <div className="text-3xl font-bold text-blue-600 dark:text-blue-400">
              {summary?.memory_avg_percent?.toFixed(1) ?? '0'}%
            </div>
            <div className="text-sm text-slate-600 dark:text-slate-400">{t('dashboard.avgMemory')}</div>
          </div>
          <div className="text-center">
            <div className="text-3xl font-bold text-purple-600 dark:text-purple-400">
              {summary?.top_apps?.length ?? 0}
            </div>
            <div className="text-sm text-slate-600 dark:text-slate-400">{t('dashboard.appsUsed')}</div>
          </div>
          <div className="text-center">
            <div className="text-3xl font-bold text-green-600 dark:text-green-400">
              {((summary?.total_active_secs ?? 0) / ((summary?.total_active_secs ?? 0) + (summary?.total_idle_secs ?? 1)) * 100).toFixed(0)}%
            </div>
            <div className="text-sm text-slate-600 dark:text-slate-400">{t('dashboard.activityRatio')}</div>
          </div>
        </div>
      </Card>
    </div>
  )
}
