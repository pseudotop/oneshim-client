import { test, expect } from './helpers/test'

test.describe('Settings Actions', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/settings')
  })

  test('P117: notification toggles exist', async ({ page }) => {
    const section = page.locator('#section-notification')
    await expect(section).toContainText('Enable Notifications')
    await expect(section).toContainText('Break Reminder')
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
    await expect(select).toBeVisible()
  })

  test('P121: prerelease toggle exists', async ({ page }) => {
    await expect(page.getByRole('checkbox', { name: /Include prerelease/i })).toBeVisible()
  })
})
