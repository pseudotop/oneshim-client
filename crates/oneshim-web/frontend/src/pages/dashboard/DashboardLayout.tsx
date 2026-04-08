/**
 * Dashboard layout — shared SSE connection, summary query, and date range state.
 * Child routes receive data via Outlet context.
 */

import { useQuery } from '@tanstack/react-query'
import { Monitor } from 'lucide-react'
import { useCallback, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Outlet } from 'react-router-dom'
import { fetchSummary } from '../../api/client'
import type { DailySummary } from '../../api/contracts'
import DateRangePicker from '../../components/DateRangePicker'
import { ChartSkeleton, EmptyState, Skeleton, StatCardsSkeleton } from '../../components/ui'
import { type ConnectionStatus, type IdleUpdate, type MetricsUpdate, useSSE } from '../../hooks/useSSE'
import { RouteErrorBoundary } from '../../routes'
import { colors, typography } from '../../styles/tokens'
import { cn } from '../../utils/cn'

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

export interface DashboardContext {
  status: ConnectionStatus
  latestMetrics: MetricsUpdate | null
  idleState: IdleUpdate | null
  metricsHistory: MetricsUpdate[]
  summary: DailySummary | undefined
  summaryLoading: boolean
  dateRange: { from?: string; to?: string }
  handleRangeChange: (range: { from?: string; to?: string }) => void
}

export default function DashboardLayout() {
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

  if (summaryLoading) {
    return (
      <div className="min-h-full space-y-6 p-6">
        <Skeleton className="h-8 w-48" />
        <StatCardsSkeleton count={4} />
        <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
          <ChartSkeleton />
          <ChartSkeleton />
        </div>
        <ChartSkeleton height="h-40" />
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

  const ctx: DashboardContext = {
    status,
    latestMetrics,
    idleState,
    metricsHistory,
    summary,
    summaryLoading,
    dateRange,
    handleRangeChange: (range) => handleRangeChange(range.from, range.to),
  }

  return (
    <div className="min-h-full space-y-6 p-6">
      {/* UI note */}
      <div id="section-overview" className="flex flex-col justify-between gap-4 md:flex-row md:items-center">
        <div className="flex items-center space-x-4">
          <h1 className={cn(typography.h1, colors.text.pageTitle)}>{t('dashboard.title')}</h1>
          <ConnectionIndicator status={status} t={t} />
        </div>
        <DateRangePicker onRangeChange={handleRangeChange} />
      </div>

      <RouteErrorBoundary route="/">
        <Outlet context={ctx} />
      </RouteErrorBoundary>
    </div>
  )
}
