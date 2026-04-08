import { i18nRegex } from './helpers/i18n'
import { expect, test } from './helpers/test'

const notificationEnabledName = i18nRegex('settings.notifEnabled')
const notificationIdleName = i18nRegex('settings.notifIdle')
const languageSelectorName = i18nRegex('settings.language')
// update channel selector tested via #update-channel id (no i18n lookup needed)

test.describe('Settings Actions', () => {
  test('P117: notification toggles exist', async ({ page }) => {
    await page.goto('/settings/general')
    const section = page.locator('#section-notification')
    await expect(section).toContainText(notificationEnabledName)
    await expect(section).toContainText(notificationIdleName)
  })

  test('P118: monitor interval input exists', async ({ page }) => {
    await page.goto('/settings/monitoring')
    const input = page.locator('#settings-metrics-interval')
    await expect(input).toBeVisible()
  })

  test('P119: save button exists', async ({ page }) => {
    await page.goto('/settings')
    const btn = page.getByTestId('settings-save')
    await expect(btn).toBeVisible()
  })

  test('P120: language selector exists', async ({ page }) => {
    await page.goto('/settings/general')
    await expect(page.getByLabel(languageSelectorName)).toBeVisible()
  })

  test('P121: update channel selector exists', async ({ page }) => {
    await page.goto('/settings/general')
    await expect(page.locator('#update-channel')).toBeVisible()
  })
})
