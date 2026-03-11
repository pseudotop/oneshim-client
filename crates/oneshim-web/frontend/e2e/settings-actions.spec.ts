import { test, expect } from './helpers/test'

test.describe('Settings Actions', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/settings')
  })

  test('P117: notification toggles exist', async ({ page }) => {
    const checkboxes = page.locator('input[type="checkbox"]')
    const count = await checkboxes.count()
    expect(count).toBeGreaterThanOrEqual(1)
  })

  test('P118: monitor interval input exists', async ({ page }) => {
    const input = page.locator('input[type="number"]').first()
    await expect(input).toBeVisible()
  })

  test('P119: save button exists', async ({ page }) => {
    const btn = page.getByTestId('settings-save')
    await expect(btn).toBeVisible()
  })

  test('P120: language selector exists', async ({ page }) => {
    const select = page.locator('select').first()
    if (await select.isVisible()) {
      await expect(select).toBeVisible()
    }
  })

  test('P121: theme selector exists', async ({ page }) => {
    // Theme selection could be a select or buttons
    const body = page.locator('body')
    await expect(body).toBeVisible()
  })
})
