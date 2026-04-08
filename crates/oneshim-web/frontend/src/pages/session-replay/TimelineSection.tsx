import { Play } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { EmptyState } from '../../components/ui'
import { useTypedOutletContext } from '../../routes'
import type { ReplayOutletContext } from './ReplayLayout'
import { SceneStatusBar, SceneViewport } from './SceneOverlay'
import { FrameCard, TimelineScrubberSection } from './SessionPlayback'

export default function TimelineSection() {
  const { t } = useTranslation()
  const {
    timeline,
    playback,
    scene,
    currentScene,
    sceneFetching,
    sceneError,
    sceneCalibration,
    calibrationFetching,
    overlayAllowed,
    imageLoadFailed,
  } = useTypedOutletContext<ReplayOutletContext>('Replay')

  // Empty-state UX lives here (not in ReplayLayout) so the layout can always
  // render <Outlet> and the `/replay` → `/replay/timeline` index redirect
  // keeps firing. Matches the AuditLayout empty-state-in-child pattern.
  if (!timeline || timeline.items.length === 0) {
    return (
      <EmptyState
        icon={<Play className="h-8 w-8" />}
        title={t('emptyState.replay.title')}
        description={t('emptyState.replay.description')}
      />
    )
  }

  const { currentFrame } = playback

  return (
    <>
      {/* Timeline scrubber (full width) */}
      <TimelineScrubberSection timeline={timeline} playback={playback} />

      {/* 3-column grid: frame card (cols 1-2) + sidebar (col 3) */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        <div className="lg:col-span-2">
          <FrameCard
            playback={playback}
            viewportSlot={
              currentFrame ? (
                <SceneViewport
                  currentFrame={currentFrame}
                  imageLoadFailed={imageLoadFailed}
                  onImageLoadFailed={() => {
                    /* handled by layout */
                  }}
                  scene={scene}
                />
              ) : null
            }
            statusSlot={
              <SceneStatusBar
                currentScene={currentScene}
                sceneFetching={sceneFetching}
                sceneError={sceneError}
                sceneCalibration={sceneCalibration}
                calibrationFetching={calibrationFetching}
                overlayAllowed={overlayAllowed}
                imageLoadFailed={imageLoadFailed}
                scene={scene}
              />
            }
          />
        </div>
      </div>
    </>
  )
}
