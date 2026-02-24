// 세션 리플레이 페이지 - Datadog RUM Replay / Microsoft Clarity 스타일

import { useState, useEffect, useRef, useCallback, useMemo } from 'react'
import { useMutation, useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { AlertCircle, Image, Clock, Tag as TagIcon, AppWindow, Monitor, Play, Eye, EyeOff } from 'lucide-react'
import DateRangePicker from '../components/DateRangePicker'
import TimelineScrubber from '../components/TimelineScrubber'
import EventLog from '../components/EventLog'
import { Card, CardHeader, CardTitle, CardContent } from '../components/ui/Card'
import { Badge } from '../components/ui/Badge'
import { Spinner } from '../components/ui/Spinner'
import { Button } from '../components/ui/Button'
import { EmptyState } from '../components/ui'
import {
  executeSceneAction,
  fetchTimeline,
  fetchFrameTags,
  fetchAutomationScene,
  fetchSceneCalibration,
  fetchSettings,
} from '../api/client'
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
  const [imageLoadFailed, setImageLoadFailed] = useState(false)
  const [showSceneOverlay, setShowSceneOverlay] = useState(true)
  const [selectedSceneElementId, setSelectedSceneElementId] = useState<string | null>(null)
  const [sceneTypeText, setSceneTypeText] = useState('')
  const [allowSensitiveInput, setAllowSensitiveInput] = useState(false)
  const [sceneActionFeedback, setSceneActionFeedback] = useState<{
    success: boolean
    message: string
  } | null>(null)
  const sceneViewportRef = useRef<HTMLDivElement | null>(null)
  const [sceneViewportSize, setSceneViewportSize] = useState({ width: 0, height: 0 })

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

  useEffect(() => {
    setImageLoadFailed(false)
  }, [currentFrame?.id])

  // 현재 프레임 태그 로드 (React Query)
  const { data: currentFrameTags = [] } = useQuery({
    queryKey: ['frameTags', currentFrame?.id],
    queryFn: () => fetchFrameTags(currentFrame!.id),
    enabled: !!currentFrame,
  })

  const { data: appSettings } = useQuery({
    queryKey: ['settings'],
    queryFn: fetchSettings,
    staleTime: 30000,
  })

  const {
    data: currentScene,
    isFetching: sceneFetching,
    error: sceneError,
  } = useQuery({
    queryKey: ['automationScene', currentFrame?.id, currentFrame?.app_name],
    queryFn: () =>
      fetchAutomationScene({
        appName: currentFrame?.app_name,
        frameId: currentFrame?.id,
      }),
    enabled: !!currentFrame,
    retry: false,
  })

  const {
    data: sceneCalibration,
    isFetching: calibrationFetching,
  } = useQuery({
    queryKey: ['sceneCalibration', currentFrame?.id, currentFrame?.app_name],
    queryFn: () =>
      fetchSceneCalibration({
        appName: currentFrame?.app_name,
        frameId: currentFrame?.id,
      }),
    enabled: !!currentFrame,
    retry: false,
  })

  const sceneIntelligenceEnabled = appSettings?.ai_provider.scene_intelligence.enabled ?? true
  const overlayAllowed = appSettings?.ai_provider.scene_intelligence.overlay_enabled ?? true
  const sceneExecutionAllowed =
    appSettings?.ai_provider.scene_intelligence.allow_action_execution ?? false
  const sceneCalibrationPassed = sceneCalibration?.passed === true
  const sceneCalibrationReasons = Array.isArray(sceneCalibration?.reasons)
    ? sceneCalibration.reasons
    : []

  useEffect(() => {
    setSelectedSceneElementId(null)
    setSceneTypeText('')
    setAllowSensitiveInput(false)
    setSceneActionFeedback(null)
  }, [currentFrame?.id, currentScene?.scene_id])

  useEffect(() => {
    if (!overlayAllowed) {
      setShowSceneOverlay(false)
    }
  }, [overlayAllowed, currentFrame?.id])

  useEffect(() => {
    const target = sceneViewportRef.current
    if (!target) return

    const updateSize = () => {
      const rect = target.getBoundingClientRect()
      setSceneViewportSize({ width: rect.width, height: rect.height })
    }

    updateSize()
    const observer = new ResizeObserver(updateSize)
    observer.observe(target)

    return () => {
      observer.disconnect()
    }
  }, [currentFrame?.id])

  const projectedSceneElements = useMemo(() => {
    if (!showSceneOverlay || !currentScene) return []
    const viewportWidth = sceneViewportSize.width
    const viewportHeight = sceneViewportSize.height
    if (viewportWidth <= 0 || viewportHeight <= 0) return []

    const sceneWidth = Math.max(currentScene.screen_width, 1)
    const sceneHeight = Math.max(currentScene.screen_height, 1)
    const scale = Math.min(viewportWidth / sceneWidth, viewportHeight / sceneHeight)
    const renderWidth = sceneWidth * scale
    const renderHeight = sceneHeight * scale
    const offsetX = (viewportWidth - renderWidth) / 2
    const offsetY = (viewportHeight - renderHeight) / 2

    return currentScene.elements
      .map((element) => {
        const left = offsetX + element.bbox_abs.x * scale
        const top = offsetY + element.bbox_abs.y * scale
        const width = Math.max(element.bbox_abs.width * scale, 1)
        const height = Math.max(element.bbox_abs.height * scale, 1)

        return {
          ...element,
          left,
          top,
          width,
          height,
          title: element.role ?? element.label,
        }
      })
      .filter(
        (element) =>
          Number.isFinite(element.left) &&
          Number.isFinite(element.top) &&
          element.width > 1 &&
          element.height > 1
      )
  }, [showSceneOverlay, currentScene, sceneViewportSize])

  const selectedSceneElement = useMemo(
    () =>
      selectedSceneElementId
        ? projectedSceneElements.find((element) => element.element_id === selectedSceneElementId) ?? null
        : null,
    [projectedSceneElements, selectedSceneElementId]
  )

  const selectedActionType = useMemo<'click' | 'type_text'>(() => {
    if (!selectedSceneElement) return 'click'
    const role = selectedSceneElement.role?.toLowerCase() ?? ''
    if (role.includes('input') || role.includes('textbox') || role.includes('field')) {
      return 'type_text'
    }
    return 'click'
  }, [selectedSceneElement])

  const suggestedActionText = useMemo(() => {
    if (!selectedSceneElement) return ''
    const label = selectedSceneElement.label?.trim() || t('replay.unnamedElement', 'Unnamed element')
    const appName = currentFrame?.app_name || t('replay.currentApp', 'current app')
    if (selectedActionType === 'type_text') {
      return t('replay.suggestTypeHint', { label, app: appName, defaultValue: `Type into "${label}" in ${appName}` })
    }
    return t('replay.suggestClickHint', { label, app: appName, defaultValue: `Click "${label}" in ${appName}` })
  }, [selectedSceneElement, currentFrame?.app_name, selectedActionType, t])

  const executeSceneActionMutation = useMutation({
    mutationFn: executeSceneAction,
    onSuccess: (response) => {
      const ok = response.result.success
      setSceneActionFeedback({
        success: ok,
        message: ok
          ? t('replay.actionSuccessWithPolicy', {
              defaultValue: 'Suggested action executed (policy: {{policy}}).',
              policy: response.applied_privacy_policy,
            }) +
            (response.scene_action_override_active
              ? ` ${t('replay.overrideActiveSuffix', '(override active)')}`
              : '')
          : response.result.error || t('replay.actionFailed', 'Suggested action failed.'),
      })
    },
    onError: (mutationError) => {
      const message =
        mutationError instanceof Error
          ? mutationError.message
          : t('replay.actionFailed', 'Suggested action failed.')
      setSceneActionFeedback({
        success: false,
        message,
      })
    },
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
                      <div
                        ref={sceneViewportRef}
                        className="relative aspect-video bg-slate-100 dark:bg-slate-700 rounded-lg overflow-hidden"
                      >
                        {!imageLoadFailed ? (
                          <img
                            src={currentFrame.image_url}
                            alt={`Screenshot at ${currentFrame.timestamp}`}
                            className="w-full h-full object-contain"
                            onError={() => setImageLoadFailed(true)}
                          />
                        ) : (
                          <div className="w-full h-full flex items-center justify-center text-sm text-slate-600 dark:text-slate-300 px-4 text-center">
                            {t('replay.imageUnavailable', '스크린샷 이미지를 불러오지 못했습니다. 파일 보존 정책 또는 경로 상태를 확인하세요.')}
                          </div>
                        )}
                        {!imageLoadFailed &&
                          showSceneOverlay &&
                          projectedSceneElements.map((element) => (
                            <button
                              type="button"
                              key={element.element_id}
                              className={`absolute transition-colors ${
                                selectedSceneElementId === element.element_id
                                  ? 'border-2 border-amber-400 bg-amber-400/20'
                                  : 'border border-teal-500/90 bg-teal-500/10 hover:bg-teal-500/20'
                              }`}
                              style={{
                                left: `${element.left}px`,
                                top: `${element.top}px`,
                                width: `${element.width}px`,
                                height: `${element.height}px`,
                              }}
                              title={element.title}
                              onClick={() => {
                                setSelectedSceneElementId(element.element_id)
                                setSceneActionFeedback(null)
                              }}
                            >
                              <span className="pointer-events-none absolute -top-5 left-0 max-w-[12rem] truncate rounded bg-teal-600 px-1.5 py-0.5 text-[10px] text-white shadow">
                                {element.title}
                              </span>
                            </button>
                          ))}
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

                      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-2">
                        <div className="text-xs text-slate-500 dark:text-slate-400">
                          {t('replay.sceneElements', { count: currentScene?.elements.length ?? 0 })}
                          {sceneFetching && (
                            <span className="ml-2 text-slate-400 dark:text-slate-500">
                              {t('common.loading')}
                            </span>
                          )}
                          {sceneError && (
                            <span className="ml-2 text-amber-600 dark:text-amber-400">
                              {t('replay.sceneUnavailable')}
                            </span>
                          )}
                          {!sceneError && sceneCalibration && (
                            <span
                              className={`ml-2 ${
                                sceneCalibrationPassed
                                  ? 'text-emerald-600 dark:text-emerald-400'
                                  : 'text-amber-600 dark:text-amber-400'
                              }`}
                            >
                              {sceneCalibrationPassed
                                ? t('replay.calibrationPassed', 'Calibration passed')
                                : t('replay.calibrationFailed', 'Calibration failed')}
                            </span>
                          )}
                          {calibrationFetching && (
                            <span className="ml-2 text-slate-400 dark:text-slate-500">
                              {t('replay.calibrating', 'Calibrating...')}
                            </span>
                          )}
                        </div>
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={() => setShowSceneOverlay((prev) => !prev)}
                          disabled={!currentScene || imageLoadFailed || !overlayAllowed}
                        >
                          {showSceneOverlay ? (
                            <>
                              <EyeOff className="w-4 h-4 mr-1" />
                              {t('replay.hideOverlay')}
                            </>
                          ) : (
                            <>
                              <Eye className="w-4 h-4 mr-1" />
                              {t('replay.showOverlay')}
                            </>
                          )}
                        </Button>
                      </div>
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
            <div className="lg:col-span-1 space-y-4">
              <Card>
                <CardHeader>
                  <CardTitle>{t('replay.assistantTitle', 'Action Assistant')}</CardTitle>
                </CardHeader>
                <CardContent className="space-y-3">
                  <p className="text-xs text-slate-500 dark:text-slate-400">
                    {t(
                      'replay.assistantDescription',
                      'Click a highlighted element to prepare an automation action.'
                    )}
                  </p>
                  {selectedSceneElement ? (
                    <>
                      <div className="rounded-lg border border-slate-200 dark:border-slate-700 p-3 space-y-2">
                        <div className="text-sm font-semibold text-slate-900 dark:text-white truncate">
                          {selectedSceneElement.label}
                        </div>
                        <div className="grid grid-cols-2 gap-2 text-xs text-slate-600 dark:text-slate-300">
                          <div>
                            {t('replay.role', 'Role')}: {selectedSceneElement.role ?? t('replay.unknown', 'Unknown')}
                          </div>
                          <div>
                            {t('replay.intent', 'Intent')}: {selectedSceneElement.intent ?? t('replay.unknown', 'Unknown')}
                          </div>
                          <div className="col-span-2">
                            {t('replay.confidence', 'Confidence')}:{' '}
                            {Math.round(selectedSceneElement.confidence * 100)}%
                          </div>
                        </div>
                      </div>

                      <div className="rounded-lg bg-slate-100 dark:bg-slate-800 px-3 py-2 text-xs text-slate-700 dark:text-slate-200 break-words">
                        <span className="font-medium">{t('replay.suggestedAction', 'Suggested action')}: </span>
                        {suggestedActionText}
                      </div>

                      {!sceneIntelligenceEnabled && (
                        <div className="rounded-lg px-3 py-2 text-xs bg-amber-100 text-amber-800 dark:bg-amber-900/30 dark:text-amber-300">
                          {t(
                            'replay.sceneIntelligenceDisabled',
                            'Scene intelligence is disabled in settings.'
                          )}
                        </div>
                      )}
                      {sceneCalibration && !sceneCalibrationPassed && sceneCalibrationReasons.length > 0 && (
                        <div className="rounded-lg px-3 py-2 text-xs bg-amber-100 text-amber-800 dark:bg-amber-900/30 dark:text-amber-300">
                          {t(
                            'replay.calibrationReasons',
                            'Calibration notes'
                          )}: {sceneCalibrationReasons.join('; ')}
                        </div>
                      )}

                      {selectedActionType === 'type_text' && (
                        <div className="space-y-2">
                          <label className="text-xs text-slate-600 dark:text-slate-300">
                            {t('replay.typeTextLabel', 'Input Text')}
                          </label>
                          <input
                            value={sceneTypeText}
                            onChange={(e) => setSceneTypeText(e.target.value)}
                            placeholder={t('replay.typeTextPlaceholder', 'Enter text to type')}
                            className="w-full px-2 py-1.5 rounded-md border border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800 text-sm text-slate-900 dark:text-slate-100"
                          />
                          <label className="flex items-center gap-2 text-xs text-slate-600 dark:text-slate-300">
                            <input
                              type="checkbox"
                              checked={allowSensitiveInput}
                              onChange={(e) => setAllowSensitiveInput(e.target.checked)}
                            />
                            {t(
                              'replay.allowSensitiveInput',
                              'Allow sensitive text input under current privacy policy'
                            )}
                          </label>
                        </div>
                      )}

                      <div className="flex flex-wrap gap-2">
                        <Button
                          size="sm"
                          isLoading={executeSceneActionMutation.isPending}
                          onClick={() => {
                            if (!selectedSceneElement) return
                            executeSceneActionMutation.mutate({
                              command_id: `replay-scene-${currentFrame?.id ?? 'frame'}-${Date.now()}`,
                              session_id: `replay-${currentFrame?.id ?? 'frame'}`,
                              frame_id: currentFrame?.id,
                              scene_id: currentScene?.scene_id,
                              element_id: selectedSceneElement.element_id,
                              action_type: selectedActionType,
                              bbox_abs: selectedSceneElement.bbox_abs,
                              role: selectedSceneElement.role,
                              label: selectedSceneElement.label,
                              text: selectedActionType === 'type_text' ? sceneTypeText : undefined,
                              allow_sensitive_input:
                                selectedActionType === 'type_text'
                                  ? allowSensitiveInput
                                  : undefined,
                            })
                          }}
                          disabled={
                            !sceneExecutionAllowed ||
                            (selectedActionType === 'type_text' && sceneTypeText.trim().length === 0)
                          }
                        >
                          {t('replay.runSuggestedAction', 'Run Suggested Action')}
                        </Button>
                        <Button
                          variant="secondary"
                          size="sm"
                          onClick={() => {
                            setSelectedSceneElementId(null)
                            setSceneActionFeedback(null)
                          }}
                        >
                          {t('replay.clearSelection', 'Clear Selection')}
                        </Button>
                      </div>

                      {!sceneExecutionAllowed && (
                        <div className="rounded-lg px-3 py-2 text-xs bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-300">
                          {t(
                            'replay.sceneExecutionDisabled',
                            'Scene action execution is disabled in Settings > Automation.'
                          )}
                        </div>
                      )}

                      {sceneActionFeedback && (
                        <div
                          className={`rounded-lg px-3 py-2 text-xs ${
                            sceneActionFeedback.success
                              ? 'bg-emerald-100 text-emerald-800 dark:bg-emerald-900/30 dark:text-emerald-300'
                              : 'bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-300'
                          }`}
                        >
                          {sceneActionFeedback.message}
                        </div>
                      )}
                    </>
                  ) : (
                    <div className="rounded-lg border border-dashed border-slate-300 dark:border-slate-700 px-3 py-4 text-sm text-slate-500 dark:text-slate-400">
                      {t('replay.noElementSelected', 'No element selected.')}
                    </div>
                  )}
                </CardContent>
              </Card>

              <div className="h-[500px]">
                <EventLog
                  items={timeline.items}
                  currentTime={currentTime}
                  onItemClick={handleTimeChange}
                />
              </div>
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
