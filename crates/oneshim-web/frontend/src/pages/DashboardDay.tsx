/**
 * DashboardDay — daily timetable view with insight, timeline, and statistics.
 */
import { useQuery } from '@tanstack/react-query'
import { ChevronLeft, ChevronRight, Calendar } from 'lucide-react'
import { useState } from 'react'
import InsightCard from '../components/InsightCard'
import StatisticsPanel from '../components/StatisticsPanel'
import TimelineView from '../components/TimelineView'
import { Button, Card, Skeleton } from '../components/ui'
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

interface DailyDigestResponse {
  date: string
  insight: {
    narrative: string
    highlights: Array<{ highlight_type: string; text: string; segment_id?: string }>
  } | null
  timeline: Array<{
    segment_id: string
    start_time: string
    end_time: string
    duration_mins: number
    regime_label: string
    regime_color: string
    dominant_app: string
    content_summary: Array<{ content: string; work_type: string; mins: number }>
    annotation?: { highlight_type: string; text: string }
  }>
  statistics: {
    deep_work_hours: number
    communication_hours: number
    meeting_hours: number
    context_switches: number
    longest_focus_mins: number
    longest_focus_content: string
    regime_distribution: Record<string, number>
    comparison?: {
      deep_work_delta: number
      communication_delta: number
      context_switch_delta: number
    }
  }
}

export default function DashboardDay() {
  const [date, setDate] = useState(todayStr)

  const { data, isLoading, error } = useQuery<DailyDigestResponse>({
    queryKey: ['dashboard-day', date],
    queryFn: async () => {
      const r = await fetch(`/api/dashboard/day?date=${date}`);
      if (!r.ok) throw new Error(`HTTP ${r.status}: ${r.statusText}`);
      return r.json();
    },
  })

  const isToday = date === todayStr()

  return (
    <div className="min-h-full space-y-6 p-6">
      {/* Date navigation */}
      <div className="flex flex-col justify-between gap-4 md:flex-row md:items-center">
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
              className="bg-transparent text-sm text-content outline-none"
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
      <p className={cn('text-sm', colors.text.secondary)}>{formatDisplayDate(date)}</p>

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
          <TimelineView timeline={data.timeline} />
          <StatisticsPanel statistics={data.statistics} />
        </>
      )}
    </div>
  )
}
