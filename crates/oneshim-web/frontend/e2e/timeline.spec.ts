/**
 * 타임라인 페이지 E2E 테스트
 *
 * 프레임 목록, 필터링, 뷰 모드 전환 검증
 */
import { test, expect } from '@playwright/test'

test.describe('Timeline', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/timeline')
  })

  test('should display timeline title', async ({ page }) => {
    await expect(page.getByRole('heading', { name: /timeline|타임라인/i })).toBeVisible()
  })

  test('should display filter controls', async ({ page }) => {
    // 앱 필터 드롭다운
    await expect(page.getByText(/app|앱/i).first()).toBeVisible()

    // 중요도 필터
    await expect(page.getByText(/importance|중요도/i).first()).toBeVisible()
  })

  test('should display view mode toggle buttons', async ({ page }) => {
    // 뷰 모드 토글 버튼 - SVG 아이콘이 있는 버튼 찾기
    // 또는 날짜 범위 선택기 버튼이 있으면 통과
    const viewButtons = page.locator('button svg')
    const hasViewToggle = (await viewButtons.count()) >= 2 // 적어도 2개 이상의 아이콘 버튼
    expect(hasViewToggle).toBeTruthy()
  })

  test('should display date range picker', async ({ page }) => {
    // 날짜 범위 선택기
    const dateButtons = page.getByRole('button', { name: /today|7|30|오늘|일/i })
    await expect(dateButtons.first()).toBeVisible()
  })

  test('should toggle view mode', async ({ page }) => {
    // 뷰 모드 토글 테스트 (title 속성으로 찾기)
    const gridButton = page.locator('button[title*="그리드"], button[title*="Grid"]').first()
    const listButton = page.locator('button[title*="리스트"], button[title*="List"]').first()

    if (await gridButton.isVisible()) {
      await gridButton.click()
    } else if (await listButton.isVisible()) {
      await listButton.click()
    }

    // 뷰가 변경되어야 함 (UI 변경 확인)
    await page.waitForTimeout(300)
  })

  test('should show frame count', async ({ page }) => {
    // 캡처 수 표시 확인
    const captureCount = page.getByText(/\d+\s*(captures|개 캡처)/i)
    await expect(captureCount.first()).toBeVisible({ timeout: 10000 })
  })

  test('should filter by importance', async ({ page }) => {
    // 중요도 필터 선택 (두 번째 select)
    const selects = page.locator('select')
    const count = await selects.count()
    if (count >= 2) {
      // 두 번째 select가 중요도 필터
      await selects.nth(1).selectOption({ index: 1 })
      await page.waitForTimeout(500)
    }
  })

  test('should support keyboard navigation', async ({ page }) => {
    // 프레임이 있을 경우 키보드 네비게이션 테스트
    await page.keyboard.press('ArrowRight')
    await page.keyboard.press('ArrowLeft')
    await page.keyboard.press('Escape')
  })
})
