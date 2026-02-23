import type {
  AppSettings,
  AutomationSettings,
  AutomationStats,
  AutomationStatus,
  BackupArchive,
  DailySummary,
  DeleteResult,
  FocusMetricsResponse,
  LocalSuggestion,
  PoliciesInfo,
  ReportResponse,
  RestoreResult,
  SearchResponse,
  StorageStats,
  Tag,
  TimelineResponse,
  UiScene,
  UpdateStatus,
  WorkflowPreset,
} from './client'

const API_BASE = '/api'
const STANDALONE_STORAGE_KEY = 'oneshim-web-standalone-mode'
const STANDALONE_QUERY_KEY = 'standalone'

function hasWindow(): boolean {
  return typeof window !== 'undefined'
}

function detectInitialStandaloneMode(): boolean {
  if (!hasWindow()) {
    return true
  }

  const params = new URLSearchParams(window.location.search)
  const queryValue = params.get(STANDALONE_QUERY_KEY)
  if (queryValue === '0' || queryValue === 'false') {
    window.localStorage.setItem(STANDALONE_STORAGE_KEY, '0')
    return false
  }
  if (queryValue === '1' || queryValue === 'true') {
    window.localStorage.setItem(STANDALONE_STORAGE_KEY, '1')
    return true
  }

  const saved = window.localStorage.getItem(STANDALONE_STORAGE_KEY)
  if (saved === '0') return false
  if (saved === '1') return true

  // Standalone-first default (opt-out via ?standalone=0)
  return true
}

let standaloneMode = detectInitialStandaloneMode()

function setStandaloneMode(enabled: boolean): void {
  standaloneMode = enabled
  if (hasWindow()) {
    window.localStorage.setItem(STANDALONE_STORAGE_KEY, enabled ? '1' : '0')
  }
}

export function isStandaloneModeEnabled(): boolean {
  return standaloneMode
}

function jsonResponse(payload: unknown, status = 200): Response {
  return new Response(JSON.stringify(payload), {
    status,
    headers: {
      'Content-Type': 'application/json',
    },
  })
}

function textBlobResponse(payload: string, contentType: string): Response {
  return new Response(payload, {
    status: 200,
    headers: {
      'Content-Type': contentType,
    },
  })
}

function parseBodyJson(body?: BodyInit | null): unknown {
  if (!body || typeof body !== 'string') {
    return null
  }
  try {
    return JSON.parse(body)
  } catch {
    return null
  }
}

function todayIsoDate(): string {
  return new Date().toISOString().split('T')[0]
}

function makeDefaultSettings(): AppSettings {
  const automation: AutomationSettings = { enabled: true }
  return {
    retention_days: 30,
    max_storage_mb: 2048,
    web_port: 9090,
    allow_external: false,
    capture_enabled: true,
    idle_threshold_secs: 300,
    metrics_interval_secs: 10,
    process_interval_secs: 30,
    notification: {
      enabled: true,
      idle_notification: true,
      idle_notification_mins: 10,
      long_session_notification: true,
      long_session_mins: 60,
      high_usage_notification: true,
      high_usage_threshold: 80,
    },
    update: {
      enabled: true,
      check_interval_hours: 24,
      include_prerelease: false,
      auto_install: false,
    },
    telemetry: {
      enabled: false,
      crash_reports: false,
      usage_analytics: false,
      performance_metrics: false,
    },
    monitor: {
      process_monitoring: true,
      input_activity: true,
      privacy_mode: false,
    },
    privacy: {
      excluded_apps: [],
      excluded_app_patterns: [],
      excluded_title_patterns: [],
      auto_exclude_sensitive: true,
      pii_filter_level: 'standard',
    },
    schedule: {
      active_hours_enabled: false,
      active_start_hour: 9,
      active_end_hour: 18,
      active_days: ['Mon', 'Tue', 'Wed', 'Thu', 'Fri'],
      pause_on_screen_lock: true,
      pause_on_battery_saver: true,
    },
    automation,
    sandbox: {
      enabled: true,
      profile: 'balanced',
      allowed_read_paths: [],
      allowed_write_paths: [],
      allow_network: false,
      max_memory_bytes: 536870912,
      max_cpu_time_ms: 30000,
    },
    ai_provider: {
      ocr_provider: 'local',
      llm_provider: 'local',
      external_data_policy: 'disabled',
      allow_unredacted_external_ocr: false,
      ocr_validation: {
        enabled: true,
        min_confidence: 0.25,
        max_invalid_ratio: 0.6,
      },
      fallback_to_local: true,
      ocr_api: null,
      llm_api: null,
    },
  }
}

function makeDefaultUpdateStatus(): UpdateStatus {
  return {
    enabled: true,
    auto_install: false,
    phase: 'Idle',
    message: 'Standalone mode',
    pending: null,
    revision: 1,
    updated_at: new Date().toISOString(),
  }
}

function makeDefaultSummary(): DailySummary {
  return {
    date: todayIsoDate(),
    total_active_secs: 0,
    total_idle_secs: 0,
    top_apps: [],
    cpu_avg: 0,
    memory_avg_percent: 0,
    frames_captured: 0,
    events_logged: 0,
  }
}

function makeDefaultStorageStats(): StorageStats {
  return {
    db_size_bytes: 0,
    frames_size_bytes: 0,
    total_size_bytes: 0,
    frame_count: 0,
    event_count: 0,
    metric_count: 0,
    oldest_data_date: null,
    newest_data_date: null,
  }
}

function makeDefaultFocusMetrics(): FocusMetricsResponse {
  return {
    today: {
      date: todayIsoDate(),
      total_active_secs: 0,
      deep_work_secs: 0,
      communication_secs: 0,
      context_switches: 0,
      interruption_count: 0,
      avg_focus_duration_secs: 0,
      max_focus_duration_secs: 0,
      focus_score: 0,
    },
    history: [],
  }
}

function makeDefaultTimeline(): TimelineResponse {
  const end = new Date()
  const start = new Date(end.getTime() - 60 * 60 * 1000)
  return {
    session: {
      start: start.toISOString(),
      end: end.toISOString(),
      duration_secs: Math.floor((end.getTime() - start.getTime()) / 1000),
      total_events: 0,
      total_frames: 0,
      total_idle_secs: 0,
    },
    items: [],
    segments: [],
  }
}

function makeDefaultReport(): ReportResponse {
  return {
    title: 'Standalone Report',
    from_date: todayIsoDate(),
    to_date: todayIsoDate(),
    days: 1,
    total_active_secs: 0,
    total_idle_secs: 0,
    total_captures: 0,
    total_events: 0,
    avg_cpu: 0,
    avg_memory: 0,
    daily_stats: [],
    app_stats: [],
    hourly_activity: [],
    productivity: {
      score: 0,
      active_ratio: 0,
      peak_hour: 0,
      top_app: '',
      trend: 0,
    },
  }
}

function makeDefaultAutomationStatus(): AutomationStatus {
  return {
    enabled: true,
    sandbox_enabled: true,
    sandbox_profile: 'balanced',
    ocr_provider: 'local',
    llm_provider: 'local',
    external_data_policy: 'disabled',
    pending_audit_entries: 0,
  }
}

function makeDefaultPolicies(): PoliciesInfo {
  return {
    automation_enabled: true,
    sandbox_profile: 'balanced',
    sandbox_enabled: true,
    allow_network: false,
    external_data_policy: 'disabled',
  }
}

function makeDefaultAutomationStats(): AutomationStats {
  return {
    total_executions: 0,
    successful: 0,
    failed: 0,
    denied: 0,
    timeout: 0,
    avg_elapsed_ms: 0,
  }
}

function makeDefaultAutomationScene(
  appName?: string,
  screenId?: string,
  frameId?: number
): UiScene {
  return {
    scene_id: `scene-standalone-${frameId ?? Date.now()}`,
    app_name: appName ?? null,
    screen_id: screenId ?? null,
    captured_at: new Date().toISOString(),
    screen_width: 1920,
    screen_height: 1080,
    elements: [
      {
        element_id: 'el-standalone-save',
        bbox_abs: { x: 128, y: 96, width: 220, height: 48 },
        bbox_norm: { x: 0.0667, y: 0.0889, width: 0.1146, height: 0.0444 },
        label: 'Save',
        role: 'button',
        intent: 'execute',
        state: 'enabled',
        confidence: 0.92,
        text_masked: 'Save',
        parent_id: null,
      },
    ],
  }
}

type StandaloneState = {
  settings: AppSettings
  updateStatus: UpdateStatus
  tags: Tag[]
  frameTags: Map<number, Set<number>>
  suggestions: LocalSuggestion[]
  presets: WorkflowPreset[]
  nextTagId: number
}

const state: StandaloneState = {
  settings: makeDefaultSettings(),
  updateStatus: makeDefaultUpdateStatus(),
  tags: [],
  frameTags: new Map(),
  suggestions: [],
  presets: [],
  nextTagId: 1,
}

function getFrameTagList(frameId: number): Tag[] {
  const tagIds = state.frameTags.get(frameId)
  if (!tagIds) return []
  return state.tags.filter((tag) => tagIds.has(tag.id))
}

function ensureFrameTagSet(frameId: number): Set<number> {
  const existing = state.frameTags.get(frameId)
  if (existing) return existing
  const created = new Set<number>()
  state.frameTags.set(frameId, created)
  return created
}

function parseId(value: string | undefined): number | null {
  if (!value) return null
  const parsed = Number(value)
  if (!Number.isFinite(parsed)) return null
  return parsed
}

function makeSearchResponse(url: URL): SearchResponse {
  const query = url.searchParams.get('q') ?? ''
  const limit = Number(url.searchParams.get('limit') ?? '50')
  const offset = Number(url.searchParams.get('offset') ?? '0')
  return {
    query,
    total: 0,
    offset: Number.isFinite(offset) ? offset : 0,
    limit: Number.isFinite(limit) ? limit : 50,
    results: [],
  }
}

function makeDeleteResult(): DeleteResult {
  return {
    success: true,
    events_deleted: 0,
    frames_deleted: 0,
    metrics_deleted: 0,
    process_snapshots_deleted: 0,
    idle_periods_deleted: 0,
    message: 'Standalone mode: no persisted backend data to delete',
  }
}

function makeRestoreResult(): RestoreResult {
  return {
    success: true,
    restored: {
      settings: false,
      tags: 0,
      frame_tags: 0,
      events: 0,
      frames: 0,
    },
    errors: [],
  }
}

function makeBackupArchive(): BackupArchive {
  return {
    metadata: {
      version: '1.0',
      created_at: new Date().toISOString(),
      app_version: 'standalone',
      includes: {
        settings: true,
        tags: true,
        events: true,
        frames: true,
      },
    },
    settings: {
      capture_enabled: state.settings.capture_enabled,
      capture_interval_secs: 60,
      idle_threshold_secs: state.settings.idle_threshold_secs,
      metrics_interval_secs: state.settings.metrics_interval_secs,
      web_port: state.settings.web_port,
      notification_enabled: state.settings.notification.enabled,
      idle_notification_mins: state.settings.notification.idle_notification_mins,
      long_session_notification_mins: state.settings.notification.long_session_mins,
      high_usage_threshold_percent: state.settings.notification.high_usage_threshold,
    },
    tags: state.tags.map((tag) => ({ ...tag })),
    frame_tags: [],
    events: [],
    frames: [],
  }
}

export async function handleStandaloneRequest(
  url: string,
  options?: RequestInit,
  force = false
): Promise<Response | null> {
  if (!standaloneMode && !force) {
    return null
  }
  if (force && !standaloneMode) {
    setStandaloneMode(true)
  }

  const requestUrl = hasWindow()
    ? new URL(url, window.location.origin)
    : new URL(url, 'http://localhost')
  const method = (options?.method ?? 'GET').toUpperCase()
  const path = requestUrl.pathname

  if (!path.startsWith(API_BASE)) {
    return null
  }

  const body = parseBodyJson(options?.body)

  if (path === '/api/stats/summary' && method === 'GET') {
    return jsonResponse(makeDefaultSummary())
  }
  if (path === '/api/metrics' && method === 'GET') {
    return jsonResponse([])
  }
  if (path === '/api/metrics/hourly' && method === 'GET') {
    return jsonResponse([])
  }
  if (path === '/api/processes' && method === 'GET') {
    return jsonResponse([])
  }
  if (path === '/api/frames' && method === 'GET') {
    const limit = Number(requestUrl.searchParams.get('limit') ?? '50')
    const offset = Number(requestUrl.searchParams.get('offset') ?? '0')
    return jsonResponse({
      data: [],
      pagination: {
        total: 0,
        offset: Number.isFinite(offset) ? offset : 0,
        limit: Number.isFinite(limit) ? limit : 50,
        has_more: false,
      },
    })
  }
  if (path === '/api/events' && method === 'GET') {
    const limit = Number(requestUrl.searchParams.get('limit') ?? '100')
    const offset = Number(requestUrl.searchParams.get('offset') ?? '0')
    return jsonResponse({
      data: [],
      pagination: {
        total: 0,
        offset: Number.isFinite(offset) ? offset : 0,
        limit: Number.isFinite(limit) ? limit : 100,
        has_more: false,
      },
    })
  }
  if (path === '/api/idle' && method === 'GET') {
    return jsonResponse([])
  }
  if (path === '/api/sessions' && method === 'GET') {
    return jsonResponse([])
  }
  if (path === '/api/stats/apps' && method === 'GET') {
    return jsonResponse({ date: todayIsoDate(), apps: [] })
  }
  if (path === '/api/storage/stats' && method === 'GET') {
    return jsonResponse(makeDefaultStorageStats())
  }
  if (path === '/api/settings' && method === 'GET') {
    return jsonResponse(state.settings)
  }
  if (path === '/api/settings' && method === 'POST') {
    if (body && typeof body === 'object') {
      state.settings = body as AppSettings
    }
    return jsonResponse(state.settings)
  }
  if (path === '/api/update/status' && method === 'GET') {
    return jsonResponse(state.updateStatus)
  }
  if (path === '/api/update/action' && method === 'POST') {
    const action = (body as { action?: string } | null)?.action
    if (action === 'CheckNow') {
      state.updateStatus = {
        ...state.updateStatus,
        phase: 'Idle',
        message: 'Standalone mode: update check skipped',
        updated_at: new Date().toISOString(),
      }
    }
    if (action === 'Approve') {
      state.updateStatus = {
        ...state.updateStatus,
        phase: 'Updated',
        message: 'Standalone mode: no binary update applied',
        updated_at: new Date().toISOString(),
      }
    }
    if (action === 'Defer') {
      state.updateStatus = {
        ...state.updateStatus,
        phase: 'Deferred',
        message: 'Standalone mode: update deferred',
        updated_at: new Date().toISOString(),
      }
    }
    return jsonResponse({ accepted: true, status: state.updateStatus })
  }
  if (path === '/api/data/range' && method === 'DELETE') {
    return jsonResponse(makeDeleteResult())
  }
  if (path === '/api/data/all' && method === 'DELETE') {
    return jsonResponse(makeDeleteResult())
  }
  if (path === '/api/search' && method === 'GET') {
    return jsonResponse(makeSearchResponse(requestUrl))
  }
  if (path === '/api/stats/heatmap' && method === 'GET') {
    const days = Number(requestUrl.searchParams.get('days') ?? '7')
    const end = new Date()
    const start = new Date(end.getTime() - Math.max(days, 1) * 24 * 60 * 60 * 1000)
    return jsonResponse({
      from_date: start.toISOString().split('T')[0],
      to_date: end.toISOString().split('T')[0],
      cells: [],
      max_value: 0,
    })
  }
  if (path.startsWith('/api/export/') && method === 'GET') {
    return textBlobResponse(
      JSON.stringify({ mode: 'standalone', exported_at: new Date().toISOString() }, null, 2),
      'application/json'
    )
  }
  if (path === '/api/tags' && method === 'GET') {
    return jsonResponse(state.tags)
  }
  if (path === '/api/tags' && method === 'POST') {
    const payload = body as { name?: string; color?: string } | null
    const newTag: Tag = {
      id: state.nextTagId++,
      name: payload?.name?.trim() || `Tag ${state.nextTagId}`,
      color: payload?.color || '#10b981',
      created_at: new Date().toISOString(),
    }
    state.tags = [newTag, ...state.tags]
    return jsonResponse(newTag, 201)
  }

  const tagPathMatch = path.match(/^\/api\/tags\/(\d+)$/)
  if (tagPathMatch) {
    const tagId = parseId(tagPathMatch[1])
    if (tagId == null) return jsonResponse({ error: 'Invalid tag id' }, 400)
    const tagIndex = state.tags.findIndex((tag) => tag.id === tagId)
    if (method === 'PUT') {
      if (tagIndex < 0) return jsonResponse({ error: 'Tag not found' }, 404)
      const payload = body as { name?: string; color?: string } | null
      const updated: Tag = {
        ...state.tags[tagIndex],
        name: payload?.name?.trim() || state.tags[tagIndex].name,
        color: payload?.color || state.tags[tagIndex].color,
      }
      state.tags[tagIndex] = updated
      return jsonResponse(updated)
    }
    if (method === 'DELETE') {
      state.tags = state.tags.filter((tag) => tag.id !== tagId)
      for (const frameTagIds of state.frameTags.values()) {
        frameTagIds.delete(tagId)
      }
      return jsonResponse({ ok: true })
    }
  }

  const frameTagsPathMatch = path.match(/^\/api\/frames\/(\d+)\/tags(?:\/(\d+))?$/)
  if (frameTagsPathMatch) {
    const frameId = parseId(frameTagsPathMatch[1])
    const tagId = parseId(frameTagsPathMatch[2])
    if (frameId == null) return jsonResponse({ error: 'Invalid frame id' }, 400)
    if (tagId == null && method === 'GET') {
      return jsonResponse(getFrameTagList(frameId))
    }
    if (tagId == null) {
      return jsonResponse({ error: 'Invalid tag id' }, 400)
    }
    if (method === 'POST') {
      ensureFrameTagSet(frameId).add(tagId)
      return jsonResponse({ ok: true }, 201)
    }
    if (method === 'DELETE') {
      ensureFrameTagSet(frameId).delete(tagId)
      return jsonResponse({ ok: true })
    }
  }

  if (path === '/api/reports' && method === 'GET') {
    return jsonResponse(makeDefaultReport())
  }
  if (path === '/api/backup' && method === 'GET') {
    return textBlobResponse(JSON.stringify(makeBackupArchive(), null, 2), 'application/json')
  }
  if (path === '/api/backup/restore' && method === 'POST') {
    return jsonResponse(makeRestoreResult())
  }
  if (path === '/api/timeline' && method === 'GET') {
    return jsonResponse(makeDefaultTimeline())
  }
  if (path === '/api/focus/metrics' && method === 'GET') {
    return jsonResponse(makeDefaultFocusMetrics())
  }
  if (path === '/api/focus/sessions' && method === 'GET') {
    return jsonResponse([])
  }
  if (path === '/api/focus/interruptions' && method === 'GET') {
    return jsonResponse([])
  }
  if (path === '/api/focus/suggestions' && method === 'GET') {
    return jsonResponse(state.suggestions)
  }

  const suggestionFeedbackMatch = path.match(/^\/api\/focus\/suggestions\/(\d+)\/feedback$/)
  if (suggestionFeedbackMatch && method === 'POST') {
    const suggestionId = parseId(suggestionFeedbackMatch[1])
    const action = (body as { action?: string } | null)?.action
    if (suggestionId != null && action) {
      state.suggestions = state.suggestions.map((suggestion) => {
        if (suggestion.id !== suggestionId) return suggestion
        const now = new Date().toISOString()
        if (action === 'shown') return { ...suggestion, shown_at: now }
        if (action === 'dismissed') return { ...suggestion, dismissed_at: now }
        if (action === 'acted') return { ...suggestion, acted_at: now }
        return suggestion
      })
    }
    return jsonResponse({ ok: true })
  }

  if (path === '/api/automation/status' && method === 'GET') {
    return jsonResponse(makeDefaultAutomationStatus())
  }
  if (path === '/api/automation/audit' && method === 'GET') {
    return jsonResponse([])
  }
  if (path === '/api/automation/policies' && method === 'GET') {
    return jsonResponse(makeDefaultPolicies())
  }
  if (path === '/api/automation/stats' && method === 'GET') {
    return jsonResponse(makeDefaultAutomationStats())
  }
  if (path === '/api/automation/scene' && method === 'GET') {
    const frameIdRaw = requestUrl.searchParams.get('frame_id')
    const frameId = frameIdRaw == null ? undefined : Number(frameIdRaw)
    return jsonResponse(
      makeDefaultAutomationScene(
        requestUrl.searchParams.get('app_name') ?? undefined,
        requestUrl.searchParams.get('screen_id') ?? undefined,
        Number.isFinite(frameId) ? frameId : undefined
      )
    )
  }
  if (path === '/api/automation/presets' && method === 'GET') {
    return jsonResponse({ presets: state.presets })
  }
  if (path === '/api/automation/presets' && method === 'POST') {
    const payload = body as WorkflowPreset | null
    if (!payload) return jsonResponse({ error: 'Invalid preset' }, 400)
    state.presets = [...state.presets, payload]
    return jsonResponse(payload, 201)
  }

  const presetDeleteMatch = path.match(/^\/api\/automation\/presets\/([^/]+)$/)
  if (presetDeleteMatch && method === 'DELETE') {
    const presetId = decodeURIComponent(presetDeleteMatch[1])
    state.presets = state.presets.filter((preset) => preset.id !== presetId)
    return jsonResponse({ ok: true })
  }

  const presetRunMatch = path.match(/^\/api\/automation\/presets\/([^/]+)\/run$/)
  if (presetRunMatch && method === 'POST') {
    const presetId = decodeURIComponent(presetRunMatch[1])
    const preset = state.presets.find((item) => item.id === presetId)
    const totalSteps = preset?.steps.length ?? 0
    return jsonResponse({
      preset_id: presetId,
      success: true,
      message: 'Standalone mode preset run simulated',
      steps_executed: totalSteps,
      total_steps: totalSteps,
      total_elapsed_ms: totalSteps * 10,
    })
  }

  if (path === '/api/automation/execute-hint' && method === 'POST') {
    const payload = body as
      | { command_id?: string; session_id?: string; intent_hint?: string }
      | null
    const now = Date.now()
    const commandId = payload?.command_id?.trim() || `intent-hint-${now}`
    const sessionId = payload?.session_id?.trim() || 'standalone-session'
    const hint = payload?.intent_hint?.trim() || ''

    return jsonResponse({
      command_id: commandId,
      session_id: sessionId,
      planned_intent: {
        ClickElement: {
          text: hint || null,
          role: null,
          app_name: null,
          button: 'left',
        },
      },
      result: {
        success: true,
        element: null,
        verification: null,
        retry_count: 0,
        elapsed_ms: 0,
        error: null,
      },
    })
  }

  // Unknown /api route fallback: avoid hard failures in standalone mode.
  return jsonResponse({ ok: true })
}
