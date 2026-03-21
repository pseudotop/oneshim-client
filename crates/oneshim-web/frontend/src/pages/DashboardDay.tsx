/**
 * DashboardDay — daily timetable view with insight, timeline, statistics,
 * and a Pomodoro focus timer widget.
 */
import { useQuery } from '@tanstack/react-query'
import { Calendar, ChevronLeft, ChevronRight } from 'lucide-react'
import { useMemo, useState } from 'react'
import { fetchDailyDigest } from '../api/client'
import type { DailyDigestResponse } from '../api/contracts'
import InsightCard from '../components/InsightCard'
import PomodoroTimer from '../components/PomodoroTimer'
import StatisticsPanel from '../components/StatisticsPanel'
import GuiInteractionTrack from '../components/GuiInteractionTrack'
import TimelineView from '../components/TimelineView'
import { Button, Card, Skeleton } from '../components/ui'
import { useCreateOverride, useOverrides } from '../hooks/useRecalibration'
import { colors, typography } from '../styles/tokens'
import { cn } from '../utils/cn'

function todayStr(): string {
  return new Date().toISOString().split('T')[0]
}

function shiftDate(dateStr: string, days: number): string {
  const d = new Date(dateStr)
  d.setDate(d.getDate() + days)
  return d.toISOString().split('T')[0]
}

function formatDisplayDate(dateStr: string): string {
  try {
    const d = new Date(dateStr)
    return d.toLocaleDateString(undefined, { weekday: 'long', year: 'numeric', month: 'long', day: 'numeric' })
  } catch {
    return dateStr
  }
}

// Default regime options — in practice these would come from server config
const DEFAULT_REGIME_OPTIONS = [
  { id: 'deep-work', label: 'Deep Work' },
  { id: 'communication', label: 'Communication' },
  { id: 'meeting', label: 'Meeting' },
  { id: 'break', label: 'Break' },
  { id: 'admin', label: 'Admin' },
]

export default function DashboardDay() {
  const [date, setDate] = useState(todayStr)

  const { data, isLoading, error } = useQuery<DailyDigestResponse>({
    queryKey: ['dashboard-day', date],
    queryFn: () => fetchDailyDigest(date),
  })

  // Fetch overrides for the current date
  const dateFrom = `${date}T00:00:00Z`
  const dateTo = `${date}T23:59:59Z`
  const { data: overrides } = useOverrides(dateFrom, dateTo)
  const createOverrideMutation = useCreateOverride()

  // Derive regime options from timeline data or use defaults
  const regimeOptions = useMemo(() => {
    if (!data?.timeline || data.timeline.length === 0) return DEFAULT_REGIME_OPTIONS
    const seen = new Map<string, string>()
    for (const seg of data.timeline) {
      if (seg.regime_id && !seen.has(seg.regime_id)) {
        seen.set(seg.regime_id, seg.regime_label)
      }
    }
    if (seen.size > 0) {
      return Array.from(seen.entries()).map(([id, label]) => ({ id, label }))
    }
    return DEFAULT_REGIME_OPTIONS
  }, [data?.timeline])

  const isToday = date === todayStr()

  return (
    <div className="min-h-full p-6">
      {/* Date navigation */}
      <div className="mb-6 flex flex-col justify-between gap-4 md:flex-row md:items-center">
        <h1 className={cn(typography.h1, colors.text.pageTitle)}>Daily Timetable</h1>

        <div className="flex items-center gap-2">
          <Button
            variant="secondary"
            size="sm"
            onClick={() => setDate((d) => shiftDate(d, -1))}
            aria-label="Previous day"
          >
            <ChevronLeft className="h-4 w-4" />
          </Button>

          <div className="flex items-center gap-2 rounded-lg bg-surface-elevated px-3 py-1.5">
            <Calendar className={cn('h-4 w-4', colors.text.secondary)} />
            <input
              type="date"
              value={date}
              onChange={(e) => setDate(e.target.value || todayStr())}
              className="bg-transparent text-content text-sm outline-none"
              max={todayStr()}
            />
          </div>

          <Button
            variant="secondary"
            size="sm"
            onClick={() => setDate((d) => shiftDate(d, 1))}
            disabled={isToday}
            aria-label="Next day"
          >
            <ChevronRight className="h-4 w-4" />
          </Button>

          {!isToday && (
            <Button variant="ghost" size="sm" onClick={() => setDate(todayStr())}>
              Today
            </Button>
          )}
        </div>
      </div>

      {/* Display date */}
      <p className={cn('mb-6 text-sm', colors.text.secondary)}>{formatDisplayDate(date)}</p>

      {/* Two-column layout: main content + sidebar timer */}
      <div className="flex flex-col gap-6 lg:flex-row">
        {/* Main content */}
        <div className="min-w-0 flex-1 space-y-6">
          {/* Loading state */}
          {isLoading && (
            <div className="space-y-4">
              <Skeleton className="h-24 w-full" />
              <Skeleton className="h-64 w-full" />
              <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
                <Skeleton className="h-20 w-full" />
                <Skeleton className="h-20 w-full" />
                <Skeleton className="h-20 w-full" />
                <Skeleton className="h-20 w-full" />
              </div>
            </div>
          )}

          {/* Error state */}
          {error && (
            <Card variant="danger" padding="md">
              <p className="text-red-400">Failed to load daily digest. Please try again later.</p>
            </Card>
          )}

          {/* Content */}
          {data && !isLoading && (
            <>
              <InsightCard insight={data.insight} />
              <TimelineView
                timeline={data.timeline}
                overrides={overrides}
                regimeOptions={regimeOptions}
                onCreateOverride={(req) => createOverrideMutation.mutate(req)}
                isMutating={createOverrideMutation.isPending}
              />
              <GuiInteractionTrack
                start={`${date}T00:00:00Z`}
                end={`${date}T23:59:59Z`}
              />
              <StatisticsPanel statistics={data.statistics} />
            </>
          )}
        </div>

        {/* Sidebar: Pomodoro timer */}
        <aside className="w-full shrink-0 lg:w-56">
          <PomodoroTimer />
        </aside>
      </div>
    </div>
  )
}
