/**
 * PomodoroTimer — minimal focus timer with circular progress, start/cancel,
 * auto break-mode transition, and desktop notification on completion.
 */
import { Timer, X } from 'lucide-react'
import { useCallback, useEffect, useRef, useState } from 'react'
import {
  cancelPomodoro,
  completePomodoro,
  fetchCurrentPomodoro,
  type PomodoroSession,
  type PomodoroStatus,
  startPomodoro,
} from '../api/client'
import { colors, dataViz, iconSize, motion, typography } from '../styles/tokens'
import { cn } from '../utils/cn'
import { Button, Card, CardContent, CardHeader, CardTitle } from './ui'

const DEFAULT_WORK_MINS = 25
const DEFAULT_BREAK_MINS = 5

/** Format seconds as MM:SS */
function formatTime(secs: number): string {
  const m = Math.floor(secs / 60)
  const s = secs % 60
  return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`
}

function statusLabel(status: PomodoroStatus): string {
  switch (status) {
    case 'running':
      return 'Focus'
    case 'on_break':
      return 'Break'
    case 'completed':
      return 'Done'
    case 'cancelled':
      return 'Cancelled'
  }
}

function statusColor(status: PomodoroStatus): string {
  switch (status) {
    case 'running':
      return dataViz.stroke.good
    case 'on_break':
      return dataViz.stroke.warning
    case 'completed':
      return dataViz.stroke.good
    case 'cancelled':
      return dataViz.stroke.critical
  }
}

/** Circular progress ring for the timer. */
function TimerRing({
  progress,
  status,
  timeText,
  label,
}: {
  progress: number
  status: PomodoroStatus
  timeText: string
  label: string
}) {
  const r = 45
  const circumference = 2 * Math.PI * r
  const offset = circumference * (1 - Math.min(Math.max(progress, 0), 1))
  const color = statusColor(status)

  return (
    <svg width={120} height={120} viewBox="0 0 120 120" aria-hidden="true">
      {/* Background track */}
      <circle cx="60" cy="60" r={r} fill="none" stroke="currentColor" strokeWidth="6" className="text-surface-muted" />
      {/* Progress arc */}
      <circle
        cx="60"
        cy="60"
        r={r}
        fill="none"
        stroke={color}
        strokeWidth="6"
        strokeLinecap="round"
        strokeDasharray={circumference}
        strokeDashoffset={offset}
        transform="rotate(-90 60 60)"
        className={motion.all}
      />
      {/* Time text */}
      <text
        x="60"
        y="55"
        textAnchor="middle"
        dominantBaseline="middle"
        className={`fill-content text-lg ${typography.weight.bold}`}
      >
        {timeText}
      </text>
      {/* Status label */}
      <text x="60" y="75" textAnchor="middle" dominantBaseline="middle" className="fill-content-secondary text-[11px]">
        {label}
      </text>
    </svg>
  )
}

function notifyCompletion(phase: 'work' | 'break') {
  if (!('Notification' in window)) return
  if (Notification.permission === 'granted') {
    const title = phase === 'work' ? 'Focus session complete!' : 'Break is over!'
    const body = phase === 'work' ? 'Time for a break.' : 'Ready to focus again?'
    new Notification(title, { body, icon: '/favicon.ico' })
  } else if (Notification.permission !== 'denied') {
    Notification.requestPermission()
  }
}

export default function PomodoroTimer() {
  const [session, setSession] = useState<PomodoroSession | null>(null)
  const [remaining, setRemaining] = useState(0)
  const [status, setStatus] = useState<PomodoroStatus>('completed')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const prevStatusRef = useRef<PomodoroStatus>('completed')
  const tickRef = useRef<ReturnType<typeof setInterval> | null>(null)

  // Derive effective status + remaining from the session snapshot and local clock
  const computeLocal = useCallback((s: PomodoroSession) => {
    const elapsed = Math.floor((Date.now() - new Date(s.started_at).getTime()) / 1000)
    const workSecs = s.duration_minutes * 60
    const breakSecs = s.break_minutes * 60

    if (s.status === 'cancelled') {
      return { status: 'cancelled' as PomodoroStatus, remaining: 0 }
    }
    if (s.status === 'completed') {
      return { status: 'completed' as PomodoroStatus, remaining: 0 }
    }

    if (elapsed >= workSecs + breakSecs) {
      return { status: 'completed' as PomodoroStatus, remaining: 0 }
    }
    if (elapsed >= workSecs) {
      return {
        status: 'on_break' as PomodoroStatus,
        remaining: workSecs + breakSecs - elapsed,
      }
    }
    return {
      status: 'running' as PomodoroStatus,
      remaining: workSecs - elapsed,
    }
  }, [])

  // Tick every second to update the display
  useEffect(() => {
    if (!session) return
    const local = computeLocal(session)
    setStatus(local.status)
    setRemaining(local.remaining)

    tickRef.current = setInterval(() => {
      const loc = computeLocal(session)
      setStatus(loc.status)
      setRemaining(loc.remaining)

      // Detect transitions
      if (prevStatusRef.current === 'running' && loc.status === 'on_break') {
        notifyCompletion('work')
      } else if (prevStatusRef.current === 'on_break' && loc.status === 'completed') {
        notifyCompletion('break')
        // Auto-complete on server
        completePomodoro().catch((e) => console.warn('completePomodoro failed:', e))
      }
      prevStatusRef.current = loc.status
    }, 1000)

    return () => {
      if (tickRef.current) clearInterval(tickRef.current)
    }
  }, [session, computeLocal])

  // Load current session on mount
  useEffect(() => {
    fetchCurrentPomodoro()
      .then((s) => {
        if (s) {
          setSession(s)
          prevStatusRef.current = s.status
        }
      })
      .catch((e) => console.warn('fetchCurrentPomodoro failed:', e))
  }, [])

  // Request notification permission eagerly
  useEffect(() => {
    if ('Notification' in window && Notification.permission === 'default') {
      Notification.requestPermission()
    }
  }, [])

  const handleStart = async () => {
    setError(null)
    setLoading(true)
    try {
      const s = await startPomodoro({
        duration_minutes: DEFAULT_WORK_MINS,
        break_minutes: DEFAULT_BREAK_MINS,
      })
      setSession(s)
      prevStatusRef.current = 'running'
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to start')
    } finally {
      setLoading(false)
    }
  }

  const handleCancel = async () => {
    setError(null)
    setLoading(true)
    try {
      const s = await cancelPomodoro()
      setSession(s)
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to cancel')
    } finally {
      setLoading(false)
    }
  }

  const isActive = status === 'running' || status === 'on_break'

  // Compute progress fraction
  const progress = (() => {
    if (!session) return 0
    const totalSecs =
      status === 'running' ? session.duration_minutes * 60 : status === 'on_break' ? session.break_minutes * 60 : 1
    const elapsed = totalSecs - remaining
    return totalSecs > 0 ? elapsed / totalSecs : 1
  })()

  return (
    <Card variant="interactive" padding="sm">
      <CardHeader>
        <CardTitle>
          <Timer className={`mr-2 inline ${iconSize.md}`} />
          Pomodoro
        </CardTitle>
      </CardHeader>
      <CardContent>
        <div className="flex flex-col items-center gap-3">
          <TimerRing
            progress={progress}
            status={status}
            timeText={isActive ? formatTime(remaining) : '--:--'}
            label={isActive ? statusLabel(status) : 'Ready'}
          />
          <span className="sr-only" aria-live="polite">
            {isActive ? `${statusLabel(status)}: ${formatTime(remaining)}` : 'Ready'}
          </span>

          {/* Controls */}
          <div className="flex gap-2">
            {!isActive ? (
              <Button variant="primary" size="sm" onClick={handleStart} disabled={loading}>
                Start {DEFAULT_WORK_MINS}m
              </Button>
            ) : (
              <Button variant="secondary" size="sm" onClick={handleCancel} disabled={loading}>
                <X className="mr-1 h-3.5 w-3.5" />
                Cancel
              </Button>
            )}
          </div>

          {/* Error feedback */}
          {error && <p className={cn('text-xs', colors.semantic.error)}>{error}</p>}

          {/* Session info for completed */}
          {session && status === 'completed' && (
            <p className={cn(typography.small, colors.text.tertiary)}>Session completed</p>
          )}
        </div>
      </CardContent>
    </Card>
  )
}
