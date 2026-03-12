import { test, expect } from './helpers/test'

test.describe('Updates Actions', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/updates')
  })

  test('P150: #section-status exists', async ({ page }) => {
    await expect(page.locator('#section-status')).toBeVisible()
  })

  test('P151: #section-history exists', async ({ page }) => {
    await expect(page.locator('#section-history')).toBeVisible()
  })

  test('P152: version display exists', async ({ page }) => {
    const body = page.locator('body')
    await expect(body).toContainText(/\d+\.\d+\.\d+/)
  })
})
