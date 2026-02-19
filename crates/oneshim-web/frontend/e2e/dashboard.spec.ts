/**
 * 대시보드 페이지 E2E 테스트
 *
 * 활동 요약, 차트, 실시간 모니터링 검증
 */
import { test, expect } from '@playwright/test'

test.describe('Dashboard', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
  })

  test('should display dashboard title', async ({ page }) => {
    await expect(page.getByRole('heading', { name: /activity summary|활동 요약/i })).toBeVisible()
  })

  test('should display stat cards', async ({ page }) => {
    // 통계 카드 확인 (4개)
    const statCards = page.locator('[data-testid="stat-card"], .rounded-lg').filter({
      has: page.locator('text=/\\d+/')
    })
    await expect(statCards.first()).toBeVisible()
  })

  test('should display realtime monitoring section', async ({ page }) => {
    // 실시간 모니터링 섹션 확인
    await expect(page.getByText(/realtime|실시간/i)).toBeVisible()
  })

  test('should display CPU/Memory chart section', async ({ page }) => {
    // CPU/Memory 차트 섹션 (한/영 모두 지원) - h2 제목만 선택
    await expect(page.locator('h2').filter({ hasText: /CPU.*Memory/i }).first()).toBeVisible()
  })

  test('should display app usage section', async ({ page }) => {
    // 앱 사용 시간 섹션
    await expect(page.getByText(/app usage|앱 사용/i)).toBeVisible()
  })

  test('should display activity heatmap', async ({ page }) => {
    // 히트맵 섹션
    await expect(page.getByText(/heatmap|히트맵/i)).toBeVisible()
  })

  test('should display system status section', async ({ page }) => {
    // 시스템 상태 섹션
    await expect(page.getByText(/system status|시스템 상태/i)).toBeVisible()
  })

  test('should show connection status indicator', async ({ page }) => {
    // SSE 연결 상태 표시 확인 (실시간, 연결 중, 연결 끊김)
    const connectionStatus = page.getByText(/실시간|연결 중|연결 끊김|realtime|connecting|disconnected/i)
    await expect(connectionStatus.first()).toBeVisible({ timeout: 10000 })
  })
})
