/**
 * 네비게이션 E2E 테스트
 *
 * 페이지 이동 및 네비게이션 UI 검증
 */
import { test, expect } from '@playwright/test'

test.describe('Navigation', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
  })

  test('should display navigation links', async ({ page }) => {
    // 네비게이션 링크 존재 확인
    await expect(page.getByRole('link', { name: /dashboard|대시보드/i })).toBeVisible()
    await expect(page.getByRole('link', { name: /timeline|타임라인/i })).toBeVisible()
    await expect(page.getByRole('link', { name: /reports|리포트/i })).toBeVisible()
    await expect(page.getByRole('link', { name: /settings|설정/i })).toBeVisible()
    await expect(page.getByRole('link', { name: /privacy|개인정보/i })).toBeVisible()
  })

  test('should navigate to Dashboard', async ({ page }) => {
    await page.getByRole('link', { name: /dashboard|대시보드/i }).click()
    await expect(page).toHaveURL('/')
  })

  test('should navigate to Timeline', async ({ page }) => {
    await page.getByRole('link', { name: /timeline|타임라인/i }).click()
    await expect(page).toHaveURL('/timeline')
  })

  test('should navigate to Reports', async ({ page }) => {
    await page.getByRole('link', { name: /reports|리포트/i }).click()
    await expect(page).toHaveURL('/reports')
  })

  test('should navigate to Settings', async ({ page }) => {
    await page.getByRole('link', { name: /settings|설정/i }).click()
    await expect(page).toHaveURL('/settings')
  })

  test('should navigate to Privacy', async ({ page }) => {
    await page.getByRole('link', { name: /privacy|개인정보/i }).click()
    await expect(page).toHaveURL('/privacy')
  })

  test('should navigate to Search', async ({ page }) => {
    await page.getByRole('link', { name: /search|검색/i }).click()
    await expect(page).toHaveURL('/search')
  })

  test('should show keyboard shortcuts help with ? key', async ({ page }) => {
    // ? 키로 도움말 모달 열기
    // 입력 필드에 포커스가 없는 상태에서 ? 키 입력
    await page.locator('body').click()
    await page.keyboard.press('Shift+Slash')
    await page.waitForTimeout(500)

    const modal = page.getByRole('heading', { name: /키보드 단축키/i })
    await expect(modal).toBeVisible()
  })

  test('should navigate with keyboard shortcuts', async ({ page }) => {
    // D 키로 대시보드 이동
    await page.keyboard.press('d')
    await expect(page).toHaveURL('/')

    // T 키로 타임라인 이동
    await page.keyboard.press('t')
    await expect(page).toHaveURL('/timeline')

    // S 키로 설정 이동
    await page.keyboard.press('s')
    await expect(page).toHaveURL('/settings')

    // P 키로 개인정보 이동
    await page.keyboard.press('p')
    await expect(page).toHaveURL('/privacy')
  })
})
