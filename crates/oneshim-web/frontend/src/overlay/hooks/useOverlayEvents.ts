import { useEffect, useReducer } from 'react'
import type { CoachingPayload, FocusHighlightPayload, GoalProgressItem, OverlayMode, OverlayState } from '../types'

type OverlayAction =
  | { type: 'show-coaching'; payload: CoachingPayload }
  | { type: 'upgrade-message'; payload: { message_id: string; personalized_text: string } }
  | { type: 'dismiss' }
  | { type: 'update-focus'; payload: FocusHighlightPayload }
  | { type: 'clear-focus' }
  | { type: 'update-goals'; payload: GoalProgressItem[] }
  | { type: 'set-mode'; payload: OverlayMode }

const initialState: OverlayState = {
  mode: 'minimal',
  coaching: null,
  focusHighlight: null,
  goals: [],
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

      unlisten = [u1, u2, u3, u4, u5, u6, u7]
    }

    setup()
    return () => {
      for (const fn of unlisten) fn()
    }
  }, [])

  return { state, dispatch }
}
