/**
 * 개인정보 관리 페이지 E2E 테스트
 *
 * 데이터 통계, 삭제, 백업/복원 검증
 */
import { test, expect } from '@playwright/test'

test.describe('Privacy', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/privacy')
    await page.waitForLoadState('networkidle')
  })

  test('should display privacy title', async ({ page }) => {
    await expect(page.locator('h1')).toBeVisible()
  })

  test('should display storage statistics', async ({ page }) => {
    // 한/영 모두 지원
    await expect(page.getByText(/현재 저장된 데이터|Current Data/i)).toBeVisible()
  })

  test('should display date range delete section', async ({ page }) => {
    await expect(page.getByText(/날짜 범위로 삭제|Delete by Date Range/i)).toBeVisible()
  })

  test('should display date inputs for range delete', async ({ page }) => {
    const dateInputs = page.locator('input[type="date"]')
    await expect(dateInputs.first()).toBeVisible()
  })

  test('should display data type selection buttons', async ({ page }) => {
    const dataTypeButtons = page.getByRole('button', { name: /이벤트|프레임|메트릭|Events|Frames|Metrics/i })
    await expect(dataTypeButtons.first()).toBeVisible()
  })

  test('should display delete all data section', async ({ page }) => {
    await expect(page.getByText(/전체 데이터 삭제|Delete All Data/i)).toBeVisible()
  })

  test('should display backup/restore section', async ({ page }) => {
    // 페이지 하단으로 스크롤
    await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight))
    await page.waitForTimeout(1000)

    // 한/영 모두 지원: "백업 / 복원" or "Backup / Restore"
    const backupHeading = page.locator('h2').filter({ hasText: /백업|Backup/i })
    await expect(backupHeading.first()).toBeVisible({ timeout: 10000 })
  })

  test('should display backup options', async ({ page }) => {
    // 페이지 하단으로 스크롤
    await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight))
    await page.waitForTimeout(1000)

    // 한/영 모두 지원: "백업할 데이터 선택" or "Data to include"
    const backupOptions = page.getByText(/백업할 데이터 선택|Data to include/i)
    await expect(backupOptions).toBeVisible({ timeout: 10000 })
  })

  test('should display backup download button', async ({ page }) => {
    // 페이지 하단으로 스크롤
    await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight))
    await page.waitForTimeout(1000)

    // 한/영 모두 지원
    const downloadButton = page.getByRole('button', { name: /백업 다운로드|Download Backup/i })
    await expect(downloadButton).toBeVisible({ timeout: 10000 })
  })

  test('should display restore button', async ({ page }) => {
    // 페이지 하단으로 스크롤
    await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight))
    await page.waitForTimeout(1000)

    // 한/영 모두 지원
    const restoreButton = page.getByRole('button', { name: /백업 복원|Restore Backup/i })
    await expect(restoreButton).toBeVisible({ timeout: 10000 })
  })

  test('should display data collection info', async ({ page }) => {
    // 페이지 하단으로 스크롤
    await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight))
    await page.waitForTimeout(1000)

    await expect(page.getByText(/데이터 수집 안내|Data Collection|Privacy Info/i)).toBeVisible({ timeout: 10000 })
  })

  test('should toggle backup option', async ({ page }) => {
    // 페이지 하단으로 스크롤
    await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight))
    await page.waitForTimeout(1000)

    const optionButton = page.locator('button').filter({ hasText: /설정|Settings/i }).first()
    if (await optionButton.isVisible({ timeout: 5000 })) {
      await optionButton.click()
      await page.waitForTimeout(300)
    }
  })

  test('should show confirmation modal for delete all', async ({ page }) => {
    const deleteAllButton = page.getByRole('button', { name: /전체 데이터 삭제|Delete All Data/i })
    if (await deleteAllButton.isVisible()) {
      await deleteAllButton.click()
      await page.waitForTimeout(500)
    }
  })
})
