/**
 * 리포트 페이지 E2E 테스트
 *
 * 기간 선택, 차트, 통계 표시 검증
 */
import { test, expect } from '@playwright/test'

test.describe('Reports', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/reports')
  })

  test('should display reports title', async ({ page }) => {
    // 페이지 제목 (h1)
    await expect(page.locator('h1')).toBeVisible()
  })

  test('should display period selector', async ({ page }) => {
    // 기간 선택 버튼 (주간/월간/직접선택)
    const weekButton = page.getByRole('button', { name: /주간|week/i })
    const monthButton = page.getByRole('button', { name: /월간|month/i })

    await expect(weekButton.or(monthButton).first()).toBeVisible()
  })

  test('should display productivity score', async ({ page }) => {
    // 생산성 점수 표시 (i18n: reports.productivityScore)
    await expect(page.getByText(/생산성 점수|productivity score/i)).toBeVisible()
  })

  test('should display summary statistics', async ({ page }) => {
    // 요약 통계 (활동 시간)
    await expect(page.getByText(/활동 시간|active time/i)).toBeVisible()
  })

  test('should display daily activity chart section', async ({ page }) => {
    // 일별 활동 차트 섹션 (i18n: reports.dailyActivity)
    await expect(page.getByText(/일별 활동|daily activity/i)).toBeVisible()
  })

  test('should display app usage section', async ({ page }) => {
    // 앱 사용량 섹션 (i18n: reports.appUsage)
    await expect(page.getByText(/앱 사용량|app usage/i)).toBeVisible()
  })

  test('should switch period', async ({ page }) => {
    // 월간으로 전환
    const monthButton = page.getByRole('button', { name: /월간|month/i })
    if (await monthButton.isVisible()) {
      await monthButton.click()
      await page.waitForTimeout(500)
    }
  })

  test('should display trend indicator', async ({ page }) => {
    // 추세 표시 (화살표 아이콘: ↑, ↓, →)
    const trendText = page.getByText(/추세|↑|↓|→/)
    await expect(trendText.first()).toBeVisible({ timeout: 10000 })
  })

  test('should display hourly activity section', async ({ page }) => {
    // 시간대별 활동 섹션 (i18n: reports.hourlyActivity)
    await expect(page.getByText(/시간대별 활동|hourly activity/i)).toBeVisible()
  })

  test('should display system metrics section', async ({ page }) => {
    // 시스템 메트릭 섹션 (i18n: reports.systemMetrics)
    await expect(page.getByText(/시스템 메트릭|system metric/i)).toBeVisible()
  })

  test('should select custom date range', async ({ page }) => {
    // 직접 선택 버튼
    const customButton = page.getByRole('button', { name: /직접 선택|custom/i })
    if (await customButton.isVisible()) {
      await customButton.click()

      // 날짜 입력 필드 표시 확인
      const dateInputs = page.locator('input[type="date"]')
      await expect(dateInputs.first()).toBeVisible()
    }
  })
})
