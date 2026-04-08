/**
 * Focus layout — fetches focus metrics, manages date range state.
 * Child routes receive data via Outlet context.
 */

import { useQuery } from '@tanstack/react-query'
import { Brain, Focus as FocusIcon } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Outlet } from 'react-router-dom'
import { type FocusMetricsResponse, fetchFocusMetrics } from '../../api/client'
import DateRangePicker from '../../components/DateRangePicker'
import { EmptyState } from '../../components/ui'
import { Card, CardContent } from '../../components/ui/Card'
import { Spinner } from '../../components/ui/Spinner'
import { RouteErrorBoundary } from '../../routes'
import { colors, iconSize, typography } from '../../styles/tokens'
import { cn } from '../../utils/cn'

function createInitialWeekRange() {
  const to = new Date()
  const from = new Date()
  from.setDate(from.getDate() - 7)

  return {
    from: new Date(`${from.toISOString().split('T')[0]}T00:00:00Z`),
    to: new Date(`${to.toISOString().split('T')[0]}T23:59:59Z`),
  }
}

export interface FocusContext {
  metrics: FocusMetricsResponse
  dateRange: { from: Date; to: Date }
  setDateRange: React.Dispatch<React.SetStateAction<{ from: Date; to: Date }>>
}

export default function FocusLayout() {
  const { t } = useTranslation()
  const [dateRange, setDateRange] = useState<{ from: Date; to: Date }>(() => createInitialWeekRange())

  const {
    data: metrics,
    isLoading: metricsLoading,
    error: metricsError,
  } = useQuery({
    queryKey: ['focusMetrics'],
    queryFn: fetchFocusMetrics,
    staleTime: 10_000, // focus data — 10s stale time
  })

  if (metricsLoading) {
    return (
      <div className="flex h-64 items-center justify-center">
        <Spinner size="lg" />
      </div>
    )
  }

  const error = metricsError ? (metricsError instanceof Error ? metricsError.message : String(metricsError)) : null

  if (error || !metrics) {
    return (
      <Card variant="danger">
        <CardContent>
          <p className="text-semantic-error">{error || t('common.error')}</p>
        </CardContent>
      </Card>
    )
  }

  if (metrics.today.focus_score === 0) {
    return (
      <EmptyState
        icon={<Brain className="h-8 w-8" />}
        title={t('emptyState.focus.title')}
        description={t('emptyState.focus.description')}
      />
    )
  }

  const ctx: FocusContext = {
    metrics,
    dateRange,
    setDateRange,
  }

  return (
    <div className="min-h-full space-y-6 p-6">
      <div className="flex items-center justify-between">
        <h1 className={cn(typography.h1, colors.text.pageTitle, 'flex items-center gap-2')}>
          <FocusIcon className={iconSize.lg} />
          {t('focus.pageTitle')}
        </h1>
        <DateRangePicker
          initialPreset="7days"
          onRangeChange={(from, to) => {
            if (from && to) {
              setDateRange({ from: new Date(from), to: new Date(to) })
            }
          }}
        />
      </div>

      <RouteErrorBoundary route="/focus">
        <Outlet context={ctx} />
      </RouteErrorBoundary>
    </div>
  )
}
