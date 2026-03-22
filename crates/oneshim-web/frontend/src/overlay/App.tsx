import CoachingPopup from './components/CoachingPopup'
import FocusHighlight from './components/FocusHighlight'
import GoalProgressBar from './components/GoalProgressBar'
import HeatmapGhost from './components/HeatmapGhost'
import { useOverlayEvents } from './hooks/useOverlayEvents'

export default function OverlayApp() {
  const { state } = useOverlayEvents()
  const isRich = state.mode === 'rich' || state.mode === 'adaptive'

  return (
    <div className="relative h-screen w-screen overflow-hidden">
      {/* Focus area highlight (always shown when available) */}
      {state.focusHighlight && <FocusHighlight highlight={state.focusHighlight} />}

      {/* Coaching popup (shown when a message is active) */}
      {state.coaching && <CoachingPopup message={state.coaching} autoDismissSecs={state.coaching.auto_dismiss_secs} />}

      {/* Rich mode: goal progress bar at bottom */}
      {isRich && state.goals.length > 0 && <GoalProgressBar goals={state.goals} />}

      {/* Rich mode: attention heatmap ghost */}
      {isRich && <HeatmapGhost />}
    </div>
  )
}
