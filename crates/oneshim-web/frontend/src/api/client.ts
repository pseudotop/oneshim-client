// API 클라이언트

const BASE_URL = '/api'

// API 요청 타임아웃 + 재시도 래퍼
const DEFAULT_TIMEOUT_MS = 10_000
const MAX_RETRIES = 2

async function fetchWithRetry(
  url: string,
  options?: RequestInit,
  retries = MAX_RETRIES
): Promise<Response> {
  const controller = new AbortController()
  const timeoutId = setTimeout(() => controller.abort(), DEFAULT_TIMEOUT_MS)

  try {
    const response = await fetch(url, {
      ...options,
      signal: controller.signal,
    })
    // 5xx 서버 에러 시 재시도
    if (response.status >= 500 && retries > 0) {
      await new Promise((r) => setTimeout(r, 1000 * (MAX_RETRIES - retries + 1)))
      return fetchWithRetry(url, options, retries - 1)
    }
    return response
  } catch (error) {
    // 타임아웃 또는 네트워크 에러 시 재시도
    if (retries > 0) {
      await new Promise((r) => setTimeout(r, 1000 * (MAX_RETRIES - retries + 1)))
      return fetchWithRetry(url, options, retries - 1)
    }
    throw error
  } finally {
    clearTimeout(timeoutId)
  }
}

// 타입 정의
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

export type UpdatePhase =
  | 'Idle'
  | 'Checking'
  | 'PendingApproval'
  | 'Installing'
  | 'Updated'
  | 'Deferred'
  | 'Error'

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

// 페이지네이션 타입
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
  /** 이벤트 타입별 가변 페이로드 데이터 (서버에서 serde_json::Value로 직렬화) */
  data: Record<string, unknown>
}

// API 함수들
export async function fetchSummary(date?: string): Promise<DailySummary> {
  const params = new URLSearchParams()
  if (date) params.set('date', date)
  const res = await fetchWithRetry(`${BASE_URL}/stats/summary?${params}`)
  if (!res.ok) throw new Error('요약 조회 실패')
  return res.json()
}

export async function fetchMetrics(from?: string, to?: string, limit = 100): Promise<SystemMetrics[]> {
  const params = new URLSearchParams()
  if (from) params.set('from', from)
  if (to) params.set('to', to)
  params.set('limit', String(limit))
  const res = await fetchWithRetry(`${BASE_URL}/metrics?${params}`)
  if (!res.ok) throw new Error('메트릭 조회 실패')
  return res.json()
}

export async function fetchHourlyMetrics(hours = 24): Promise<HourlyMetrics[]> {
  const res = await fetchWithRetry(`${BASE_URL}/metrics/hourly?hours=${hours}`)
  if (!res.ok) throw new Error('시간별 메트릭 조회 실패')
  return res.json()
}

export async function fetchProcesses(from?: string, to?: string, limit = 20): Promise<ProcessSnapshot[]> {
  const params = new URLSearchParams()
  if (from) params.set('from', from)
  if (to) params.set('to', to)
  params.set('limit', String(limit))
  const res = await fetchWithRetry(`${BASE_URL}/processes?${params}`)
  if (!res.ok) throw new Error('프로세스 조회 실패')
  return res.json()
}

export async function fetchFrames(
  from?: string,
  to?: string,
  limit = 50,
  offset = 0
): Promise<PaginatedResponse<Frame>> {
  const params = new URLSearchParams()
  if (from) params.set('from', from)
  if (to) params.set('to', to)
  params.set('limit', String(limit))
  params.set('offset', String(offset))
  const res = await fetchWithRetry(`${BASE_URL}/frames?${params}`)
  if (!res.ok) throw new Error('프레임 조회 실패')
  return res.json()
}

export async function fetchEvents(
  from?: string,
  to?: string,
  limit = 100,
  offset = 0
): Promise<PaginatedResponse<Event>> {
  const params = new URLSearchParams()
  if (from) params.set('from', from)
  if (to) params.set('to', to)
  params.set('limit', String(limit))
  params.set('offset', String(offset))
  const res = await fetchWithRetry(`${BASE_URL}/events?${params}`)
  if (!res.ok) throw new Error('이벤트 조회 실패')
  return res.json()
}

export async function fetchIdlePeriods(from?: string, to?: string): Promise<IdlePeriod[]> {
  const params = new URLSearchParams()
  if (from) params.set('from', from)
  if (to) params.set('to', to)
  const res = await fetchWithRetry(`${BASE_URL}/idle?${params}`)
  if (!res.ok) throw new Error('유휴 기간 조회 실패')
  return res.json()
}

export async function fetchSessions(): Promise<Session[]> {
  const res = await fetchWithRetry(`${BASE_URL}/sessions`)
  if (!res.ok) throw new Error('세션 조회 실패')
  return res.json()
}

export async function fetchAppUsage(date?: string): Promise<{ date: string; apps: AppUsage[] }> {
  const params = new URLSearchParams()
  if (date) params.set('date', date)
  const res = await fetchWithRetry(`${BASE_URL}/stats/apps?${params}`)
  if (!res.ok) throw new Error('앱 사용량 조회 실패')
  return res.json()
}

export async function fetchStorageStats(): Promise<StorageStats> {
  const res = await fetchWithRetry(`${BASE_URL}/storage/stats`)
  if (!res.ok) throw new Error('저장소 통계 조회 실패')
  return res.json()
}

export async function fetchSettings(): Promise<AppSettings> {
  const res = await fetchWithRetry(`${BASE_URL}/settings`)
  if (!res.ok) throw new Error('설정 조회 실패')
  return res.json()
}

export async function updateSettings(settings: AppSettings): Promise<AppSettings> {
  const res = await fetchWithRetry(`${BASE_URL}/settings`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(settings),
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: '설정 저장 실패' }))
    throw new Error(err.error || '설정 저장 실패')
  }
  return res.json()
}

export async function fetchUpdateStatus(): Promise<UpdateStatus> {
  const res = await fetchWithRetry(`${BASE_URL}/update/status`)
  if (!res.ok) throw new Error('업데이트 상태 조회 실패')
  return res.json()
}

export async function postUpdateAction(action: UpdateAction): Promise<UpdateActionResponse> {
  const res = await fetchWithRetry(`${BASE_URL}/update/action`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ action }),
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: '업데이트 작업 실행 실패' }))
    throw new Error(err.error || '업데이트 작업 실행 실패')
  }
  return res.json()
}

// 데이터 삭제 관련 타입
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

export async function deleteDataRange(request: DeleteRangeRequest): Promise<DeleteResult> {
  const res = await fetchWithRetry(`${BASE_URL}/data/range`, {
    method: 'DELETE',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(request),
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: '데이터 삭제 실패' }))
    throw new Error(err.error || '데이터 삭제 실패')
  }
  return res.json()
}

export async function deleteAllData(): Promise<DeleteResult> {
  const res = await fetchWithRetry(`${BASE_URL}/data/all`, {
    method: 'DELETE',
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: '전체 데이터 삭제 실패' }))
    throw new Error(err.error || '전체 데이터 삭제 실패')
  }
  return res.json()
}

// 검색 관련 타입
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

export async function search(params: SearchParams): Promise<SearchResponse>
export async function search(
  query: string,
  searchType?: 'all' | 'frames' | 'events',
  limit?: number,
  offset?: number,
  tagIds?: number[]
): Promise<SearchResponse>
export async function search(
  queryOrParams: string | SearchParams,
  searchType: 'all' | 'frames' | 'events' = 'all',
  limit = 50,
  offset = 0,
  tagIds?: number[]
): Promise<SearchResponse> {
  // 오버로드 처리
  let query: string
  let type: 'all' | 'frames' | 'events'
  let lim: number
  let off: number
  let tags: number[] | undefined

  if (typeof queryOrParams === 'object') {
    query = queryOrParams.query
    type = queryOrParams.searchType ?? 'all'
    lim = queryOrParams.limit ?? 50
    off = queryOrParams.offset ?? 0
    tags = queryOrParams.tagIds
  } else {
    query = queryOrParams
    type = searchType
    lim = limit
    off = offset
    tags = tagIds
  }

  const params = new URLSearchParams()
  params.set('q', query)
  params.set('search_type', type)
  params.set('limit', String(lim))
  params.set('offset', String(off))
  if (tags && tags.length > 0) {
    params.set('tag_ids', tags.join(','))
  }
  const res = await fetchWithRetry(`${BASE_URL}/search?${params}`)
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: '검색 실패' }))
    throw new Error(err.error || '검색 실패')
  }
  return res.json()
}

// 히트맵 타입
export interface HeatmapCell {
  day: number  // 0=월, 6=일
  hour: number // 0-23
  value: number
}

export interface HeatmapResponse {
  from_date: string
  to_date: string
  cells: HeatmapCell[]
  max_value: number
}

export async function fetchHeatmap(days = 7): Promise<HeatmapResponse> {
  const res = await fetchWithRetry(`${BASE_URL}/stats/heatmap?days=${days}`)
  if (!res.ok) throw new Error('히트맵 조회 실패')
  return res.json()
}

// 데이터 내보내기
export type ExportFormat = 'json' | 'csv'
export type ExportDataType = 'metrics' | 'events' | 'frames'

export async function exportData(
  dataType: ExportDataType,
  format: ExportFormat = 'json',
  from?: string,
  to?: string
): Promise<Blob> {
  const params = new URLSearchParams()
  if (from) params.set('from', from)
  if (to) params.set('to', to)
  params.set('format', format)

  const res = await fetchWithRetry(`${BASE_URL}/export/${dataType}?${params}`)
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: '내보내기 실패' }))
    throw new Error(err.error || '내보내기 실패')
  }
  return res.blob()
}

export function downloadBlob(blob: Blob, filename: string): void {
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  document.body.appendChild(a)
  a.click()
  document.body.removeChild(a)
  URL.revokeObjectURL(url)
}

// ============================================================
// 태그 관련 타입 및 함수
// ============================================================

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

/** 모든 태그 목록 조회 */
export async function fetchTags(): Promise<Tag[]> {
  const res = await fetchWithRetry(`${BASE_URL}/tags`)
  if (!res.ok) throw new Error('태그 조회 실패')
  return res.json()
}

/** 태그 생성 */
export async function createTag(request: CreateTagRequest): Promise<Tag> {
  const res = await fetchWithRetry(`${BASE_URL}/tags`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(request),
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: '태그 생성 실패' }))
    throw new Error(err.error || '태그 생성 실패')
  }
  return res.json()
}

/** 태그 업데이트 */
export async function updateTag(tagId: number, request: UpdateTagRequest): Promise<Tag> {
  const res = await fetchWithRetry(`${BASE_URL}/tags/${tagId}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(request),
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: '태그 수정 실패' }))
    throw new Error(err.error || '태그 수정 실패')
  }
  return res.json()
}

/** 태그 삭제 */
export async function deleteTag(tagId: number): Promise<void> {
  const res = await fetchWithRetry(`${BASE_URL}/tags/${tagId}`, {
    method: 'DELETE',
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: '태그 삭제 실패' }))
    throw new Error(err.error || '태그 삭제 실패')
  }
}

/** 프레임의 태그 조회 */
export async function fetchFrameTags(frameId: number): Promise<Tag[]> {
  const res = await fetchWithRetry(`${BASE_URL}/frames/${frameId}/tags`)
  if (!res.ok) throw new Error('프레임 태그 조회 실패')
  return res.json()
}

/** 프레임에 태그 추가 */
export async function addTagToFrame(frameId: number, tagId: number): Promise<void> {
  const res = await fetchWithRetry(`${BASE_URL}/frames/${frameId}/tags/${tagId}`, {
    method: 'POST',
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: '태그 추가 실패' }))
    throw new Error(err.error || '태그 추가 실패')
  }
}

/** 프레임에서 태그 제거 */
export async function removeTagFromFrame(frameId: number, tagId: number): Promise<void> {
  const res = await fetchWithRetry(`${BASE_URL}/frames/${frameId}/tags/${tagId}`, {
    method: 'DELETE',
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: '태그 제거 실패' }))
    throw new Error(err.error || '태그 제거 실패')
  }
}

// ============================================================
// 리포트 관련 타입 및 함수
// ============================================================

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

/** 활동 리포트 조회 */
export async function fetchReport(params: ReportParams): Promise<ReportResponse> {
  const searchParams = new URLSearchParams()
  searchParams.set('period', params.period)
  if (params.from) searchParams.set('from', params.from)
  if (params.to) searchParams.set('to', params.to)

  const res = await fetchWithRetry(`${BASE_URL}/reports?${searchParams}`)
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: '리포트 조회 실패' }))
    throw new Error(err.error || '리포트 조회 실패')
  }
  return res.json()
}

// ============================================================
// 백업/복원 관련 타입 및 함수
// ============================================================

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

/** 설정 백업 데이터 (서버 SettingsBackup 구조) */
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

/** 태그 백업 데이터 (서버 TagBackup 구조) */
export interface TagBackup {
  id: number
  name: string
  color: string
  created_at: string
}

/** 프레임-태그 연결 백업 데이터 (서버 FrameTagBackup 구조) */
export interface FrameTagBackup {
  frame_id: number
  tag_id: number
  created_at: string
}

/** 이벤트 백업 데이터 (서버 EventBackup 구조) */
export interface EventBackup {
  event_id: string
  event_type: string
  timestamp: string
  app_name: string | null
  window_title: string | null
}

/** 프레임 메타데이터 백업 (이미지 제외, 서버 FrameBackup 구조) */
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

/** 백업 다운로드 */
export async function downloadBackup(params: BackupParams = {}): Promise<Blob> {
  const searchParams = new URLSearchParams()
  if (params.include_settings !== undefined) {
    searchParams.set('include_settings', String(params.include_settings))
  }
  if (params.include_tags !== undefined) {
    searchParams.set('include_tags', String(params.include_tags))
  }
  if (params.include_events !== undefined) {
    searchParams.set('include_events', String(params.include_events))
  }
  if (params.include_frames !== undefined) {
    searchParams.set('include_frames', String(params.include_frames))
  }

  const res = await fetchWithRetry(`${BASE_URL}/backup?${searchParams}`)
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: '백업 생성 실패' }))
    throw new Error(err.error || '백업 생성 실패')
  }
  return res.blob()
}

/** 백업 복원 */
export async function restoreBackup(archive: BackupArchive): Promise<RestoreResult> {
  const res = await fetchWithRetry(`${BASE_URL}/backup/restore`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(archive),
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: '복원 실패' }))
    throw new Error(err.error || '복원 실패')
  }
  return res.json()
}

// ============================================================
// 통합 타임라인 관련 타입 및 함수 (세션 리플레이)
// ============================================================

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
  | { type: 'Frame'; id: number; timestamp: string; app_name: string; window_title: string; importance: number; image_url: string }
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

/** 통합 타임라인 조회 (세션 리플레이) */
export async function fetchTimeline(params: TimelineParams = {}): Promise<TimelineResponse> {
  const searchParams = new URLSearchParams()
  if (params.from) searchParams.set('from', params.from)
  if (params.to) searchParams.set('to', params.to)
  if (params.max_events) searchParams.set('max_events', String(params.max_events))
  if (params.max_frames) searchParams.set('max_frames', String(params.max_frames))

  const res = await fetchWithRetry(`${BASE_URL}/timeline?${searchParams}`)
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: '타임라인 조회 실패' }))
    throw new Error(err.error || '타임라인 조회 실패')
  }
  return res.json()
}

// ============================================================
// Edge Intelligence (집중도) 관련 타입 및 함수
// ============================================================

/** 집중도 메트릭 */
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

/** 집중도 메트릭 응답 */
export interface FocusMetricsResponse {
  today: FocusMetrics
  history: FocusMetrics[]
}

/** 작업 세션 */
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

/** 인터럽션 */
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

/** 로컬 제안 */
export interface LocalSuggestion {
  id: number
  suggestion_type: string
  /** 제안 유형별 가변 콘텐츠 (SQLite TEXT → JSON 파싱, 구조는 suggestion_type에 따라 다름) */
  payload: Record<string, unknown>
  created_at: string
  shown_at: string | null
  dismissed_at: string | null
  acted_at: string | null
}

/** 제안 피드백 액션 */
export type SuggestionFeedbackAction = 'shown' | 'dismissed' | 'acted'

/** 집중도 메트릭 조회 (오늘 + 최근 7일) */
export async function fetchFocusMetrics(): Promise<FocusMetricsResponse> {
  const res = await fetchWithRetry(`${BASE_URL}/focus/metrics`)
  if (!res.ok) throw new Error('집중도 메트릭 조회 실패')
  return res.json()
}

/** 작업 세션 목록 조회 */
export async function fetchWorkSessions(from?: string, to?: string, limit = 100): Promise<WorkSession[]> {
  const params = new URLSearchParams()
  if (from) params.set('from', from)
  if (to) params.set('to', to)
  params.set('limit', String(limit))
  const res = await fetchWithRetry(`${BASE_URL}/focus/sessions?${params}`)
  if (!res.ok) throw new Error('작업 세션 조회 실패')
  return res.json()
}

/** 인터럽션 목록 조회 */
export async function fetchInterruptions(from?: string, to?: string, limit = 100): Promise<Interruption[]> {
  const params = new URLSearchParams()
  if (from) params.set('from', from)
  if (to) params.set('to', to)
  params.set('limit', String(limit))
  const res = await fetchWithRetry(`${BASE_URL}/focus/interruptions?${params}`)
  if (!res.ok) throw new Error('인터럽션 조회 실패')
  return res.json()
}

/** 로컬 제안 목록 조회 */
export async function fetchLocalSuggestions(): Promise<LocalSuggestion[]> {
  const res = await fetchWithRetry(`${BASE_URL}/focus/suggestions`)
  if (!res.ok) throw new Error('로컬 제안 조회 실패')
  return res.json()
}

/** 제안 피드백 제출 */
export async function submitSuggestionFeedback(
  id: number,
  action: SuggestionFeedbackAction
): Promise<void> {
  const res = await fetchWithRetry(`${BASE_URL}/focus/suggestions/${id}/feedback`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ action }),
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: '피드백 제출 실패' }))
    throw new Error(err.error || '피드백 제출 실패')
  }
}

// ============================================================
// 자동화 관련 타입 및 함수
// ============================================================

/** Settings에 포함되는 자동화 관련 설정 */
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
  ocr_provider: string
  llm_provider: string
  external_data_policy: string
  fallback_to_local: boolean
  ocr_api: ExternalApiSettings | null
  llm_api: ExternalApiSettings | null
}

export interface ExternalApiSettings {
  endpoint: string
  api_key_masked: string
  model: string | null
  timeout_secs: number
}

/** 자동화 시스템 상태 */
export interface AutomationStatus {
  enabled: boolean
  sandbox_enabled: boolean
  sandbox_profile: string
  ocr_provider: string
  llm_provider: string
  external_data_policy: string
  pending_audit_entries: number
}

/** 감사 로그 항목 */
export interface AuditEntry {
  entry_id: string
  timestamp: string
  session_id: string
  command_id: string
  action_type: string
  status: string
  details: string | null
  elapsed_ms: number | null
}

/** 실행 통계 */
export interface AutomationStats {
  total_executions: number
  successful: number
  failed: number
  denied: number
  timeout: number
  avg_elapsed_ms: number
}

/** 정책 정보 */
export interface PoliciesInfo {
  automation_enabled: boolean
  sandbox_profile: string
  sandbox_enabled: boolean
  allow_network: boolean
  external_data_policy: string
}

/** 자동화 인텐트 정의 (서버 AutomationIntent JSON — variant별로 구조가 다름) */
export type IntentDefinition = Record<string, unknown>

/** 워크플로우 프리셋 */
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

/** 프리셋 실행 결과 */
export interface PresetRunResult {
  preset_id: string
  success: boolean
  message: string
  steps_executed?: number
  total_steps?: number
  total_elapsed_ms?: number
}

/** 자동화 상태 조회 */
export async function fetchAutomationStatus(): Promise<AutomationStatus> {
  const res = await fetchWithRetry(`${BASE_URL}/automation/status`)
  if (!res.ok) throw new Error('자동화 상태 조회 실패')
  return res.json()
}

/** 감사 로그 조회 */
export async function fetchAuditLogs(limit = 50, status?: string): Promise<AuditEntry[]> {
  const params = new URLSearchParams()
  params.set('limit', String(limit))
  if (status) params.set('status', status)
  const res = await fetchWithRetry(`${BASE_URL}/automation/audit?${params}`)
  if (!res.ok) throw new Error('감사 로그 조회 실패')
  return res.json()
}

/** 정책 정보 조회 */
export async function fetchPolicies(): Promise<PoliciesInfo> {
  const res = await fetchWithRetry(`${BASE_URL}/automation/policies`)
  if (!res.ok) throw new Error('정책 조회 실패')
  return res.json()
}

/** 실행 통계 조회 */
export async function fetchAutomationStats(): Promise<AutomationStats> {
  const res = await fetchWithRetry(`${BASE_URL}/automation/stats`)
  if (!res.ok) throw new Error('실행 통계 조회 실패')
  return res.json()
}

/** 프리셋 목록 조회 */
export async function fetchPresets(): Promise<{ presets: WorkflowPreset[] }> {
  const res = await fetchWithRetry(`${BASE_URL}/automation/presets`)
  if (!res.ok) throw new Error('프리셋 조회 실패')
  return res.json()
}

/** 프리셋 생성 */
export async function createPreset(preset: WorkflowPreset): Promise<WorkflowPreset> {
  const res = await fetchWithRetry(`${BASE_URL}/automation/presets`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(preset),
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: '프리셋 생성 실패' }))
    throw new Error(err.error || '프리셋 생성 실패')
  }
  return res.json()
}

/** 프리셋 삭제 */
export async function deletePreset(id: string): Promise<void> {
  const res = await fetchWithRetry(`${BASE_URL}/automation/presets/${id}`, {
    method: 'DELETE',
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: '프리셋 삭제 실패' }))
    throw new Error(err.error || '프리셋 삭제 실패')
  }
}

/** 프리셋 실행 */
export async function runPreset(id: string): Promise<PresetRunResult> {
  const res = await fetchWithRetry(`${BASE_URL}/automation/presets/${id}/run`, {
    method: 'POST',
  })
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: '프리셋 실행 실패' }))
    throw new Error(err.error || '프리셋 실행 실패')
  }
  return res.json()
}
