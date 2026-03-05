import { Moon, Pause, Play, SkipBack, SkipForward } from 'lucide-react'
import { useCallback, useMemo, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import type { AppSegment, TimelineItem } from '../api/client'
import { formatTime } from '../utils/formatters'
import { Select } from './ui'

interface TimelineScrubberProps {
  startTime: Date
  endTime: Date
  currentTime: Date
  isPlaying: boolean
  playbackSpeed: number
  segments: AppSegment[]
  items: TimelineItem[]
  onTimeChange: (time: Date) => void
  onPlayPause: () => void
  onSpeedChange: (speed: number) => void
  onSkipToStart: () => void
  onSkipToEnd: () => void
}

const PLAYBACK_SPEEDS = [0.5, 1, 2, 5, 10]

export default function TimelineScrubber({
  startTime,
  endTime,
  currentTime,
  isPlaying,
  playbackSpeed,
  segments,
  items,
  onTimeChange,
  onPlayPause,
  onSpeedChange,
  onSkipToStart,
  onSkipToEnd,
}: TimelineScrubberProps) {
  const { t } = useTranslation()
  const trackRef = useRef<HTMLDivElement>(null)

  const totalDuration = endTime.getTime() - startTime.getTime()

  const currentPosition = totalDuration > 0 ? (currentTime.getTime() - startTime.getTime()) / totalDuration : 0

  const idlePeriods = useMemo(() => {
    return items
      .filter((item): item is Extract<TimelineItem, { type: 'IdlePeriod' }> => item.type === 'IdlePeriod')
      .map((item) => ({
        start: new Date(item.start).getTime(),
        end: new Date(item.end).getTime(),
      }))
  }, [items])

  const handleTrackClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (!trackRef.current || totalDuration <= 0) return

      const rect = trackRef.current.getBoundingClientRect()
      const x = e.clientX - rect.left
      const ratio = Math.max(0, Math.min(1, x / rect.width))
      const newTime = new Date(startTime.getTime() + ratio * totalDuration)
      onTimeChange(newTime)
    },
    [startTime, totalDuration, onTimeChange],
  )

  const handleMouseDown = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      if (!trackRef.current || totalDuration <= 0) return

      const handleMouseMove = (moveEvent: MouseEvent) => {
        if (!trackRef.current) return
        const rect = trackRef.current.getBoundingClientRect()
        const x = moveEvent.clientX - rect.left
        const ratio = Math.max(0, Math.min(1, x / rect.width))
        const newTime = new Date(startTime.getTime() + ratio * totalDuration)
        onTimeChange(newTime)
      }

      const handleMouseUp = () => {
        document.removeEventListener('mousemove', handleMouseMove)
        document.removeEventListener('mouseup', handleMouseUp)
      }

      document.addEventListener('mousemove', handleMouseMove)
      document.addEventListener('mouseup', handleMouseUp)

      handleTrackClick(e)
    },
    [startTime, totalDuration, onTimeChange, handleTrackClick],
  )

  return (
    <div className="rounded-lg border border-muted bg-surface-overlay p-4 shadow">
      {/* UI note */}
      <div className="mb-3 flex items-center justify-between">
        <div className="flex items-center space-x-2">
          {/* UI note */}
          <button
            type="button"
            onClick={onSkipToStart}
            className="rounded-lg p-2 text-content-secondary transition-colors hover:bg-hover"
            title={t('replay.skipToStart', '처음으로')}
          >
            <SkipBack className="h-5 w-5" />
          </button>

          {/* UI note */}
          <button
            type="button"
            onClick={onPlayPause}
            className="rounded-lg bg-teal-500 p-2 text-white transition-colors hover:bg-teal-600"
            title={isPlaying ? t('replay.pause', '일시정지') : t('replay.play', '재생')}
          >
            {isPlaying ? <Pause className="h-5 w-5" /> : <Play className="h-5 w-5" />}
          </button>

          {/* UI note */}
          <button
            type="button"
            onClick={onSkipToEnd}
            className="rounded-lg p-2 text-content-secondary transition-colors hover:bg-hover"
            title={t('replay.skipToEnd', '끝으로')}
          >
            <SkipForward className="h-5 w-5" />
          </button>

          {/* UI note */}
          <span className="ml-3 font-mono text-content-strong text-sm">{formatTime(currentTime.toISOString())}</span>
        </div>

        {/* UI note */}
        <div className="flex items-center space-x-2">
          <span className="text-content-secondary text-xs">{t('replay.speed', '재생 속도')}:</span>
          <Select
            value={playbackSpeed}
            selectSize="sm"
            onChange={(e) => onSpeedChange(Number(e.target.value))}
            className="w-auto"
          >
            {PLAYBACK_SPEEDS.map((speed) => (
              <option key={speed} value={speed}>
                {speed}x
              </option>
            ))}
          </Select>
        </div>
      </div>

      {/* UI note */}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: custom scrubber track — keyboard users use playback buttons */}
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: drag interaction; keyboard users use playback buttons */}
      <div
        ref={trackRef}
        className="relative h-10 cursor-pointer overflow-hidden rounded-lg bg-hover"
        onClick={handleTrackClick}
        onMouseDown={handleMouseDown}
      >
        {/* UI note */}
        {segments.map((segment) => {
          const segmentStart = new Date(segment.start).getTime()
          const segmentEnd = new Date(segment.end).getTime()
          const left = totalDuration > 0 ? ((segmentStart - startTime.getTime()) / totalDuration) * 100 : 0
          const width = totalDuration > 0 ? ((segmentEnd - segmentStart) / totalDuration) * 100 : 0

          return (
            <div
              key={`segment-${segment.app_name}-${segment.start}`}
              className="absolute top-0 h-full transition-opacity hover:opacity-80"
              style={{
                left: `${left}%`,
                width: `${Math.max(width, 0.5)}%`,
                backgroundColor: segment.color,
              }}
              title={segment.app_name}
            />
          )
        })}

        {/* UI note */}
        {idlePeriods.map((idle) => {
          const left = totalDuration > 0 ? ((idle.start - startTime.getTime()) / totalDuration) * 100 : 0
          const width = totalDuration > 0 ? ((idle.end - idle.start) / totalDuration) * 100 : 0

          return (
            <div
              key={`idle-${idle.start}-${idle.end}`}
              className="absolute top-0 flex h-full items-center justify-center"
              style={{
                left: `${left}%`,
                width: `${Math.max(width, 0.5)}%`,
                background:
                  'repeating-linear-gradient(45deg, rgba(100,116,139,0.3), rgba(100,116,139,0.3) 2px, transparent 2px, transparent 4px)',
              }}
              title={t('replay.idle', 'idle')}
            >
              {width > 3 && <Moon className="h-3 w-3 text-content-secondary opacity-70" />}
            </div>
          )
        })}

        {/* UI note */}
        <div
          className="absolute top-0 z-10 h-full w-0.5 bg-red-500 shadow-lg"
          style={{ left: `${currentPosition * 100}%` }}
        >
          {/* UI note */}
          <div className="absolute -top-1 -left-1.5 h-4 w-4 rounded-full bg-red-500 shadow-lg" />
        </div>

        {/* UI note */}
        <div className="absolute bottom-0 left-1 text-content-secondary text-xs opacity-70">
          {formatTime(startTime.toISOString())}
        </div>
        <div className="absolute right-1 bottom-0 text-content-secondary text-xs opacity-70">
          {formatTime(endTime.toISOString())}
        </div>
      </div>

      {/* UI note */}
      {segments.length > 0 && (
        <div className="mt-3 flex flex-wrap gap-2">
          {/* UI note */}
          {Array.from(new Set(segments.map((s) => s.app_name)))
            .slice(0, 8)
            .map((appName) => {
              const segment = segments.find((s) => s.app_name === appName)
              return (
                <div key={appName} className="flex items-center space-x-1">
                  <div className="h-3 w-3 rounded-sm" style={{ backgroundColor: segment?.color || '#6B7280' }} />
                  <span className="max-w-[80px] truncate text-content-secondary text-xs">{appName}</span>
                </div>
              )
            })}
          {/* UI note */}
          {idlePeriods.length > 0 && (
            <div className="flex items-center space-x-1">
              <div
                className="h-3 w-3 rounded-sm"
                style={{
                  background:
                    'repeating-linear-gradient(45deg, rgba(100,116,139,0.5), rgba(100,116,139,0.5) 1px, transparent 1px, transparent 2px)',
                }}
              />
              <span className="text-content-secondary text-xs">{t('replay.idle', 'idle')}</span>
            </div>
          )}
        </div>
      )}
    </div>
  )
}
