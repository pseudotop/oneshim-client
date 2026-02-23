import { test, expect, type Page } from '@playwright/test'

const dashboardHeadingName = /activity summary|활동 요약|dashboard preparing|대시보드 준비 중/i

const mockedSummary = {
  date: '2026-02-23',
  total_active_secs: 7200,
  total_idle_secs: 900,
  top_apps: [
    {
      name: 'Code',
      duration_secs: 3600,
      event_count: 12,
      frame_count: 8,
    },
  ],
  cpu_avg: 21.5,
  memory_avg_percent: 42.7,
  frames_captured: 8,
  events_logged: 16,
}

const mockedHourlyMetrics = [
  {
    hour: '10:00',
    cpu_avg: 21.5,
    cpu_max: 30.1,
    memory_avg: 42.7,
    memory_max: 50.2,
    sample_count: 6,
  },
]

const mockedProcesses = [
  {
    timestamp: '2026-02-23T10:00:00Z',
    processes: [
      {
        pid: 12345,
        name: 'Code',
        cpu_usage: 11.1,
        memory_bytes: 345678912,
      },
    ],
  },
]

const mockedHeatmap = {
  from_date: '2026-02-17',
  to_date: '2026-02-23',
  cells: [
    { day: 1, hour: 10, value: 1 },
    { day: 2, hour: 14, value: 2 },
  ],
  max_value: 2,
}

const mockedFocusMetrics = {
  today: {
    date: '2026-02-23',
    total_active_secs: 7200,
    deep_work_secs: 3600,
    communication_secs: 900,
    context_switches: 5,
    interruption_count: 2,
    avg_focus_duration_secs: 1200,
    max_focus_duration_secs: 2400,
    focus_score: 82,
  },
  history: [],
}

const mockedUpdateStatus = {
  enabled: true,
  auto_install: false,
  phase: 'Idle',
  message: null,
  pending: null,
  revision: 1,
  updated_at: '2026-02-23T10:00:00Z',
}

function dashboardHeading(page: Page) {
  return page.getByRole('heading', { name: dashboardHeadingName })
}

async function mockDashboardApis(page: Page) {
  await page.route('**/api/stats/summary**', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(mockedSummary),
    })
  })

  await page.route('**/api/metrics/hourly**', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(mockedHourlyMetrics),
    })
  })

  await page.route('**/api/processes**', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(mockedProcesses),
    })
  })

  await page.route('**/api/stats/heatmap**', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(mockedHeatmap),
    })
  })

  await page.route('**/api/focus/metrics**', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(mockedFocusMetrics),
    })
  })

  await page.route('**/api/focus/suggestions**', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify([]),
    })
  })

  await page.route('**/api/update/status**', async (route) => {
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify(mockedUpdateStatus),
    })
  })
}

test.describe('Dashboard', () => {
  test.beforeEach(async ({ page }) => {
    await mockDashboardApis(page)
    await page.goto('/')
    await expect(dashboardHeading(page)).toBeVisible({ timeout: 10000 })
  })

  test('should display dashboard title', async ({ page }) => {
    await expect(dashboardHeading(page)).toBeVisible()
  })

  test('should display stat cards', async ({ page }) => {
    await expect(page.getByText(/active time|활동 시간/i)).toBeVisible()
    await expect(page.getByText(/idle time|유휴 시간/i)).toBeVisible()
    await expect(page.getByText(/captures|캡처 수/i).first()).toBeVisible()
    await expect(page.getByText(/events|이벤트/i).first()).toBeVisible()
  })

  test('should display realtime monitoring section', async ({ page }) => {
    await expect(
      page.getByText(/connecting|연결 중|connected|연결됨|disconnected|연결 끊김|error|오류/i).first()
    ).toBeVisible()
  })

  test('should display CPU/Memory chart section', async ({ page }) => {
    await expect(page.getByText(/CPU \/ Memory Usage \(24h\)|CPU \/ Memory 사용량 \(24시간\)/i)).toBeVisible()
  })

  test('should display app usage section', async ({ page }) => {
    await expect(page.getByText(/app usage time|앱 사용 시간/i)).toBeVisible()
  })

  test('should display activity heatmap', async ({ page }) => {
    await expect(page.getByText(/activity heatmap|활동 히트맵/i)).toBeVisible()
  })

  test('should display system status section', async ({ page }) => {
    await expect(page.getByText(/system status|시스템 상태/i)).toBeVisible()
  })

  test('should show connection status indicator', async ({ page }) => {
    const connectionStatus = page.getByText(
      /connecting|연결 중|connected|연결됨|disconnected|연결 끊김|error|오류/i
    )
    await expect(connectionStatus.first()).toBeVisible({ timeout: 10000 })
  })
})
