import { test, expect } from './helpers/test'
import { i18nRegex } from './helpers/i18n'

const dashboardName = i18nRegex('nav.dashboard')
const timelineName = i18nRegex('nav.timeline')
const reportsName = i18nRegex('nav.reports')
const settingsName = i18nRegex('nav.settings')
const privacyName = i18nRegex('nav.privacy')
const searchName = i18nRegex('nav.search')
const shortcutsTitle = i18nRegex('shortcuts.title')

test.describe('Navigation', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
    // ActivityBar uses buttons with title attribute for navigation
    await expect(page.locator('nav[role="navigation"]')).toBeVisible()
  })

  test('should display navigation buttons in ActivityBar', async ({ page }) => {
    await expect(page.getByTitle(dashboardName)).toBeVisible()
    await expect(page.getByTitle(timelineName)).toBeVisible()
    await expect(page.getByTitle(reportsName)).toBeVisible()
    await expect(page.getByTitle(settingsName)).toBeVisible()
    await expect(page.getByTitle(privacyName)).toBeVisible()
    await expect(page.getByTitle(searchName)).toBeVisible()
  })

  test('should navigate to Dashboard', async ({ page }) => {
    await page.getByTitle(dashboardName).click()
    await expect(page).toHaveURL('/')
  })

  test('should navigate to Timeline', async ({ page }) => {
    await page.getByTitle(timelineName).click()
    await expect(page).toHaveURL('/timeline')
  })

  test('should navigate to Reports', async ({ page }) => {
    await page.getByTitle(reportsName).click()
    await expect(page).toHaveURL('/reports')
  })

  test('should navigate to Settings', async ({ page }) => {
    await page.getByTitle(settingsName).click()
    await expect(page).toHaveURL('/settings')
  })

  test('should navigate to Privacy', async ({ page }) => {
    await page.getByTitle(privacyName).click()
    await expect(page).toHaveURL('/privacy')
  })

  test('should navigate to Search', async ({ page }) => {
    await page.getByTitle(searchName).click()
    await expect(page).toHaveURL('/search')
  })

  test('should show keyboard shortcuts help with ? key', async ({ page }) => {
    await page.locator('body').click()
    await page.keyboard.press('Shift+Slash')

    const heading = page.getByRole('heading', {
      name: shortcutsTitle,
    })
    await expect(heading).toBeVisible({ timeout: 10000 })
  })

  test('should navigate with keyboard shortcuts', async ({ page }) => {
    await page.keyboard.press('d')
    await expect(page).toHaveURL('/')

    await page.keyboard.press('t')
    await expect(page).toHaveURL('/timeline')

    await page.keyboard.press('s')
    await expect(page).toHaveURL('/settings')

    await page.keyboard.press('p')
    await expect(page).toHaveURL('/privacy')
  })
})
