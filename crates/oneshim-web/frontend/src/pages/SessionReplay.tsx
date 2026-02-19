// 세션 리플레이 페이지 - Datadog RUM Replay / Microsoft Clarity 스타일

import { useState, useEffect, useRef, useCallback, useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { AlertCircle, Image, Clock, Tag as TagIcon, AppWindow, Monitor, Play } from 'lucide-react'
import DateRangePicker from '../components/DateRangePicker'
import TimelineScrubber from '../components/TimelineScrubber'
import EventLog from '../components/EventLog'
import { Card, CardHeader, CardTitle, CardContent } from '../components/ui/Card'
import { Badge } from '../components/ui/Badge'
import { Spinner } from '../components/ui/Spinner'
import { EmptyState } from '../components/ui'
import { fetchTimeline, fetchFrameTags } from '../api/client'
import type { TimelineItem } from '../api/client'

export default function SessionReplay() {
  const { t } = useTranslation()

  // 날짜 범위
  const [fromDate, setFromDate] = useState(() => {
    const d = new Date()
    d.setHours(d.getHours() - 1) // 기본 1시간 전
    return d.toISOString().slice(0, 16)
  })
  const [toDate, setToDate] = useState(() => new Date().toISOString().slice(0, 16))

  // 타임라인 데이터 (React Query)
  const { data: timeline, isLoading: loading, error: timelineError } = useQuery({
    queryKey: ['timeline', fromDate, toDate],
    queryFn: () => fetchTimeline({ from: new Date(fromDate).toISOString(), to: new Date(toDate).toISOString() }),
  })
  const error = timelineError ? (timelineError instanceof Error ? timelineError.message : '타임라인 로드 실패') : null

  // 재생 상태
  const [isPlaying, setIsPlaying] = useState(false)
  const [playbackSpeed, setPlaybackSpeed] = useState(1)
  const [currentTime, setCurrentTime] = useState<Date>(new Date())

  // 재생 타이머
  const playIntervalRef = useRef<number | null>(null)

  // 타임라인 로드 시 시작 시간으로 초기화
  useEffect(() => {
    if (timeline?.session) {
      setCurrentTime(new Date(timeline.session.start))
    }
  }, [timeline])

  // 시작/종료 시간
  const startTime = useMemo(() => {
    return timeline?.session ? new Date(timeline.session.start) : new Date()
  }, [timeline])

  const endTime = useMemo(() => {
    return timeline?.session ? new Date(timeline.session.end) : new Date()
  }, [timeline])

  // 현재 시간에 해당하는 프레임 찾기
  const currentFrame = useMemo(() => {
    if (!timeline?.items) return null

    const frames = timeline.items.filter(
      (item): item is Extract<TimelineItem, { type: 'Frame' }> => item.type === 'Frame'
    )

    if (frames.length === 0) return null

    const currentMs = currentTime.getTime()

    // 현재 시간 이전의 가장 가까운 프레임
    let closest: typeof frames[0] | null = null
    let closestDiff = Infinity

    for (const frame of frames) {
      const frameMs = new Date(frame.timestamp).getTime()
      if (frameMs <= currentMs) {
        const diff = currentMs - frameMs
        if (diff < closestDiff) {
          closestDiff = diff
          closest = frame
        }
      }
    }

    return closest
  }, [timeline, currentTime])

  // 현재 프레임 태그 로드 (React Query)
  const { data: currentFrameTags = [] } = useQuery({
    queryKey: ['frameTags', currentFrame?.id],
    queryFn: () => fetchFrameTags(currentFrame!.id),
    enabled: !!currentFrame,
  })

  // 재생 로직
  useEffect(() => {
    if (isPlaying && timeline) {
      playIntervalRef.current = window.setInterval(() => {
        setCurrentTime((prev) => {
          const newTime = new Date(prev.getTime() + playbackSpeed * 1000)
          // 끝에 도달하면 정지
          if (newTime >= endTime) {
            setIsPlaying(false)
            return endTime
          }
          return newTime
        })
      }, 1000)
    } else {
      if (playIntervalRef.current) {
        clearInterval(playIntervalRef.current)
        playIntervalRef.current = null
      }
    }

    return () => {
      if (playIntervalRef.current) {
        clearInterval(playIntervalRef.current)
      }
    }
  }, [isPlaying, playbackSpeed, endTime, timeline])

  // 이벤트 핸들러
  const handlePlayPause = useCallback(() => {
    setIsPlaying((prev) => !prev)
  }, [])

  const handleSpeedChange = useCallback((speed: number) => {
    setPlaybackSpeed(speed)
  }, [])

  const handleTimeChange = useCallback((time: Date) => {
    setCurrentTime(time)
  }, [])

  const handleSkipToStart = useCallback(() => {
    setCurrentTime(startTime)
    setIsPlaying(false)
  }, [startTime])

  const handleSkipToEnd = useCallback(() => {
    setCurrentTime(endTime)
    setIsPlaying(false)
  }, [endTime])

  const handleDateRangeChange = useCallback((from: string | undefined, to: string | undefined) => {
    if (from) setFromDate(from)
    if (to) setToDate(to)
  }, [])

  // 시간 포맷 (상세)
  const formatDetailTime = (date: Date) => {
    return date.toLocaleString('ko-KR', {
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    })
  }

  return (
    <div className="space-y-4">
      {/* 헤더 */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <h1 className="text-2xl font-bold text-slate-900 dark:text-white">
          {t('replay.title', '세션 리플레이')}
        </h1>
        <DateRangePicker
          onRangeChange={handleDateRangeChange}
          initialFrom={fromDate.split('T')[0]}
          initialTo={toDate.split('T')[0]}
        />
      </div>

      {/* 에러 메시지 */}
      {error && (
        <div className="flex items-center space-x-2 text-red-500 bg-red-50 dark:bg-red-900/20 p-3 rounded-lg">
          <AlertCircle className="w-5 h-5" />
          <span>{error}</span>
        </div>
      )}

      {/* 로딩 */}
      {loading && (
        <div className="flex items-center justify-center py-12">
          <Spinner />
        </div>
      )}

      {/* 빈 상태 */}
      {!loading && (!timeline || timeline.items.length === 0) && !error && (
        <EmptyState
          icon={<Play className="w-8 h-8" />}
          title={t('emptyState.replay.title')}
          description={t('emptyState.replay.description')}
        />
      )}

      {/* 메인 콘텐츠 */}
      {!loading && timeline && timeline.items.length > 0 && (
        <>
          {/* 타임라인 스크러버 */}
          <TimelineScrubber
            startTime={startTime}
            endTime={endTime}
            currentTime={currentTime}
            isPlaying={isPlaying}
            playbackSpeed={playbackSpeed}
            segments={timeline.segments}
            items={timeline.items}
            onTimeChange={handleTimeChange}
            onPlayPause={handlePlayPause}
            onSpeedChange={handleSpeedChange}
            onSkipToStart={handleSkipToStart}
            onSkipToEnd={handleSkipToEnd}
          />

          {/* 스크린샷 뷰어 + 이벤트 로그 */}
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-4">
            {/* 스크린샷 뷰어 (2/3) */}
            <div className="lg:col-span-2">
              <Card>
                <CardHeader>
                  <CardTitle>
                    {currentFrame
                      ? `${currentFrame.app_name} - ${currentFrame.window_title}`
                      : t('replay.selectTime', '시간을 선택하세요')}
                  </CardTitle>
                </CardHeader>
                <CardContent>
                  {currentFrame ? (
                    <div className="space-y-4">
                      {/* 이미지 */}
                      <div className="relative aspect-video bg-slate-100 dark:bg-slate-700 rounded-lg overflow-hidden">
                        <img
                          src={currentFrame.image_url}
                          alt={`Screenshot at ${currentFrame.timestamp}`}
                          className="w-full h-full object-contain"
                          onError={(e) => {
                            e.currentTarget.style.display = 'none'
                          }}
                        />
                      </div>

                      {/* 프레임 메타데이터 */}
                      <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
                        <div className="flex items-center space-x-2 text-sm">
                          <AppWindow className="w-4 h-4 text-slate-400" />
                          <span className="text-slate-600 dark:text-slate-400">
                            {currentFrame.app_name}
                          </span>
                        </div>
                        <div className="flex items-center space-x-2 text-sm">
                          <Monitor className="w-4 h-4 text-slate-400" />
                          <span className="text-slate-600 dark:text-slate-400 truncate">
                            {currentFrame.window_title}
                          </span>
                        </div>
                        <div className="flex items-center space-x-2 text-sm">
                          <Clock className="w-4 h-4 text-slate-400" />
                          <span className="text-slate-600 dark:text-slate-400">
                            {formatDetailTime(new Date(currentFrame.timestamp))}
                          </span>
                        </div>
                        <div className="flex items-center space-x-2 text-sm">
                          <span className="text-slate-500 dark:text-slate-400">
                            {t('search.importance', '중요도')}:
                          </span>
                          <Badge color={currentFrame.importance >= 0.7 ? 'success' : currentFrame.importance >= 0.4 ? 'warning' : 'default'}>
                            {Math.round(currentFrame.importance * 100)}%
                          </Badge>
                        </div>
                      </div>

                      {/* 태그 */}
                      {currentFrameTags.length > 0 && (
                        <div className="flex items-center flex-wrap gap-2">
                          <TagIcon className="w-4 h-4 text-slate-400" />
                          {currentFrameTags.map((tag) => (
                            <span
                              key={tag.id}
                              className="px-2 py-0.5 text-xs rounded-full text-white"
                              style={{ backgroundColor: tag.color }}
                            >
                              {tag.name}
                            </span>
                          ))}
                        </div>
                      )}
                    </div>
                  ) : (
                    <div className="flex flex-col items-center justify-center py-12 text-slate-500 dark:text-slate-400">
                      <Image className="w-12 h-12 mb-3 opacity-50" />
                      <p>{t('replay.noFrames', '해당 시간의 프레임이 없습니다')}</p>
                    </div>
                  )}
                </CardContent>
              </Card>
            </div>

            {/* 이벤트 로그 (1/3) */}
            <div className="lg:col-span-1 h-[500px]">
              <EventLog
                items={timeline.items}
                currentTime={currentTime}
                onItemClick={handleTimeChange}
              />
            </div>
          </div>

          {/* 세션 통계 */}
          <Card>
            <CardContent className="py-3">
              <div className="grid grid-cols-2 sm:grid-cols-5 gap-4 text-center">
                <div>
                  <p className="text-xs text-slate-500 dark:text-slate-400">
                    {t('replay.duration', '세션 시간')}
                  </p>
                  <p className="text-lg font-semibold text-slate-900 dark:text-white">
                    {Math.round(timeline.session.duration_secs / 60)}{t('dashboard.minutes', '분')}
                  </p>
                </div>
                <div>
                  <p className="text-xs text-slate-500 dark:text-slate-400">
                    {t('replay.totalEvents', '총 이벤트')}
                  </p>
                  <p className="text-lg font-semibold text-slate-900 dark:text-white">
                    {timeline.session.total_events}
                  </p>
                </div>
                <div>
                  <p className="text-xs text-slate-500 dark:text-slate-400">
                    {t('replay.totalFrames', '총 프레임')}
                  </p>
                  <p className="text-lg font-semibold text-slate-900 dark:text-white">
                    {timeline.session.total_frames}
                  </p>
                </div>
                <div>
                  <p className="text-xs text-slate-500 dark:text-slate-400">
                    {t('replay.totalIdle', '총 유휴')}
                  </p>
                  <p className="text-lg font-semibold text-slate-900 dark:text-white">
                    {Math.round(timeline.session.total_idle_secs / 60)}{t('dashboard.minutes', '분')}
                  </p>
                </div>
                <div>
                  <p className="text-xs text-slate-500 dark:text-slate-400">
                    {t('replay.apps', '앱 수')}
                  </p>
                  <p className="text-lg font-semibold text-slate-900 dark:text-white">
                    {new Set(timeline.segments.map(s => s.app_name)).size}
                  </p>
                </div>
              </div>
            </CardContent>
          </Card>
        </>
      )}
    </div>
  )
}
