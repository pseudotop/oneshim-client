/**
 * TimelineView — vertical timetable with colored regime blocks.
 * Pure CSS layout (no Recharts), proportional block heights based on duration.
 */
import { useState } from 'react'
import { Card } from './ui'
import { cn } from '../utils/cn'
import { colors, typography } from '../styles/tokens'

interface TimelineEntry {
  segment_id: string
  start_time: string
  end_time: string
  duration_mins: number
  regime_label: string
  regime_color: string
  dominant_app: string
  content_summary: Array<{ content: string; work_type: string; mins: number }>
  annotation?: { highlight_type: string; text: string }
}

interface TimelineViewProps {
  timeline: TimelineEntry[]
  onSegmentClick?: (segmentId: string) => void
}

const annotationIcons: Record<string, string> = {
  ACHIEVEMENT: '\u{1F3C6}',
  WARNING: '\u{26A0}\u{FE0F}',
  SUGGESTION: '\u{1F4A1}',
}

function formatTime(iso: string): string {
  try {
    const date = new Date(iso)
    return date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false })
  } catch {
    return iso.slice(11, 16)
  }
}

export default function TimelineView({ timeline, onSegmentClick }: TimelineViewProps) {
  const [expandedId, setExpandedId] = useState<string | null>(null)

  if (timeline.length === 0) {
    return (
      <Card padding="md">
        <p className={cn(typography.body, colors.text.secondary, 'text-center py-8')}>
          No activity recorded for this day
        </p>
      </Card>
    )
  }

  const handleClick = (segmentId: string) => {
    setExpandedId((prev) => (prev === segmentId ? null : segmentId))
    onSegmentClick?.(segmentId)
  }

  // Scale: 1 min = 2px, minimum 32px per block
  const minBlockHeight = 32

  return (
    <Card padding="md">
      <div className="space-y-0">
        {timeline.map((entry) => {
          const blockHeight = Math.max(entry.duration_mins * 2, minBlockHeight)
          const isExpanded = expandedId === entry.segment_id

          return (
            <div key={entry.segment_id} className="flex gap-3">
              {/* Time label column */}
              <div className="flex w-14 flex-shrink-0 flex-col items-end pt-2">
                <span className={cn('text-xs font-medium', colors.text.secondary)}>
                  {formatTime(entry.start_time)}
                </span>
              </div>

              {/* Colored block column */}
              <div className="relative flex-1">
                <button
                  type="button"
                  className={cn(
                    'w-full rounded-lg border-l-4 px-3 py-2 text-left transition-colors hover:opacity-90',
                    isExpanded ? 'ring-2 ring-brand-signal/50' : '',
                  )}
                  style={{
                    minHeight: `${blockHeight}px`,
                    borderLeftColor: entry.regime_color,
                    backgroundColor: `${entry.regime_color}15`,
                  }}
                  onClick={() => handleClick(entry.segment_id)}
                  aria-label={`${entry.regime_label} ${entry.duration_mins} minutes - click to expand`}
                  aria-expanded={isExpanded}
                >
                  <div className="flex items-center justify-between gap-2">
                    <span className={cn('text-xs font-semibold', colors.text.primary)}>
                      {entry.regime_label}
                    </span>
                    <span className={cn('text-xs', colors.text.tertiary)}>
                      {entry.duration_mins}m
                    </span>
                  </div>
                  <span className={cn('text-xs', colors.text.secondary)}>
                    {entry.dominant_app}
                  </span>

                  {/* Content summary (top items) */}
                  {entry.content_summary.length > 0 && (
                    <div className="mt-1 space-y-0.5">
                      {entry.content_summary.slice(0, isExpanded ? undefined : 2).map((item, idx) => (
                        <div key={`${entry.segment_id}-content-${idx}`} className="flex items-center gap-1.5">
                          <span className={cn('text-xs', colors.text.tertiary)}>
                            {item.work_type} ({item.mins}m)
                          </span>
                          <span className={cn('truncate text-xs', colors.text.secondary)}>
                            {item.content}
                          </span>
                        </div>
                      ))}
                    </div>
                  )}

                  {/* Annotation bubble */}
                  {entry.annotation && (
                    <div className="mt-2 inline-flex items-center gap-1 rounded-md bg-surface-muted px-2 py-1">
                      <span aria-hidden="true">
                        {annotationIcons[entry.annotation.highlight_type] ?? '\u{1F4A1}'}
                      </span>
                      <span className={cn('text-xs', colors.text.secondary)}>
                        {entry.annotation.text}
                      </span>
                    </div>
                  )}
                </button>
              </div>
            </div>
          )
        })}
      </div>
    </Card>
  )
}

TimelineView.displayName = 'TimelineView'
