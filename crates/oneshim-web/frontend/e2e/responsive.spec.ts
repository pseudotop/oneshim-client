import { test, expect, type Page } from './helpers/test'
import { i18nRegex } from './helpers/i18n'
import { mockDynamicJson, mockStaticJson } from './helpers/mock-api'

const dashboardHeadingName = i18nRegex('dashboard.title')
const timelineHeadingName = i18nRegex('timeline.title')
const searchHeadingName = i18nRegex('search.title')
const globalSearchPlaceholder = /\(Enter\)/i

async function mockResponsiveApis(page: Page) {
  await mockStaticJson(page, '**/api/stats/summary**', {
    date: '2026-02-23',
    total_active_secs: 3600,
    total_idle_secs: 600,
    top_apps: [{ name: 'Code', duration_secs: 1800, event_count: 3, frame_count: 2 }],
    cpu_avg: 18.2,
    memory_avg_percent: 40.1,
    frames_captured: 2,
    events_logged: 3,
  })
  await mockStaticJson(page, '**/api/metrics/hourly**', [
    {
      hour: '10:00',
      cpu_avg: 18.2,
      cpu_max: 25.1,
      memory_avg: 40.1,
      memory_max: 44.3,
      sample_count: 4,
    },
  ])
  await mockStaticJson(page, '**/api/processes**', [
    {
      timestamp: '2026-02-23T10:00:00Z',
      processes: [{ pid: 1001, name: 'Code', cpu_usage: 10.2, memory_bytes: 345678912 }],
    },
  ])
  await mockStaticJson(page, '**/api/focus/metrics**', {
    today: {
      date: '2026-02-23',
      total_active_secs: 3600,
      deep_work_secs: 1800,
      communication_secs: 300,
      context_switches: 4,
      interruption_count: 1,
      avg_focus_duration_secs: 900,
      max_focus_duration_secs: 1800,
      focus_score: 78,
    },
    history: [],
  })
  await mockStaticJson(page, '**/api/focus/suggestions**', [])
  await mockStaticJson(page, '**/api/stats/heatmap**', {
    from_date: '2026-02-17',
    to_date: '2026-02-23',
    cells: [{ day: 1, hour: 10, value: 1 }],
    max_value: 1,
  })
  await mockStaticJson(page, '**/api/update/status**', {
    enabled: true,
    auto_install: false,
    phase: 'Idle',
    message: null,
    pending: null,
    revision: 1,
    updated_at: '2026-02-23T10:00:00Z',
  })
  await mockStaticJson(page, '**/api/tags**', [])
  await mockStaticJson(page, '**/api/frames**', {
    data: [
      {
        id: 1,
        timestamp: '2026-02-23T10:00:00Z',
        trigger_type: 'interval',
        app_name: 'Code',
        window_title: 'ONESHIM',
        importance: 0.8,
        resolution: '1920x1080',
        file_path: null,
        ocr_text: null,
        image_url: null,
        tag_ids: [],
      },
    ],
    pagination: { total: 1, offset: 0, limit: 50, has_more: false },
  })
  await mockDynamicJson(page, '**/api/search**', async (request) => {
    const url = new URL(request.url())
    const query = url.searchParams.get('q') ?? ''
    return {
      query,
      total: 1,
      offset: 0,
      limit: 20,
      results: [
        {
          result_type: 'frame',
          id: '1',
          timestamp: '2026-02-23T10:00:00Z',
          app_name: 'Code',
          window_title: 'ONESHIM',
          matched_text: query || 'focus',
          image_url: null,
          importance: 0.8,
          tags: [],
        },
      ],
    }
  })
}

test.describe('Responsive', () => {
  test('supports mobile navigation in 430x932 viewport', async ({ page }) => {
    await page.setViewportSize({ width: 430, height: 932 })
    await mockResponsiveApis(page)

    await page.goto('/')
    await expect(page.getByRole('heading', { name: dashboardHeadingName })).toBeVisible()

    await page.locator('a[href="/timeline"]').first().click()

    await expect(page).toHaveURL(/\/timeline/)
    await expect(page.getByRole('heading', { name: timelineHeadingName })).toBeVisible()
  })

  test('supports tablet global search flow in 768x1024 viewport', async ({ page }) => {
    await page.setViewportSize({ width: 768, height: 1024 })
    await mockResponsiveApis(page)

    await page.goto('/')
    await expect(page.getByRole('heading', { name: dashboardHeadingName })).toBeVisible()

    const globalSearch = page.getByPlaceholder(globalSearchPlaceholder)
    await expect(globalSearch).toBeVisible()
    await globalSearch.fill('focus')
    await globalSearch.press('Enter')

    await expect(page).toHaveURL(/\/search\?q=focus/)
    await expect(page.getByRole('heading', { name: searchHeadingName })).toBeVisible()
  })
})
