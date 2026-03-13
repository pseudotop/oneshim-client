// Generated from api/client.ts declarations. Keep this file type-only.
// Source of truth for frontend API transport contracts.
export interface DailySummary {
  date: string
  total_active_secs: number
  total_idle_secs: number
  top_apps: AppUsage[]
  cpu_avg: number
  memory_avg_percent: number
  frames_captured: number
  events_logged: number
}

export interface AppUsage {
  name: string
  duration_secs: number
  event_count: number
  frame_count: number
}

export interface SystemMetrics {
  timestamp: string
  cpu_usage: number
  memory_used: number
  memory_total: number
  memory_percent: number
  disk_used: number
  disk_total: number
  network_upload: number
  network_download: number
}

export interface HourlyMetrics {
  hour: string
  cpu_avg: number
  cpu_max: number
  memory_avg: number
  memory_max: number
  sample_count: number
}

export interface ProcessSnapshot {
  timestamp: string
  processes: ProcessEntry[]
}

export interface ProcessEntry {
  pid: number
  name: string
  cpu_usage: number
  memory_bytes: number
}

export interface Frame {
  id: number
  timestamp: string
  trigger_type: string
  app_name: string
  window_title: string
  importance: number
  resolution: string
  file_path: string | null
  ocr_text: string | null
  image_url: string | null
  tag_ids: number[]
}

export interface IdlePeriod {
  start_time: string
  end_time: string | null
  duration_secs: number | null
}

export interface Session {
  session_id: string
  started_at: string
  ended_at: string | null
  total_events: number
  total_frames: number
  total_idle_secs: number
  active_duration_secs: number | null
}

export interface StorageStats {
  db_size_bytes: number
  frames_size_bytes: number
  total_size_bytes: number
  frame_count: number
  event_count: number
  metric_count: number
  oldest_data_date: string | null
  newest_data_date: string | null
}

export interface NotificationSettings {
  enabled: boolean
  idle_notification: boolean
  idle_notification_mins: number
  long_session_notification: boolean
  long_session_mins: number
  high_usage_notification: boolean
  high_usage_threshold: number
}

export interface TelemetrySettings {
  enabled: boolean
  crash_reports: boolean
  usage_analytics: boolean
  performance_metrics: boolean
}

export interface MonitorControlSettings {
  process_monitoring: boolean
  input_activity: boolean
  privacy_mode: boolean
}

export interface PrivacySettings {
  excluded_apps: string[]
  excluded_app_patterns: string[]
  excluded_title_patterns: string[]
  auto_exclude_sensitive: boolean
  pii_filter_level: string
}

export interface ScheduleSettings {
  active_hours_enabled: boolean
  active_start_hour: number
  active_end_hour: number
  active_days: string[]
  pause_on_screen_lock: boolean
  pause_on_battery_saver: boolean
}

export interface UpdateSettings {
  enabled: boolean
  check_interval_hours: number
  include_prerelease: boolean
  auto_install: boolean
}

export interface AppSettings {
  retention_days: number
  max_storage_mb: number
  web_port: number
  allow_external: boolean
  capture_enabled: boolean
  idle_threshold_secs: number
  metrics_interval_secs: number
  process_interval_secs: number
  notification: NotificationSettings
  update: UpdateSettings
  telemetry: TelemetrySettings
  monitor: MonitorControlSettings
  privacy: PrivacySettings
  schedule: ScheduleSettings
  automation: AutomationSettings
  sandbox: SandboxSettings
  ai_provider: AiProviderSettings
}

export type UpdatePhase = 'Idle' | 'Checking' | 'PendingApproval' | 'Installing' | 'Updated' | 'Deferred' | 'Error'

export interface PendingUpdateInfo {
  current_version: string
  latest_version: string
  release_url: string
  release_name: string | null
  published_at: string | null
  download_url: string
}

export interface UpdateStatus {
  enabled: boolean
  auto_install: boolean
  phase: UpdatePhase
  message: string | null
  pending: PendingUpdateInfo | null
  revision: number
  updated_at: string
}

export interface UpdateActionResponse {
  accepted: boolean
  status: UpdateStatus
}

export type UpdateAction = 'Approve' | 'Defer' | 'CheckNow'

export interface PaginationMeta {
  total: number
  offset: number
  limit: number
  has_more: boolean
}

export interface PaginatedResponse<T> {
  data: T[]
  pagination: PaginationMeta
}

export interface Event {
  event_id: string
  event_type: string
  timestamp: string
  app_name: string | null
  window_title: string | null
  data: Record<string, unknown>
}

export interface ProviderModelsRequest {
  provider_type: string
  api_key: string
  endpoint?: string | null
}

export interface ProviderModelsResponse {
  models: string[]
  notice?: string | null
}

export interface ProviderPreset {
  provider_type: string
  aliases: string[]
  display_name: string
  llm_endpoint: string
  ocr_endpoint: string
  model_catalog_endpoint: string
  ocr_model_catalog_supported: boolean
  ocr_model_catalog_notice?: string | null
  llm_models: string[]
  ocr_models: string[]
}

export interface ProviderPresetCatalog {
  version: number
  updated_at: string
  providers: ProviderPreset[]
}

export interface DeleteRangeRequest {
  from: string
  to: string
  data_types?: string[]
}

export interface DeleteResult {
  success: boolean
  events_deleted: number
  frames_deleted: number
  metrics_deleted: number
  process_snapshots_deleted: number
  idle_periods_deleted: number
  message: string
}

export interface SearchTagInfo {
  id: number
  name: string
  color: string
}

export interface SearchResult {
  result_type: 'frame' | 'event'
  id: string
  timestamp: string
  app_name: string | null
  window_title: string | null
  matched_text: string | null
  image_url: string | null
  importance: number | null
  tags?: SearchTagInfo[]
}

export interface SearchResponse {
  query: string
  total: number
  offset: number
  limit: number
  results: SearchResult[]
}

export interface SearchParams {
  query: string
  searchType?: 'all' | 'frames' | 'events'
  tagIds?: number[]
  limit?: number
  offset?: number
}

export interface HeatmapCell {
  day: number // 0=Mon, 6=Sun
  hour: number // 0-23
  value: number
}

export interface HeatmapResponse {
  from_date: string
  to_date: string
  cells: HeatmapCell[]
  max_value: number
}

export type ExportFormat = 'json' | 'csv'

export type ExportDataType = 'metrics' | 'events' | 'frames'

export interface Tag {
  id: number
  name: string
  color: string
  created_at: string
}

export interface CreateTagRequest {
  name: string
  color?: string
}

export interface UpdateTagRequest {
  name: string
  color: string
}

export type ReportPeriod = 'week' | 'month' | 'custom'

export interface ReportDailyStat {
  date: string
  active_secs: number
  idle_secs: number
  captures: number
  events: number
  cpu_avg: number
  memory_avg: number
}

export interface ReportAppStat {
  name: string
  duration_secs: number
  events: number
  captures: number
  percentage: number
}

export interface ReportHourlyActivity {
  hour: number
  activity: number
}

export interface ReportProductivity {
  score: number
  active_ratio: number
  peak_hour: number
  top_app: string
  trend: number
}

export interface ReportResponse {
  title: string
  from_date: string
  to_date: string
  days: number
  total_active_secs: number
  total_idle_secs: number
  total_captures: number
  total_events: number
  avg_cpu: number
  avg_memory: number
  daily_stats: ReportDailyStat[]
  app_stats: ReportAppStat[]
  hourly_activity: ReportHourlyActivity[]
  productivity: ReportProductivity
}

export interface ReportParams {
  period: ReportPeriod
  from?: string
  to?: string
}

export interface BackupMetadata {
  version: string
  created_at: string
  app_version: string
  includes: {
    settings: boolean
    tags: boolean
    events: boolean
    frames: boolean
  }
}

export interface SettingsBackup {
  capture_enabled: boolean
  capture_interval_secs: number
  idle_threshold_secs: number
  metrics_interval_secs: number
  web_port: number
  notification_enabled: boolean
  idle_notification_mins: number
  long_session_notification_mins: number
  high_usage_threshold_percent: number
}

export interface TagBackup {
  id: number
  name: string
  color: string
  created_at: string
}

export interface FrameTagBackup {
  frame_id: number
  tag_id: number
  created_at: string
}

export interface EventBackup {
  event_id: string
  event_type: string
  timestamp: string
  app_name: string | null
  window_title: string | null
}

export interface FrameBackup {
  id: number
  timestamp: string
  trigger_type: string
  app_name: string
  window_title: string
  importance: number
  width: number
  height: number
  ocr_text: string | null
}

export interface BackupArchive {
  metadata: BackupMetadata
  settings?: SettingsBackup
  tags?: TagBackup[]
  frame_tags?: FrameTagBackup[]
  events?: EventBackup[]
  frames?: FrameBackup[]
}

export interface BackupParams {
  include_settings?: boolean
  include_tags?: boolean
  include_events?: boolean
  include_frames?: boolean
}

export interface RestoreResult {
  success: boolean
  restored: {
    settings: boolean
    tags: number
    frame_tags: number
    events: number
    frames: number
  }
  errors: string[]
}

export interface TimelineSessionInfo {
  start: string
  end: string
  duration_secs: number
  total_events: number
  total_frames: number
  total_idle_secs: number
}

export type TimelineItem =
  | { type: 'Event'; id: string; timestamp: string; event_type: string; app_name?: string; window_title?: string }
  | {
      type: 'Frame'
      id: number
      timestamp: string
      app_name: string
      window_title: string
      importance: number
      image_url: string
    }
  | { type: 'IdlePeriod'; start: string; end: string; duration_secs: number }

export interface AppSegment {
  app_name: string
  start: string
  end: string
  color: string
}

export interface TimelineResponse {
  session: TimelineSessionInfo
  items: TimelineItem[]
  segments: AppSegment[]
}

export interface TimelineParams {
  from?: string
  to?: string
  max_events?: number
  max_frames?: number
}

export interface FocusMetrics {
  date: string
  total_active_secs: number
  deep_work_secs: number
  communication_secs: number
  context_switches: number
  interruption_count: number
  avg_focus_duration_secs: number
  max_focus_duration_secs: number
  focus_score: number
}

export interface FocusMetricsResponse {
  today: FocusMetrics
  history: FocusMetrics[]
}

export interface WorkSession {
  id: number
  started_at: string
  ended_at: string | null
  primary_app: string
  category: string
  state: string
  interruption_count: number
  deep_work_secs: number
  duration_secs: number
}

export interface Interruption {
  id: number
  interrupted_at: string
  from_app: string
  from_category: string
  to_app: string
  to_category: string
  resumed_at: string | null
  resumed_to_app: string | null
  duration_secs: number | null
}

export interface LocalSuggestion {
  id: number
  suggestion_type: string
  payload: Record<string, unknown>
  created_at: string
  shown_at: string | null
  dismissed_at: string | null
  acted_at: string | null
}

export type SuggestionFeedbackAction = 'shown' | 'dismissed' | 'acted'

export interface AutomationSettings {
  enabled: boolean
}

export interface SandboxSettings {
  enabled: boolean
  profile: string
  allowed_read_paths: string[]
  allowed_write_paths: string[]
  allow_network: boolean
  max_memory_bytes: number
  max_cpu_time_ms: number
}

export interface AiProviderSettings {
  access_mode: string
  ocr_provider: string
  llm_provider: string
  external_data_policy: string
  allow_unredacted_external_ocr: boolean
  ocr_validation: OcrValidationSettings
  scene_action_override: SceneActionOverrideSettings
  scene_intelligence: SceneIntelligenceSettings
  fallback_to_local: boolean
  ocr_api: ExternalApiSettings | null
  llm_api: ExternalApiSettings | null
}

export interface SceneActionOverrideSettings {
  enabled: boolean
  reason: string
  approved_by: string
  expires_at: string | null
}

export interface OcrValidationSettings {
  enabled: boolean
  min_confidence: number
  max_invalid_ratio: number
}

export interface SceneIntelligenceSettings {
  enabled: boolean
  overlay_enabled: boolean
  allow_action_execution: boolean
  min_confidence: number
  max_elements: number
  calibration_enabled: boolean
  calibration_min_elements: number
  calibration_min_avg_confidence: number
}

export interface ExternalApiSettings {
  endpoint: string
  api_key_masked: string
  model: string | null
  provider_type: string
  timeout_secs: number
}

export interface AutomationStatus {
  enabled: boolean
  sandbox_enabled: boolean
  sandbox_profile: string
  ocr_provider: string
  llm_provider: string
  ocr_source: string
  llm_source: string
  ocr_fallback_reason: string | null
  llm_fallback_reason: string | null
  external_data_policy: string
  pending_audit_entries: number
}

export interface AuditEntry {
  schema_version: string
  entry_id: string
  timestamp: string
  session_id: string
  command_id: string
  action_type: string
  status: string
  details: string | null
  elapsed_ms: number | null
}

export interface AutomationStats {
  total_executions: number
  successful: number
  failed: number
  denied: number
  timeout: number
  avg_elapsed_ms: number
  success_rate: number
  blocked_rate: number
  p95_elapsed_ms: number
  timing_samples: number
}

export interface PoliciesInfo {
  automation_enabled: boolean
  sandbox_profile: string
  sandbox_enabled: boolean
  allow_network: boolean
  external_data_policy: string
  scene_action_override_enabled: boolean
  scene_action_override_active: boolean
  scene_action_override_reason: string | null
  scene_action_override_approved_by: string | null
  scene_action_override_expires_at: string | null
  scene_action_override_issue: string | null
}

export interface AutomationContracts {
  audit_schema_version: string
  scene_schema_version: string
  scene_action_schema_version: string
}

export type IntentDefinition = Record<string, unknown>

export interface WorkflowPreset {
  id: string
  name: string
  description: string
  category: string
  steps: WorkflowStep[]
  builtin: boolean
  platform: string | null
}

export interface WorkflowStep {
  name: string
  intent: IntentDefinition
  delay_ms: number
  stop_on_failure: boolean
}

export interface PresetRunResult {
  preset_id: string
  success: boolean
  message: string
  steps_executed?: number
  total_steps?: number
  total_elapsed_ms?: number
}

export interface ExecuteIntentHintRequest {
  command_id?: string
  session_id: string
  intent_hint: string
}

export interface ExecuteIntentHintResponse {
  command_id: string
  session_id: string
  planned_intent: IntentDefinition
  result: {
    success: boolean
    element: unknown | null
    verification: unknown | null
    retry_count: number
    elapsed_ms: number
    error: string | null
  }
}

export type SceneActionType = 'click' | 'type_text'

export interface ExecuteSceneActionRequest {
  command_id?: string
  session_id: string
  frame_id?: number
  scene_id?: string
  element_id: string
  action_type: SceneActionType
  bbox_abs: UiSceneBounds
  role?: string | null
  label?: string | null
  text?: string | null
  allow_sensitive_input?: boolean
}

export interface ExecuteSceneActionResponse {
  schema_version: string
  command_id: string
  session_id: string
  frame_id?: number
  scene_id?: string
  element_id: string
  applied_privacy_policy: string
  scene_action_override_active: boolean
  scene_action_override_expires_at?: string | null
  executed_intents: IntentDefinition[]
  result: {
    success: boolean
    element: unknown | null
    verification: unknown | null
    retry_count: number
    elapsed_ms: number
    error: string | null
  }
}

export interface UiSceneBounds {
  x: number
  y: number
  width: number
  height: number
}

export interface UiSceneElement {
  element_id: string
  bbox_abs: UiSceneBounds
  bbox_norm: UiSceneBounds
  label: string
  role: string | null
  intent: string | null
  state: string | null
  confidence: number
  text_masked: string | null
  parent_id: string | null
}

export interface UiScene {
  schema_version: string
  scene_id: string
  app_name: string | null
  screen_id: string | null
  captured_at: string
  screen_width: number
  screen_height: number
  elements: UiSceneElement[]
}

export interface SceneCalibrationReport {
  schema_version: string
  scene_id: string
  total_elements: number
  considered_elements: number
  avg_confidence: number
  min_confidence: number
  min_required_elements: number
  min_required_avg_confidence: number
  passed: boolean
  reasons: string[]
}
