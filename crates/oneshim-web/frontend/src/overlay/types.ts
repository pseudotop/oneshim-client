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

export interface FocusHighlightPayload {
  x: number
  y: number
  width: number
  height: number
  border_color: string
  opacity: number
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

export interface ModePayload {
  mode: OverlayMode
}

export interface OverlayState {
  mode: OverlayMode
  coaching: CoachingPayload | null
  focusHighlight: FocusHighlightPayload | null
  goals: GoalProgressItem[]
}
