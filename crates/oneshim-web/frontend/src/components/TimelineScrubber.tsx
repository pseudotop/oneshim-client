// 타임라인 스크러버 컴포넌트 - 재생 컨트롤, 앱 세그먼트 바, 유휴 구간 표시

import { useRef, useCallback, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { Play, Pause, SkipBack, SkipForward, Moon } from 'lucide-react'
import type { AppSegment, TimelineItem } from '../api/client'
import { formatTime } from '../utils/formatters'

interface TimelineScrubberProps {
  /** 시작 시각 */
  startTime: Date
  /** 종료 시각 */
  endTime: Date
  /** 현재 재생 시각 */
  currentTime: Date
  /** 재생 중 여부 */
  isPlaying: boolean
  /** 재생 속도 (1x, 2x, 5x, 10x) */
  playbackSpeed: number
  /** 앱 세그먼트 목록 */
  segments: AppSegment[]
  /** 타임라인 아이템 (유휴 기간 표시용) */
  items: TimelineItem[]
  /** 시간 변경 콜백 */
  onTimeChange: (time: Date) => void
  /** 재생/일시정지 토글 */
  onPlayPause: () => void
  /** 재생 속도 변경 */
  onSpeedChange: (speed: number) => void
  /** 처음으로 */
  onSkipToStart: () => void
  /** 끝으로 */
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

  // 전체 시간 범위 (밀리초)
  const totalDuration = endTime.getTime() - startTime.getTime()

  // 현재 위치 비율 (0-1)
  const currentPosition = totalDuration > 0
    ? (currentTime.getTime() - startTime.getTime()) / totalDuration
    : 0

  // 유휴 기간 추출
  const idlePeriods = useMemo(() => {
    return items
      .filter((item): item is Extract<TimelineItem, { type: 'IdlePeriod' }> => item.type === 'IdlePeriod')
      .map(item => ({
        start: new Date(item.start).getTime(),
        end: new Date(item.end).getTime(),
      }))
  }, [items])

  // 클릭으로 시간 이동
  const handleTrackClick = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    if (!trackRef.current || totalDuration <= 0) return

    const rect = trackRef.current.getBoundingClientRect()
    const x = e.clientX - rect.left
    const ratio = Math.max(0, Math.min(1, x / rect.width))
    const newTime = new Date(startTime.getTime() + ratio * totalDuration)
    onTimeChange(newTime)
  }, [startTime, totalDuration, onTimeChange])

  // 드래그로 시간 이동
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

    // 초기 클릭 위치도 적용
    handleTrackClick(e)
  }, [startTime, totalDuration, onTimeChange, handleTrackClick])

  return (
    <div className="bg-white dark:bg-slate-800 rounded-lg p-4 shadow border border-slate-200 dark:border-slate-700">
      {/* 컨트롤 버튼 */}
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center space-x-2">
          {/* 처음으로 */}
          <button
            onClick={onSkipToStart}
            className="p-2 rounded-lg text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-700 transition-colors"
            title={t('replay.skipToStart', '처음으로')}
          >
            <SkipBack className="w-5 h-5" />
          </button>

          {/* 재생/일시정지 */}
          <button
            onClick={onPlayPause}
            className="p-2 rounded-lg bg-teal-500 text-white hover:bg-teal-600 transition-colors"
            title={isPlaying ? t('replay.pause', '일시정지') : t('replay.play', '재생')}
          >
            {isPlaying ? <Pause className="w-5 h-5" /> : <Play className="w-5 h-5" />}
          </button>

          {/* 끝으로 */}
          <button
            onClick={onSkipToEnd}
            className="p-2 rounded-lg text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-700 transition-colors"
            title={t('replay.skipToEnd', '끝으로')}
          >
            <SkipForward className="w-5 h-5" />
          </button>

          {/* 현재 시간 */}
          <span className="ml-3 text-sm font-mono text-slate-700 dark:text-slate-300">
            {formatTime(currentTime.toISOString())}
          </span>
        </div>

        {/* 재생 속도 */}
        <div className="flex items-center space-x-2">
          <span className="text-xs text-slate-500 dark:text-slate-400">
            {t('replay.speed', '재생 속도')}:
          </span>
          <select
            value={playbackSpeed}
            onChange={(e) => onSpeedChange(Number(e.target.value))}
            className="px-2 py-1 text-sm rounded border border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-700 text-slate-700 dark:text-slate-300"
          >
            {PLAYBACK_SPEEDS.map((speed) => (
              <option key={speed} value={speed}>
                {speed}x
              </option>
            ))}
          </select>
        </div>
      </div>

      {/* 타임라인 트랙 */}
      <div
        ref={trackRef}
        className="relative h-10 bg-slate-200 dark:bg-slate-700 rounded-lg cursor-pointer overflow-hidden"
        onClick={handleTrackClick}
        onMouseDown={handleMouseDown}
      >
        {/* 앱 세그먼트 */}
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

        {/* 유휴 기간 표시 (줄무늬) */}
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
              title={t('replay.idle', '유휴')}
            >
              {width > 3 && (
                <Moon className="w-3 h-3 text-slate-500 dark:text-slate-400 opacity-70" />
              )}
            </div>
          )
        })}

        {/* 현재 위치 인디케이터 */}
        <div
          className="absolute top-0 w-0.5 h-full bg-red-500 shadow-lg z-10"
          style={{ left: `${currentPosition * 100}%` }}
        >
          {/* 핸들 */}
          <div className="absolute -top-1 -left-1.5 w-4 h-4 bg-red-500 rounded-full shadow-lg" />
        </div>

        {/* 시작/종료 시간 표시 */}
        <div className="absolute bottom-0 left-1 text-xs text-slate-600 dark:text-slate-400 opacity-70">
          {formatTime(startTime.toISOString())}
        </div>
        <div className="absolute bottom-0 right-1 text-xs text-slate-600 dark:text-slate-400 opacity-70">
          {formatTime(endTime.toISOString())}
        </div>
      </div>

      {/* 앱 범례 */}
      {segments.length > 0 && (
        <div className="flex flex-wrap gap-2 mt-3">
          {/* 고유 앱 목록 */}
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
          {/* 유휴 범례 */}
          {idlePeriods.length > 0 && (
            <div className="flex items-center space-x-1">
              <div
                className="w-3 h-3 rounded-sm"
                style={{
                  background: 'repeating-linear-gradient(45deg, rgba(100,116,139,0.5), rgba(100,116,139,0.5) 1px, transparent 1px, transparent 2px)',
                }}
              />
              <span className="text-xs text-slate-600 dark:text-slate-400">
                {t('replay.idle', '유휴')}
              </span>
            </div>
          )}
        </div>
      )}
    </div>
  )
}
