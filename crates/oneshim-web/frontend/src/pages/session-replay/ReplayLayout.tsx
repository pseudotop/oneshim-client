import { useQuery } from '@tanstack/react-query'
import { AlertCircle } from 'lucide-react'
import { useCallback, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Outlet } from 'react-router-dom'
import { fetchAutomationScene, fetchSceneCalibration, fetchSettings, fetchTimeline } from '../../api/client'
import type { TimelineResponse, UiScene } from '../../api/contracts'
import DateRangePicker from '../../components/DateRangePicker'
import { Alert } from '../../components/ui'
import { Spinner } from '../../components/ui/Spinner'
import { colors, iconSize, typography } from '../../styles/tokens'
import { cn } from '../../utils/cn'
import { usePlaybackState } from './hooks/usePlaybackState'
import type { SceneState } from './SceneOverlay'
import { useSceneState } from './SceneOverlay'
import type { PlaybackState } from './types'

export interface ReplayOutletContext {
  /**
   * Nullable so the defaultChild (TimelineSection) can own the empty-state
   * UX and this layout can always render <Outlet>, letting RouteRenderer's
   * `/replay` → `/replay/timeline` index redirect fire even when no session
   * has been captured yet. Same empty-state-in-child pattern AuditLayout
   * adopted after the 2026-04-08 routing.spec regression.
   */
  timeline: TimelineResponse | null
  playback: PlaybackState
  scene: SceneState
  currentScene: UiScene | undefined
  sceneFetching: boolean
  sceneError: Error | null
  sceneCalibration: { passed?: boolean; reasons?: string[] } | undefined
  calibrationFetching: boolean
  sceneIntelligenceEnabled: boolean
  overlayAllowed: boolean
  sceneExecutionAllowed: boolean
  imageLoadFailed: boolean
  onImageLoadFailed: () => void
}

export default function ReplayLayout() {
  const { t } = useTranslation()

  const [fromDate, setFromDate] = useState(() => {
    const d = new Date()
    d.setHours(d.getHours() - 1) // default to 1 hour ago
    return d.toISOString().slice(0, 16)
  })
  const [toDate, setToDate] = useState(() => new Date().toISOString().slice(0, 16))

  const {
    data: timelineData,
    isLoading: loading,
    error: timelineError,
  } = useQuery({
    queryKey: ['timeline', fromDate, toDate],
    queryFn: () => fetchTimeline({ from: new Date(fromDate).toISOString(), to: new Date(toDate).toISOString() }),
  })
  const timeline = timelineData ?? null
  const error = timelineError
    ? timelineError instanceof Error
      ? timelineError.message
      : '타임라인 로드 failure'
    : null

  // usePlaybackState's signature accepts `TimelineData | undefined`; pass the
  // raw query data so the hook's own undefined checks handle the loading/
  // empty states without a null-coercion dance.
  const playback = usePlaybackState(timelineData)
  const { currentFrame } = playback

  const [imageLoadFailed, setImageLoadFailed] = useState(false)

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

  const scene = useSceneState({
    currentFrame,
    currentScene,
    sceneFetching,
    sceneError,
    sceneCalibration,
    calibrationFetching,
    sceneIntelligenceEnabled,
    overlayAllowed,
    sceneExecutionAllowed,
    imageLoadFailed,
    onImageLoadFailed: () => setImageLoadFailed(true),
  })

  const handleDateRangeChange = useCallback((from: string | undefined, to: string | undefined) => {
    if (from) setFromDate(from)
    if (to) setToDate(to)
  }, [])

  return (
    <div className="min-h-full space-y-4 p-6">
      {/* Header + date range */}
      <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <h1 className={cn(typography.h1, colors.text.pageTitle)}>{t('replay.title', 'session 리플레이')}</h1>
        <DateRangePicker
          onRangeChange={handleDateRangeChange}
          initialFrom={fromDate.split('T')[0]}
          initialTo={toDate.split('T')[0]}
        />
      </div>

      {/* Error state */}
      {error && (
        <Alert variant="error" icon={<AlertCircle className={iconSize.md} />}>
          {error}
        </Alert>
      )}

      {/* Loading state */}
      {loading && (
        <div className="flex items-center justify-center py-12">
          <Spinner />
        </div>
      )}

      {/* Main content — Outlet is rendered unconditionally (post-loading) so
          that the `/replay` → `/replay/timeline` index redirect always fires.
          TimelineSection (the defaultChild) owns the empty state when
          timeline is null or has zero items. */}
      {!loading && (
        <Outlet
          context={
            {
              timeline,
              playback,
              scene,
              currentScene,
              sceneFetching,
              sceneError,
              sceneCalibration,
              calibrationFetching,
              sceneIntelligenceEnabled,
              overlayAllowed,
              sceneExecutionAllowed,
              imageLoadFailed,
              onImageLoadFailed: () => setImageLoadFailed(true),
            } satisfies ReplayOutletContext
          }
        />
      )}
    </div>
  )
}
