import { useMutation, useQuery } from '@tanstack/react-query'
import { AlertCircle, AppWindow, Clock, Eye, EyeOff, Image, Monitor, Play, Tag as TagIcon } from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import type { TimelineItem } from '../api/client'
import {
  executeSceneAction,
  fetchAutomationScene,
  fetchFrameTags,
  fetchSceneCalibration,
  fetchSettings,
  fetchTimeline,
} from '../api/client'
import DateRangePicker from '../components/DateRangePicker'
import EventLog from '../components/EventLog'
import TimelineScrubber from '../components/TimelineScrubber'
import { EmptyState } from '../components/ui'
import { Badge } from '../components/ui/Badge'
import { Button } from '../components/ui/Button'
import { Card, CardContent, CardHeader, CardTitle } from '../components/ui/Card'
import { Spinner } from '../components/ui/Spinner'
import { colors, typography } from '../styles/tokens'
import { cn } from '../utils/cn'

export default function SessionReplay() {
  const { t } = useTranslation()

  const [fromDate, setFromDate] = useState(() => {
    const d = new Date()
    d.setHours(d.getHours() - 1) // default to 1 hour ago
    return d.toISOString().slice(0, 16)
  })
  const [toDate, setToDate] = useState(() => new Date().toISOString().slice(0, 16))

  const {
    data: timeline,
    isLoading: loading,
    error: timelineError,
  } = useQuery({
    queryKey: ['timeline', fromDate, toDate],
    queryFn: () => fetchTimeline({ from: new Date(fromDate).toISOString(), to: new Date(toDate).toISOString() }),
  })
  const error = timelineError
    ? timelineError instanceof Error
      ? timelineError.message
      : '타임라인 로드 failure'
    : null

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
  const sceneObserverRef = useRef<ResizeObserver | null>(null)
  const [sceneViewportSize, setSceneViewportSize] = useState({ width: 0, height: 0 })

  const playIntervalRef = useRef<number | null>(null)

  useEffect(() => {
    if (timeline?.session) {
      setCurrentTime(new Date(timeline.session.start))
    }
  }, [timeline])

  const startTime = useMemo(() => {
    return timeline?.session ? new Date(timeline.session.start) : new Date()
  }, [timeline])

  const endTime = useMemo(() => {
    return timeline?.session ? new Date(timeline.session.end) : new Date()
  }, [timeline])

  const currentFrame = useMemo(() => {
    if (!timeline?.items) return null

    const frames = timeline.items.filter(
      (item): item is Extract<TimelineItem, { type: 'Frame' }> => item.type === 'Frame',
    )

    if (frames.length === 0) return null

    const currentMs = currentTime.getTime()

    let closest: (typeof frames)[0] | null = null
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
  }, [])

  const { data: currentFrameTags = [] } = useQuery({
    queryKey: ['frameTags', currentFrame?.id],
    // biome-ignore lint/style/noNonNullAssertion: guarded by enabled: !!currentFrame
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

  const { data: sceneCalibration, isFetching: calibrationFetching } = useQuery({
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
  const sceneExecutionAllowed = appSettings?.ai_provider.scene_intelligence.allow_action_execution ?? false
  const sceneCalibrationPassed = sceneCalibration?.passed === true
  const sceneCalibrationReasons = Array.isArray(sceneCalibration?.reasons) ? sceneCalibration.reasons : []

  useEffect(() => {
    setSelectedSceneElementId(null)
    setSceneTypeText('')
    setAllowSensitiveInput(false)
    setSceneActionFeedback(null)
  }, [])

  useEffect(() => {
    if (!overlayAllowed) {
      setShowSceneOverlay(false)
    }
  }, [overlayAllowed])

  const sceneViewportCallbackRef = useCallback((node: HTMLDivElement | null) => {
    if (sceneObserverRef.current) {
      sceneObserverRef.current.disconnect()
      sceneObserverRef.current = null
    }
    sceneViewportRef.current = node
    if (!node) return

    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const { width, height } = entry.contentRect
        if (width > 0 && height > 0) {
          setSceneViewportSize({ width, height })
        }
      }
    })
    observer.observe(node)
    sceneObserverRef.current = observer
  }, [])

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
          Number.isFinite(element.left) && Number.isFinite(element.top) && element.width > 1 && element.height > 1,
      )
  }, [showSceneOverlay, currentScene, sceneViewportSize])

  const selectedSceneElement = useMemo(
    () =>
      selectedSceneElementId
        ? (projectedSceneElements.find((element) => element.element_id === selectedSceneElementId) ?? null)
        : null,
    [projectedSceneElements, selectedSceneElementId],
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
            (response.scene_action_override_active ? ` ${t('replay.overrideActiveSuffix', '(override active)')}` : '')
          : response.result.error || t('replay.actionFailed', 'Suggested action failed.'),
      })
    },
    onError: (mutationError) => {
      const message =
        mutationError instanceof Error ? mutationError.message : t('replay.actionFailed', 'Suggested action failed.')
      setSceneActionFeedback({
        success: false,
        message,
      })
    },
  })

  useEffect(() => {
    if (isPlaying && timeline) {
      playIntervalRef.current = window.setInterval(() => {
        setCurrentTime((prev) => {
          const newTime = new Date(prev.getTime() + playbackSpeed * 1000)
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
    <div className="h-full space-y-4 overflow-y-auto p-6">
      {/* UI note */}
      <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <h1 className={cn(typography.h1, colors.text.primary)}>{t('replay.title', 'session 리플레이')}</h1>
        <DateRangePicker
          onRangeChange={handleDateRangeChange}
          initialFrom={fromDate.split('T')[0]}
          initialTo={toDate.split('T')[0]}
        />
      </div>

      {/* UI note */}
      {error && (
        <div className="flex items-center space-x-2 rounded-lg bg-accent-red/10 p-3 text-accent-red">
          <AlertCircle className="h-5 w-5" />
          <span>{error}</span>
        </div>
      )}

      {/* UI note */}
      {loading && (
        <div className="flex items-center justify-center py-12">
          <Spinner />
        </div>
      )}

      {/* UI note */}
      {!loading && (!timeline || timeline.items.length === 0) && !error && (
        <EmptyState
          icon={<Play className="h-8 w-8" />}
          title={t('emptyState.replay.title')}
          description={t('emptyState.replay.description')}
        />
      )}

      {/* UI note */}
      {!loading && timeline && timeline.items.length > 0 && (
        <>
          {/* UI note */}
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

          {/* UI note */}
          <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
            {/* UI note */}
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
                      {/* UI note */}
                      <div
                        ref={sceneViewportCallbackRef}
                        className="relative aspect-video overflow-hidden rounded-lg bg-surface-elevated"
                      >
                        {!imageLoadFailed ? (
                          <img
                            src={currentFrame.image_url}
                            alt={`Screenshot at ${currentFrame.timestamp}`}
                            className="h-full w-full object-contain"
                            onError={() => setImageLoadFailed(true)}
                          />
                        ) : (
                          <div className="flex h-full w-full items-center justify-center px-4 text-center text-content-secondary text-sm">
                            {t(
                              'replay.imageUnavailable',
                              '스크린샷 이미지를 불러오지 못했습니다. file 보존 policy 또는 path state를 확인하세요.',
                            )}
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

                      {/* UI note */}
                      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
                        <div className="flex items-center space-x-2 text-sm">
                          <AppWindow className="h-4 w-4 text-content-muted" />
                          <span className="text-content-secondary">{currentFrame.app_name}</span>
                        </div>
                        <div className="flex items-center space-x-2 text-sm">
                          <Monitor className="h-4 w-4 text-content-muted" />
                          <span className="truncate text-content-secondary">{currentFrame.window_title}</span>
                        </div>
                        <div className="flex items-center space-x-2 text-sm">
                          <Clock className="h-4 w-4 text-content-muted" />
                          <span className="text-content-secondary">
                            {formatDetailTime(new Date(currentFrame.timestamp))}
                          </span>
                        </div>
                        <div className="flex items-center space-x-2 text-sm">
                          <span className="text-content-secondary">{t('search.importance', '중요도')}:</span>
                          <Badge
                            color={
                              currentFrame.importance >= 0.7
                                ? 'success'
                                : currentFrame.importance >= 0.4
                                  ? 'warning'
                                  : 'default'
                            }
                          >
                            {Math.round(currentFrame.importance * 100)}%
                          </Badge>
                        </div>
                      </div>

                      {/* UI note */}
                      {currentFrameTags.length > 0 && (
                        <div className="flex flex-wrap items-center gap-2">
                          <TagIcon className="h-4 w-4 text-content-muted" />
                          {currentFrameTags.map((tag) => (
                            <span
                              key={tag.id}
                              className="rounded-full px-2 py-0.5 text-white text-xs"
                              style={{ backgroundColor: tag.color }}
                            >
                              {tag.name}
                            </span>
                          ))}
                        </div>
                      )}

                      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
                        <div className="text-content-secondary text-xs">
                          {t('replay.sceneElements', { count: currentScene?.elements.length ?? 0 })}
                          {sceneFetching && <span className="ml-2 text-content-muted">{t('common.loading')}</span>}
                          {sceneError && (
                            <span className="ml-2 text-semantic-warning">{t('replay.sceneUnavailable')}</span>
                          )}
                          {!sceneError && sceneCalibration && (
                            <span
                              className={`ml-2 ${
                                sceneCalibrationPassed ? 'text-accent-emerald' : 'text-semantic-warning'
                              }`}
                            >
                              {sceneCalibrationPassed
                                ? t('replay.calibrationPassed', 'Calibration passed')
                                : t('replay.calibrationFailed', 'Calibration failed')}
                            </span>
                          )}
                          {calibrationFetching && (
                            <span className="ml-2 text-content-muted">{t('replay.calibrating', 'Calibrating...')}</span>
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
                              <EyeOff className="mr-1 h-4 w-4" />
                              {t('replay.hideOverlay')}
                            </>
                          ) : (
                            <>
                              <Eye className="mr-1 h-4 w-4" />
                              {t('replay.showOverlay')}
                            </>
                          )}
                        </Button>
                      </div>
                    </div>
                  ) : (
                    <div className="flex flex-col items-center justify-center py-12 text-content-secondary">
                      <Image className="mb-3 h-12 w-12 opacity-50" />
                      <p>{t('replay.noFrames', '해당 시간의 frame이 없습니다')}</p>
                    </div>
                  )}
                </CardContent>
              </Card>
            </div>

            {/* UI note */}
            <div className="space-y-4 lg:col-span-1">
              <Card>
                <CardHeader>
                  <CardTitle>{t('replay.assistantTitle', 'Action Assistant')}</CardTitle>
                </CardHeader>
                <CardContent className="space-y-3">
                  <p className="text-content-secondary text-xs">
                    {t('replay.assistantDescription', 'Click a highlighted element to prepare an automation action.')}
                  </p>
                  {selectedSceneElement ? (
                    <>
                      <div className="space-y-2 rounded-lg border border-muted p-3">
                        <div className="truncate font-semibold text-content text-sm">{selectedSceneElement.label}</div>
                        <div className="grid grid-cols-2 gap-2 text-content-secondary text-xs">
                          <div>
                            {t('replay.role', 'Role')}: {selectedSceneElement.role ?? t('replay.unknown', 'Unknown')}
                          </div>
                          <div>
                            {t('replay.intent', 'Intent')}:{' '}
                            {selectedSceneElement.intent ?? t('replay.unknown', 'Unknown')}
                          </div>
                          <div className="col-span-2">
                            {t('replay.confidence', 'Confidence')}: {Math.round(selectedSceneElement.confidence * 100)}%
                          </div>
                        </div>
                      </div>

                      <div className="break-words rounded-lg bg-surface-elevated px-3 py-2 text-content-strong text-xs">
                        <span className="font-medium">{t('replay.suggestedAction', 'Suggested action')}: </span>
                        {suggestedActionText}
                      </div>

                      {!sceneIntelligenceEnabled && (
                        <div className="rounded-lg bg-semantic-warning/10 px-3 py-2 text-semantic-warning text-xs">
                          {t('replay.sceneIntelligenceDisabled', 'Scene intelligence is disabled in settings.')}
                        </div>
                      )}
                      {sceneCalibration && !sceneCalibrationPassed && sceneCalibrationReasons.length > 0 && (
                        <div className="rounded-lg bg-semantic-warning/10 px-3 py-2 text-semantic-warning text-xs">
                          {t('replay.calibrationReasons', 'Calibration notes')}: {sceneCalibrationReasons.join('; ')}
                        </div>
                      )}

                      {selectedActionType === 'type_text' && (
                        <div className="space-y-2">
                          <label htmlFor="scene-type-text" className="text-content-secondary text-xs">
                            {t('replay.typeTextLabel', 'Input Text')}
                          </label>
                          <input
                            id="scene-type-text"
                            value={sceneTypeText}
                            onChange={(e) => setSceneTypeText(e.target.value)}
                            placeholder={t('replay.typeTextPlaceholder', 'Enter text to type')}
                            className="w-full rounded-md border border-DEFAULT bg-surface-overlay px-2 py-1.5 text-content text-sm"
                          />
                          <label className="flex items-center gap-2 text-content-secondary text-xs">
                            <input
                              type="checkbox"
                              checked={allowSensitiveInput}
                              onChange={(e) => setAllowSensitiveInput(e.target.checked)}
                            />
                            {t('replay.allowSensitiveInput', 'Allow sensitive text input under current privacy policy')}
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
                                selectedActionType === 'type_text' ? allowSensitiveInput : undefined,
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
                        <div className="rounded-lg bg-surface-elevated px-3 py-2 text-content-strong text-xs">
                          {t(
                            'replay.sceneExecutionDisabled',
                            'Scene action execution is disabled in Settings > Automation.',
                          )}
                        </div>
                      )}

                      {sceneActionFeedback && (
                        <div
                          className={`rounded-lg px-3 py-2 text-xs ${
                            sceneActionFeedback.success
                              ? 'bg-semantic-success/20 text-semantic-success'
                              : 'bg-semantic-error/20 text-semantic-error'
                          }`}
                        >
                          {sceneActionFeedback.message}
                        </div>
                      )}
                    </>
                  ) : (
                    <div className="rounded-lg border border-DEFAULT border-dashed px-3 py-4 text-content-secondary text-sm">
                      {t('replay.noElementSelected', 'No element selected.')}
                    </div>
                  )}
                </CardContent>
              </Card>

              <div className="h-[500px]">
                <EventLog items={timeline.items} currentTime={currentTime} onItemClick={handleTimeChange} />
              </div>
            </div>
          </div>

          {/* UI note */}
          <Card>
            <CardContent className="py-3">
              <div className="grid grid-cols-2 gap-4 text-center sm:grid-cols-5">
                <div>
                  <p className="text-content-secondary text-xs">{t('replay.duration', 'session 시간')}</p>
                  <p className="font-semibold text-content text-lg">
                    {Math.round(timeline.session.duration_secs / 60)}
                    {t('dashboard.minutes', '분')}
                  </p>
                </div>
                <div>
                  <p className="text-content-secondary text-xs">{t('replay.totalEvents', '총 event')}</p>
                  <p className="font-semibold text-content text-lg">{timeline.session.total_events}</p>
                </div>
                <div>
                  <p className="text-content-secondary text-xs">{t('replay.totalFrames', '총 frame')}</p>
                  <p className="font-semibold text-content text-lg">{timeline.session.total_frames}</p>
                </div>
                <div>
                  <p className="text-content-secondary text-xs">{t('replay.totalIdle', '총 idle')}</p>
                  <p className="font-semibold text-content text-lg">
                    {Math.round(timeline.session.total_idle_secs / 60)}
                    {t('dashboard.minutes', '분')}
                  </p>
                </div>
                <div>
                  <p className="text-content-secondary text-xs">{t('replay.apps', '앱 수')}</p>
                  <p className="font-semibold text-content text-lg">
                    {new Set(timeline.segments.map((s) => s.app_name)).size}
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
