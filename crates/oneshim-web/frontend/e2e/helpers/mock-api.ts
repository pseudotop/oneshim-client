import type { Page, Request, Route } from '@playwright/test'

type RoutePattern = Parameters<Page['route']>[0]
type JsonResolver = (request: Request) => unknown | Promise<unknown>

const STREAM_HEADERS = {
  'cache-control': 'no-cache',
  connection: 'keep-alive',
  'content-type': 'text/event-stream',
}

async function fulfillJson(route: Route, payload: unknown, status = 200): Promise<void> {
  await route.fulfill({
    status,
    contentType: 'application/json',
    body: JSON.stringify(payload),
  })
}

export async function mockStaticJson(
  page: Page,
  pattern: RoutePattern,
  payload: unknown,
  status = 200
): Promise<void> {
  await page.route(pattern, async (route) => {
    await fulfillJson(route, payload, status)
  })
}

export async function mockDynamicJson(
  page: Page,
  pattern: RoutePattern,
  resolver: JsonResolver,
  status = 200
): Promise<void> {
  await page.route(pattern, async (route) => {
    const payload = await resolver(route.request())
    await fulfillJson(route, payload, status)
  })
}

export async function mockBackgroundStreams(page: Page): Promise<void> {
  await page.route('**/api/stream**', async (route) => {
    await route.fulfill({
      status: 200,
      headers: STREAM_HEADERS,
      body: 'event: message\ndata: {}\n\n',
    })
  })

  await page.route('**/api/update/stream**', async (route) => {
    await route.fulfill({
      status: 200,
      headers: STREAM_HEADERS,
      body: 'event: message\ndata: {}\n\n',
    })
  })
}

const fallbackSettings = {
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
  automation: {
    enabled: true,
  },
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
    fallback_to_local: true,
    ocr_api: null,
    llm_api: null,
  },
}

const fallbackUpdateStatus = {
  enabled: true,
  auto_install: false,
  phase: 'Idle',
  message: null,
  pending: null,
  revision: 1,
  updated_at: '2026-02-23T10:00:00Z',
}

const fallbackStorageStats = {
  db_size_bytes: 0,
  frames_size_bytes: 0,
  total_size_bytes: 0,
  frame_count: 0,
  event_count: 0,
  metric_count: 0,
  oldest_data_date: null,
  newest_data_date: null,
}

const fallbackSummary = {
  date: '2026-02-23',
  total_active_secs: 0,
  total_idle_secs: 0,
  top_apps: [],
  cpu_avg: 0,
  memory_avg_percent: 0,
  frames_captured: 0,
  events_logged: 0,
}

const fallbackReport = {
  title: 'Fallback Report',
  from_date: '2026-02-17',
  to_date: '2026-02-23',
  days: 7,
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

const fallbackFrames = {
  data: [],
  pagination: {
    total: 0,
    offset: 0,
    limit: 50,
    has_more: false,
  },
}

const fallbackFocusMetrics = {
  today: {
    date: '2026-02-23',
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

export async function mockDefaultApiFallbacks(page: Page): Promise<void> {
  await mockDynamicJson(page, '**/api/settings', (request) =>
    request.method() === 'POST' ? request.postDataJSON() ?? fallbackSettings : fallbackSettings
  )
  await mockStaticJson(page, '**/api/update/status**', fallbackUpdateStatus)
  await mockStaticJson(page, '**/api/storage/stats**', fallbackStorageStats)
  await mockStaticJson(page, '**/api/stats/summary**', fallbackSummary)
  await mockStaticJson(page, '**/api/metrics/hourly**', [])
  await mockStaticJson(page, '**/api/processes**', [])
  await mockStaticJson(page, '**/api/tags**', [])
  await mockStaticJson(page, '**/api/frames**', fallbackFrames)
  await mockStaticJson(page, '**/api/reports**', fallbackReport)
  await mockStaticJson(page, '**/api/focus/metrics**', fallbackFocusMetrics)
  await mockStaticJson(page, '**/api/focus/sessions**', [])
  await mockStaticJson(page, '**/api/focus/interruptions**', [])
  await mockStaticJson(page, '**/api/focus/suggestions**', [])
  await mockDynamicJson(page, '**/api/automation/execute-hint', (request) => {
    const payload = request.postDataJSON() as
      | { command_id?: string; session_id?: string; intent_hint?: string }
      | undefined
    return {
      command_id: payload?.command_id ?? 'intent-hint-e2e',
      session_id: payload?.session_id ?? 'sess-e2e',
      planned_intent: {
        ClickElement: {
          text: payload?.intent_hint ?? null,
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
    }
  })
}
