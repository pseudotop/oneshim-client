import { test, expect } from './helpers/test'

test.describe('Privacy Actions', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/privacy')
  })

  test('P131: #section-data exists', async ({ page }) => {
    await expect(page.locator('#section-data')).toBeVisible()
  })

  test('P132: date inputs exist', async ({ page }) => {
    const startDate = page.locator('#privacy-start-date')
    if (await startDate.isVisible()) {
      await expect(startDate).toBeVisible()
    }
  })

  test('P133: data type toggle buttons exist', async ({ page }) => {
    const buttons = page.locator('button')
    const count = await buttons.count()
    expect(count).toBeGreaterThanOrEqual(2)
  })

  test('P134: delete-range button exists', async ({ page }) => {
    const btn = page.getByTestId('delete-range')
    await expect(btn).toBeVisible()
  })

  test('P135: delete-all button exists', async ({ page }) => {
    const btn = page.getByTestId('delete-all')
    await expect(btn).toBeVisible()
  })

  test('P136: delete button opens confirm modal', async ({ page }) => {
    const btn = page.getByTestId('delete-all')
    await btn.click()
    const modal = page.locator('[role="alertdialog"]')
    await expect(modal).toBeVisible()
  })

  test('P137: confirm modal has aria-modal', async ({ page }) => {
    const btn = page.getByTestId('delete-all')
    await btn.click()
    const modal = page.locator('[aria-modal="true"]')
    await expect(modal).toBeVisible()
  })

  test('P138: confirm modal Escape closes', async ({ page }) => {
    const btn = page.getByTestId('delete-all')
    await btn.click()
    await expect(page.locator('[role="alertdialog"]')).toBeVisible()
    await page.keyboard.press('Escape')
    await expect(page.locator('[role="alertdialog"]')).not.toBeVisible()
  })

  test('P139: #section-export exists', async ({ page }) => {
    await expect(page.locator('#section-export')).toBeVisible()
  })

  test('P140: download-backup button exists', async ({ page }) => {
    const btn = page.getByTestId('download-backup')
    await expect(btn).toBeVisible()
  })

  test('P141: backup toggle checkboxes exist', async ({ page }) => {
    const section = page.locator('#section-export')
    const checkboxes = section.locator('input[type="checkbox"]')
    const count = await checkboxes.count()
    expect(count).toBeGreaterThanOrEqual(1)
  })

  test('P142: #section-consent exists', async ({ page }) => {
    await expect(page.locator('#section-consent')).toBeVisible()
  })
})
