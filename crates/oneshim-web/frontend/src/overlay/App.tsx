import { useCallback } from 'react'
import CoachingPopup from './components/CoachingPopup'
import FocusHighlight from './components/FocusHighlight'
import { FocusModeIndicator } from './components/FocusModeIndicator'
import GoalProgressBar from './components/GoalProgressBar'
import HeatmapGhost from './components/HeatmapGhost'
import { SuggestionsPanel } from './components/SuggestionsPanel'
import { TrackingBorder } from './components/TrackingBorder'
import { useOverlayEvents } from './hooks/useOverlayEvents'
import type { SuggestionViewDto } from './types'

export default function OverlayApp() {
  const { state, dispatch } = useOverlayEvents()
  const isRich = state.mode === 'rich' || state.mode === 'adaptive'

  async function handleClosePanel() {
    dispatch({ type: 'toggle-suggestions-panel', payload: false })
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('toggle_overlay_interactive', { interactive: false })
  }

  const handleRefreshSuggestions = useCallback(async () => {
    const { invoke } = await import('@tauri-apps/api/core')
    try {
      const suggestions = await invoke<SuggestionViewDto[]>('get_pending_suggestions')
      dispatch({ type: 'set-suggestions', payload: suggestions })
    } catch {
      /* ignore */
    }
  }, [dispatch])

  return (
    <div className="relative h-screen w-screen overflow-hidden">
      {/* Tracking capture border indicator */}
      <TrackingBorder paused={state.captureState.paused} visible={state.captureState.indicator_visible} />

      {/* Focus mode pill indicator (top center) */}
      <FocusModeIndicator active={state.focusMode} />

      {/* Focus area highlight (always shown when available) */}
      {state.focusHighlight && <FocusHighlight highlight={state.focusHighlight} />}

      {/* Coaching popup (shown when a message is active) */}
      {state.coaching && <CoachingPopup message={state.coaching} autoDismissSecs={state.coaching.auto_dismiss_secs} />}

      {/* Suggestions panel (right side, slide in/out) */}
      <SuggestionsPanel
        open={state.suggestionsPanelOpen}
        suggestions={state.suggestions}
        onClose={handleClosePanel}
        onRefresh={handleRefreshSuggestions}
      />

      {/* Rich mode: goal progress bar at bottom */}
      {isRich && state.goals.length > 0 && <GoalProgressBar goals={state.goals} />}

      {/* Rich mode: attention heatmap ghost */}
      {isRich && <HeatmapGhost />}
    </div>
  )
}
