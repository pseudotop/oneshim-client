import { useEffect, useReducer } from 'react'
<<<<<<< Updated upstream
import type { CaptureStatePayload, CoachingPayload, FocusHighlightPayload, GoalProgressItem, OverlayMode, OverlayState } from '../types'
=======
import type { CaptureStatePayload, CoachingPayload, FocusHighlightPayload, FocusModePayload, GoalProgressItem, OverlayMode, OverlayState, SuggestionViewDto } from '../types'
>>>>>>> Stashed changes

type OverlayAction =
  | { type: 'show-coaching'; payload: CoachingPayload }
  | { type: 'upgrade-message'; payload: { message_id: string; personalized_text: string } }
  | { type: 'dismiss' }
  | { type: 'update-focus'; payload: FocusHighlightPayload }
  | { type: 'clear-focus' }
  | { type: 'update-goals'; payload: GoalProgressItem[] }
  | { type: 'set-mode'; payload: OverlayMode }
  | { type: 'set-focus-mode'; payload: boolean }
  | { type: 'capture-state-changed'; payload: CaptureStatePayload }
  | { type: 'toggle-suggestions-panel'; payload?: boolean }
  | { type: 'set-suggestions'; payload: SuggestionViewDto[] }
  | { type: 'remove-suggestion'; payload: string }

const initialState: OverlayState = {
  mode: 'minimal',
  coaching: null,
  focusHighlight: null,
  focusMode: false,
  goals: [],
  captureState: { paused: false, indicator_visible: false },
  suggestionsPanelOpen: false,
  suggestions: [],
}

function reducer(state: OverlayState, action: OverlayAction): OverlayState {
  switch (action.type) {
    case 'show-coaching':
      return { ...state, coaching: action.payload }
    case 'upgrade-message':
      if (state.coaching?.message_id === action.payload.message_id) {
        return {
          ...state,
          coaching: { ...state.coaching, text: action.payload.personalized_text },
        }
      }
      return state
    case 'dismiss':
      return { ...state, coaching: null }
    case 'update-focus':
      return { ...state, focusHighlight: action.payload }
    case 'clear-focus':
      return { ...state, focusHighlight: null }
    case 'update-goals':
      return { ...state, goals: action.payload }
    case 'set-mode':
      return { ...state, mode: action.payload }
    case 'set-focus-mode':
      return { ...state, focusMode: action.payload }
    case 'capture-state-changed':
      return { ...state, captureState: action.payload }
    case 'toggle-suggestions-panel':
      return {
        ...state,
        suggestionsPanelOpen: action.payload !== undefined
          ? action.payload
          : !state.suggestionsPanelOpen,
      }
    case 'set-suggestions':
      return { ...state, suggestions: action.payload }
    case 'remove-suggestion':
      return {
        ...state,
        suggestions: state.suggestions.filter(s => s.id !== action.payload),
      }
    default:
      return state
  }
}

export function useOverlayEvents() {
  const [state, dispatch] = useReducer(reducer, initialState)

  useEffect(() => {
    let unlisten: Array<() => void> = []

    async function setup() {
      const { listen } = await import('@tauri-apps/api/event')

      const u1 = await listen<CoachingPayload>('overlay:show-coaching', (e) => {
        dispatch({ type: 'show-coaching', payload: e.payload })
      })
      const u2 = await listen<{ message_id: string; personalized_text: string }>('overlay:upgrade-message', (e) => {
        dispatch({ type: 'upgrade-message', payload: e.payload })
      })
      const u3 = await listen('overlay:dismiss', () => {
        dispatch({ type: 'dismiss' })
      })
      const u4 = await listen<FocusHighlightPayload>('overlay:update-focus', (e) => {
        dispatch({ type: 'update-focus', payload: e.payload })
      })
      const u5 = await listen<{ goals: GoalProgressItem[] }>('overlay:update-goals', (e) => {
        dispatch({ type: 'update-goals', payload: e.payload.goals })
      })
      const u6 = await listen<{ mode: OverlayMode }>('overlay:set-mode', (e) => {
        dispatch({ type: 'set-mode', payload: e.payload.mode })
      })

      const u7 = await listen('overlay:clear-focus', () => {
        dispatch({ type: 'clear-focus' })
      })

      const u8 = await listen<CaptureStatePayload>('overlay:capture-state-changed', (e) => {
        dispatch({ type: 'capture-state-changed', payload: e.payload })
      })

<<<<<<< Updated upstream
      unlisten = [u1, u2, u3, u4, u5, u6, u7, u8]
=======
      const u9 = await listen<FocusModePayload>('overlay:focus-mode', (e) => {
        dispatch({ type: 'set-focus-mode', payload: e.payload.active })
      })

      // u10: Suggestions panel toggle (from Cmd+Shift+S)
      const u10 = await listen('overlay:toggle-suggestions', () => {
        dispatch({ type: 'toggle-suggestions-panel' })
      })

      // u11: Suggestions changed — re-fetch
      const u11 = await listen<{ count: number }>('overlay:suggestions-changed', async () => {
        const { invoke } = await import('@tauri-apps/api/core')
        try {
          const suggestions = await invoke<SuggestionViewDto[]>('get_pending_suggestions')
          dispatch({ type: 'set-suggestions', payload: suggestions })
        } catch { /* ignore fetch failures */ }
      })

      unlisten = [u1, u2, u3, u4, u5, u6, u7, u8, u9, u10, u11]
>>>>>>> Stashed changes

      // Query actual backend state (overlay window may be created after state changes)
      try {
        const { invoke } = await import('@tauri-apps/api/core')
        const status = await invoke<CaptureStatePayload>('get_capture_status')
        dispatch({ type: 'capture-state-changed', payload: status })
      } catch { /* standalone/dev mode — keep defaults */ }
    }

    setup()
    return () => {
      for (const fn of unlisten) fn()
    }
  }, [])

  return { state, dispatch }
}
