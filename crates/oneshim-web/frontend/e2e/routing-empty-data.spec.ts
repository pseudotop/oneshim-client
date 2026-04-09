/**
 * Regression suite for the empty-data layout `<Outlet>` suppression bug class.
 *
 * Background: the layout for each parent route used to short-circuit with an
 * EmptyState (or an error Card) whenever its query returned no data, which
 * suppressed the `<Outlet>` and prevented RouteRenderer's index
 * `<Navigate to="<defaultChild>" replace />` from ever firing. AuditLayout
 * was fixed first (2026-04-08), but the same pattern was repeated across
 * Dashboard, Timeline, Replay, Automation, Focus and Reports layouts.
 *
 * Each test here overrides the relevant mock(s) so the layout's query lands
 * on the previously-buggy branch, then navigates directly to the parent
 * route and asserts the URL eventually lands on the default child. If any
 * layout re-introduces the "early-return EmptyState instead of rendering
 * Outlet" anti-pattern, these tests will fail.
 */

import { expect, test } from './helpers/test'
import { mockStaticJson } from './helpers/mock-api'

test.describe('Sub-pathname routing redirects survive empty data', () => {
  test('/ → /overview when summary is empty (no events/frames/active time)', async ({ page }) => {
    // DashboardLayout early-returned EmptyState when:
    //   !latestMetrics && !summary.events_logged && !summary.frames_captured && summary.total_active_secs === 0
    // Force the guard to trigger and verify the index redirect still fires.
    await mockStaticJson(page, '**/api/stats/summary**', {
      date: '2026-02-23',
      total_active_secs: 0,
      total_idle_secs: 0,
      top_apps: [],
      cpu_avg: 0,
      memory_avg_percent: 0,
      frames_captured: 0,
      events_logged: 0,
    })

    await page.goto('/')
    await page.waitForURL('**/overview')
    await expect(page).toHaveURL(/\/overview$/)
  })

  test('/timeline → /timeline/all when no frames are stored yet', async ({ page }) => {
    await mockStaticJson(page, '**/api/frames**', {
      data: [],
      pagination: { total: 0, offset: 0, limit: 50, has_more: false },
    })

    await page.goto('/timeline')
    await page.waitForURL('**/timeline/all')
    await expect(page).toHaveURL(/\/timeline\/all$/)
  })

  test('/replay → /replay/timeline when the session has no items', async ({ page }) => {
    await mockStaticJson(page, '**/api/timeline**', {
      session: {
        start: '2026-02-23T09:55:00Z',
        end: '2026-02-23T10:15:00Z',
        duration_secs: 1200,
        total_events: 0,
        total_frames: 0,
        total_idle_secs: 0,
      },
      items: [],
      segments: [],
    })

    await page.goto('/replay')
    await page.waitForURL('**/replay/timeline')
    await expect(page).toHaveURL(/\/replay\/timeline$/)
  })

  test('/automation → /automation/policies when automation is disabled and has zero runs', async ({ page }) => {
    await mockStaticJson(page, '**/api/automation/status**', {
      enabled: false,
      sandbox_enabled: false,
      sandbox_profile: 'balanced',
      ocr_provider: 'local',
      llm_provider: 'local',
      ocr_source: 'local',
      llm_source: 'local',
      ocr_fallback_reason: null,
      llm_fallback_reason: null,
      external_data_policy: 'disabled',
      pending_audit_entries: 0,
    })
    // fallback automation/stats already reports all-zero totals, which together
    // with `enabled: false` satisfies the previously-buggy short-circuit.

    await page.goto('/automation')
    await page.waitForURL('**/automation/policies')
    await expect(page).toHaveURL(/\/automation\/policies$/)
  })

  test('/focus → /focus/score when focus score is zero', async ({ page }) => {
    await mockStaticJson(page, '**/api/focus/metrics**', {
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
    })

    await page.goto('/focus')
    await page.waitForURL('**/focus/score')
    await expect(page).toHaveURL(/\/focus\/score$/)
  })

  test('/reports → /reports/activity when the report query fails', async ({ page }) => {
    // 500 pushes useQuery into its error branch; ReportsLayout used to suppress
    // the Outlet in that path (only rendered `{report && <Outlet />}`),
    // which prevented the index redirect.
    await page.route('**/api/reports**', (route) =>
      route.fulfill({
        status: 500,
        contentType: 'application/json',
        body: JSON.stringify({ error: 'internal' }),
      }),
    )

    await page.goto('/reports')
    await page.waitForURL('**/reports/activity')
    await expect(page).toHaveURL(/\/reports\/activity$/)
  })

  test('/audit → /audit/summary when there are no audit entries (regression baseline)', async ({ page }) => {
    await mockStaticJson(page, '**/api/automation/audit**', [])

    await page.goto('/audit')
    await page.waitForURL('**/audit/summary')
    await expect(page).toHaveURL(/\/audit\/summary$/)
  })
})

test.describe('Shell sidebar-hidden grid toggle', () => {
  test('/day has no sidebar and main fills the viewport', async ({ page }) => {
    // /day has no route children → SidePanel returns null → .sidebar-hidden
    // class collapses the grid to 2 columns. Main content should be wide.
    await page.goto('/day')
    await page.waitForTimeout(500)

    const shell = page.locator('.app-shell')
    await expect(shell).toHaveClass(/sidebar-hidden/)

    const main = page.locator('main#main-content')
    const mainBox = await main.boundingBox()
    // With a 1280px viewport and 48px activitybar, main ≈ 1232px
    expect(mainBox!.width).toBeGreaterThan(1000)
  })

  test('/ has sidebar visible (no sidebar-hidden)', async ({ page }) => {
    // / has children (overview, monitoring, insights) → sidebar shows
    await page.goto('/')
    await page.waitForURL('**/overview')

    const shell = page.locator('.app-shell')
    await expect(shell).not.toHaveClass(/sidebar-hidden/)
  })

  test('/day → / restores the sidebar column', async ({ page }) => {
    // Navigate to no-children route, then back to children route
    await page.goto('/day')
    await page.waitForTimeout(500)
    await expect(page.locator('.app-shell')).toHaveClass(/sidebar-hidden/)

    // Navigate to Dashboard (has children)
    await page.getByTestId('nav-dashboard').click()
    await page.waitForURL('**/overview')
    await expect(page.locator('.app-shell')).not.toHaveClass(/sidebar-hidden/)
  })
})

test.describe('Empty state CTA navigation', () => {
  test('timeline empty state "Open Settings" navigates to /settings/monitoring', async ({ page }) => {
    // Mock frames as empty with capture disabled
    await mockStaticJson(page, '**/api/frames**', {
      data: [],
      pagination: { total: 0, offset: 0, limit: 50, has_more: false },
    })
    await mockStaticJson(page, '**/api/settings', {
      capture_enabled: false,
    })

    await page.goto('/timeline/all')

    // The EmptyState CTA button should be visible
    const ctaButton = page.getByRole('button', { name: /Open Settings|설정 열기|設定を開く|Abrir Configuración|打开设置/ })
    await expect(ctaButton).toBeVisible({ timeout: 10000 })

    await ctaButton.click()
    await expect(page).toHaveURL(/\/settings\/monitoring$/)
  })
})
