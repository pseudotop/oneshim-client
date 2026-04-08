import { Play } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import EventLog from '../../components/EventLog'
import { EmptyState } from '../../components/ui'
import { Card, CardContent } from '../../components/ui/Card'
import { useTypedOutletContext } from '../../routes'
import { typography } from '../../styles/tokens'
import type { ReplayOutletContext } from './ReplayLayout'
import { SceneAssistantPanel } from './SceneOverlay'

export default function EventsSection() {
  const { t } = useTranslation()
  const { timeline, playback, scene, currentScene, sceneIntelligenceEnabled, sceneExecutionAllowed, sceneCalibration } =
    useTypedOutletContext<ReplayOutletContext>('Replay')

  // Mirror the EmptyState guard in TimelineSection — with no timeline data
  // there is nothing to render for events either. ReplayLayout is intentionally
  // neutral so its index redirect can always fire.
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
      {/* Sidebar content (col 3 of parent grid) */}
      <div className="space-y-4 lg:col-span-1">
        {currentFrame && (
          <SceneAssistantPanel
            currentFrame={currentFrame}
            currentScene={currentScene}
            sceneIntelligenceEnabled={sceneIntelligenceEnabled}
            sceneExecutionAllowed={sceneExecutionAllowed}
            sceneCalibration={sceneCalibration}
            scene={scene}
          />
        )}

        <div id="section-events" className="min-h-[300px] flex-1">
          <EventLog items={timeline.items} currentTime={playback.currentTime} onItemClick={playback.handleTimeChange} />
        </div>
      </div>

      {/* Session statistics footer (full width) */}
      <Card>
        <CardContent className="py-3">
          <div className="grid grid-cols-2 gap-4 text-center sm:grid-cols-5">
            <div>
              <p className="text-content-secondary text-xs">{t('replay.duration', 'session 시간')}</p>
              <p className={`${typography.weight.semibold} text-content text-lg`}>
                {Math.round(timeline.session.duration_secs / 60)}
                {t('dashboard.minutes', '분')}
              </p>
            </div>
            <div>
              <p className="text-content-secondary text-xs">{t('replay.totalEvents', '총 event')}</p>
              <p className={`${typography.weight.semibold} text-content text-lg`}>{timeline.session.total_events}</p>
            </div>
            <div>
              <p className="text-content-secondary text-xs">{t('replay.totalFrames', '총 frame')}</p>
              <p className={`${typography.weight.semibold} text-content text-lg`}>{timeline.session.total_frames}</p>
            </div>
            <div>
              <p className="text-content-secondary text-xs">{t('replay.totalIdle', '총 idle')}</p>
              <p className={`${typography.weight.semibold} text-content text-lg`}>
                {Math.round(timeline.session.total_idle_secs / 60)}
                {t('dashboard.minutes', '분')}
              </p>
            </div>
            <div>
              <p className="text-content-secondary text-xs">{t('replay.apps', '앱 수')}</p>
              <p className={`${typography.weight.semibold} text-content text-lg`}>
                {new Set(timeline.segments.map((s) => s.app_name)).size}
              </p>
            </div>
          </div>
        </CardContent>
      </Card>
    </>
  )
}
