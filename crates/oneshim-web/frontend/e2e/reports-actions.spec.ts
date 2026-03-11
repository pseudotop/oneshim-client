import { test, expect } from './helpers/test'

test.describe('Reports Actions', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/reports')
  })

  test('P069: period-week button exists', async ({ page }) => {
    const btn = page.getByTestId('period-week')
    await expect(btn).toBeVisible()
  })

  test('P070: period-month button exists', async ({ page }) => {
    const btn = page.getByTestId('period-month')
    await expect(btn).toBeVisible()
  })

  test('P071: period-custom button exists', async ({ page }) => {
    const btn = page.getByTestId('period-custom')
    await expect(btn).toBeVisible()
  })

  test('P072: generate-report button exists', async ({ page }) => {
    const btn = page.getByTestId('generate-report')
    await expect(btn).toBeVisible()
  })

  test('P073: clicking period button changes active state', async ({ page }) => {
    const btn = page.getByTestId('period-month')
    await btn.click()
    // Verify active state via aria-pressed or data-active attribute
    const isActive = await btn.getAttribute('aria-pressed') === 'true'
      || await btn.getAttribute('data-active') === 'true'
      || await btn.evaluate(el => el.classList.contains('active'))
    expect(isActive).toBeTruthy()
  })

  test('P074: #section-activity exists', async ({ page }) => {
    await expect(page.locator('#section-activity')).toBeVisible()
  })

  test('P075: #section-focus exists', async ({ page }) => {
    await expect(page.locator('#section-focus')).toBeVisible()
  })
})
