export type OverlayMode = 'minimal' | 'rich' | 'adaptive'
export type DismissAction = 'ok' | 'later' | 'timeout'

export interface CoachingPayload {
  message_id: string
  profile: string
  trigger_type: string
  text: string
  auto_dismiss_secs: number
}

export interface UpgradePayload {
  message_id: string
  personalized_text: string
}

export interface FocusHighlightTarget {
  candidate_id: string
  x: number
  y: number
  width: number
  height: number
  color: string
  label: string | null
}

export interface FocusHighlightPayload {
  handle_id: string
  targets: FocusHighlightTarget[]
}

export interface GoalProgressItem {
  regime_label: string
  current_minutes: number
  target_minutes: number
  percentage: number
  display_color: string
}

export interface GoalPayload {
  goals: GoalProgressItem[]
}

export interface CaptureStatePayload {
  paused: boolean
  indicator_visible: boolean
}

export interface ModePayload {
  mode: OverlayMode
}

<<<<<<< Updated upstream
=======
export interface FocusModePayload {
  active: boolean
}

export interface SuggestionViewDto {
  id: string
  title: string
  body: string
  priority: string
  category: string | null
  source: string
  created_at: string
  is_read: boolean
}

>>>>>>> Stashed changes
export interface OverlayState {
  mode: OverlayMode
  coaching: CoachingPayload | null
  focusHighlight: FocusHighlightPayload | null
  focusMode: boolean
  goals: GoalProgressItem[]
  captureState: CaptureStatePayload
  suggestionsPanelOpen: boolean
  suggestions: SuggestionViewDto[]
}
