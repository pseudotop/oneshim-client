
import { useRef, useCallback, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Play, Pause, SkipBack, SkipForward, Moon } from 'lucide-react'
import { Select } from './ui'
import type { AppSegment, TimelineItem } from '../api/client'
import { formatTime } from '../utils/formatters'

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

  const currentPosition = totalDuration > 0
    ? (currentTime.getTime() - startTime.getTime()) / totalDuration
    : 0

  const idlePeriods = useMemo(() => {
    return items
      .filter((item): item is Extract<TimelineItem, { type: 'IdlePeriod' }> => item.type === 'IdlePeriod')
      .map(item => ({
        start: new Date(item.start).getTime(),
        end: new Date(item.end).getTime(),
      }))
  }, [items])

  const handleTrackClick = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    if (!trackRef.current || totalDuration <= 0) return

    const rect = trackRef.current.getBoundingClientRect()
    const x = e.clientX - rect.left
    const ratio = Math.max(0, Math.min(1, x / rect.width))
    const newTime = new Date(startTime.getTime() + ratio * totalDuration)
    onTimeChange(newTime)
  }, [startTime, totalDuration, onTimeChange])

  const handleMouseDown = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
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
  }, [startTime, totalDuration, onTimeChange, handleTrackClick])

  return (
    <div className="bg-white dark:bg-slate-800 rounded-lg p-4 shadow border border-slate-200 dark:border-slate-700">
      {/* UI note */}
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center space-x-2">
          {/* UI note */}
          <button
            onClick={onSkipToStart}
            className="p-2 rounded-lg text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-700 transition-colors"
            title={t('replay.skipToStart', '처음으로')}
          >
            <SkipBack className="w-5 h-5" />
          </button>

          {/* UI note */}
          <button
            onClick={onPlayPause}
            className="p-2 rounded-lg bg-teal-500 text-white hover:bg-teal-600 transition-colors"
            title={isPlaying ? t('replay.pause', '일시정지') : t('replay.play', '재생')}
          >
            {isPlaying ? <Pause className="w-5 h-5" /> : <Play className="w-5 h-5" />}
          </button>

          {/* UI note */}
          <button
            onClick={onSkipToEnd}
            className="p-2 rounded-lg text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-700 transition-colors"
            title={t('replay.skipToEnd', '끝으로')}
          >
            <SkipForward className="w-5 h-5" />
          </button>

          {/* UI note */}
          <span className="ml-3 text-sm font-mono text-slate-700 dark:text-slate-300">
            {formatTime(currentTime.toISOString())}
          </span>
        </div>

        {/* UI note */}
        <div className="flex items-center space-x-2">
          <span className="text-xs text-slate-500 dark:text-slate-400">
            {t('replay.speed', '재생 속도')}:
          </span>
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
      <div
        ref={trackRef}
        className="relative h-10 bg-slate-200 dark:bg-slate-700 rounded-lg cursor-pointer overflow-hidden"
        onClick={handleTrackClick}
        onMouseDown={handleMouseDown}
      >
        {/* UI note */}
        {segments.map((segment, index) => {
          const segmentStart = new Date(segment.start).getTime()
          const segmentEnd = new Date(segment.end).getTime()
          const left = totalDuration > 0
            ? ((segmentStart - startTime.getTime()) / totalDuration) * 100
            : 0
          const width = totalDuration > 0
            ? ((segmentEnd - segmentStart) / totalDuration) * 100
            : 0

          return (
            <div
              key={`segment-${index}`}
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
        {idlePeriods.map((idle, index) => {
          const left = totalDuration > 0
            ? ((idle.start - startTime.getTime()) / totalDuration) * 100
            : 0
          const width = totalDuration > 0
            ? ((idle.end - idle.start) / totalDuration) * 100
            : 0

          return (
            <div
              key={`idle-${index}`}
              className="absolute top-0 h-full flex items-center justify-center"
              style={{
                left: `${left}%`,
                width: `${Math.max(width, 0.5)}%`,
                background: 'repeating-linear-gradient(45deg, rgba(100,116,139,0.3), rgba(100,116,139,0.3) 2px, transparent 2px, transparent 4px)',
              }}
              title={t('replay.idle', 'idle')}
            >
              {width > 3 && (
                <Moon className="w-3 h-3 text-slate-500 dark:text-slate-400 opacity-70" />
              )}
            </div>
          )
        })}

        {/* UI note */}
        <div
          className="absolute top-0 w-0.5 h-full bg-red-500 shadow-lg z-10"
          style={{ left: `${currentPosition * 100}%` }}
        >
          {/* UI note */}
          <div className="absolute -top-1 -left-1.5 w-4 h-4 bg-red-500 rounded-full shadow-lg" />
        </div>

        {/* UI note */}
        <div className="absolute bottom-0 left-1 text-xs text-slate-600 dark:text-slate-400 opacity-70">
          {formatTime(startTime.toISOString())}
        </div>
        <div className="absolute bottom-0 right-1 text-xs text-slate-600 dark:text-slate-400 opacity-70">
          {formatTime(endTime.toISOString())}
        </div>
      </div>

      {/* UI note */}
      {segments.length > 0 && (
        <div className="flex flex-wrap gap-2 mt-3">
          {/* UI note */}
          {Array.from(new Set(segments.map(s => s.app_name))).slice(0, 8).map((appName) => {
            const segment = segments.find(s => s.app_name === appName)
            return (
              <div key={appName} className="flex items-center space-x-1">
                <div
                  className="w-3 h-3 rounded-sm"
                  style={{ backgroundColor: segment?.color || '#6B7280' }}
                />
                <span className="text-xs text-slate-600 dark:text-slate-400 truncate max-w-[80px]">
                  {appName}
                </span>
              </div>
            )
          })}
          {/* UI note */}
          {idlePeriods.length > 0 && (
            <div className="flex items-center space-x-1">
              <div
                className="w-3 h-3 rounded-sm"
                style={{
                  background: 'repeating-linear-gradient(45deg, rgba(100,116,139,0.5), rgba(100,116,139,0.5) 1px, transparent 1px, transparent 2px)',
                }}
              />
              <span className="text-xs text-slate-600 dark:text-slate-400">
                {t('replay.idle', 'idle')}
              </span>
            </div>
          )}
        </div>
      )}
    </div>
  )
}
