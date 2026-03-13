import { i18nRegex } from './helpers/i18n'
import { expect, type Page, test } from './helpers/test'

const generalTabName = i18nRegex('settings.tabs.general')
const monitoringTabName = i18nRegex('settings.tabs.monitoring')
const notificationEnabledName = i18nRegex('settings.notifEnabled')
const notificationIdleName = i18nRegex('settings.notifIdle')
const languageSelectorName = i18nRegex('settings.language')
const prereleaseToggleName = i18nRegex('settings.updateIncludePrerelease')

async function openSettingsTab(page: Page, tabName: RegExp) {
  const tab = page.getByRole('tab', { name: tabName })
  await tab.click()
  await expect(tab).toHaveAttribute('aria-selected', 'true')
}

test.describe('Settings Actions', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/settings')
  })

  test('P117: notification toggles exist', async ({ page }) => {
    await openSettingsTab(page, generalTabName)
    const section = page.locator('#section-notification')
    await expect(section).toContainText(notificationEnabledName)
    await expect(section).toContainText(notificationIdleName)
  })

  test('P118: monitor interval input exists', async ({ page }) => {
    await openSettingsTab(page, monitoringTabName)
    const input = page.locator('#settings-metrics-interval')
    await expect(input).toBeVisible()
  })

  test('P119: save button exists', async ({ page }) => {
    const btn = page.getByTestId('settings-save')
    await expect(btn).toBeVisible()
  })

  test('P120: language selector exists', async ({ page }) => {
    await openSettingsTab(page, generalTabName)
    await expect(page.getByTitle(languageSelectorName)).toBeVisible()
  })

  test('P121: prerelease toggle exists', async ({ page }) => {
    await openSettingsTab(page, generalTabName)
    await expect(page.getByRole('checkbox', { name: prereleaseToggleName })).toBeVisible()
  })
})
