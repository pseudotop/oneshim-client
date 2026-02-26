import { test, expect, type Page } from './helpers/test'
import { i18nRegex } from './helpers/i18n'

const moreMenuButtonName = i18nRegex('common.more')
const dashboardName = i18nRegex('nav.dashboard')
const timelineName = i18nRegex('nav.timeline')
const reportsName = i18nRegex('nav.reports')
const settingsName = i18nRegex('nav.settings')
const privacyName = i18nRegex('nav.privacy')
const searchName = i18nRegex('nav.search')
const shortcutsTitle = i18nRegex('shortcuts.title')

async function openMoreMenu(page: Page) {
  const moreButton = page.getByRole('button', { name: moreMenuButtonName })
  await expect(moreButton).toBeVisible()
  await moreButton.click()
  await expect(page.getByRole('menu')).toBeVisible()
}

async function clickMoreMenuItem(page: Page, name: RegExp) {
  await openMoreMenu(page)
  await page.getByRole('menuitem', { name }).click()
}

test.describe('Navigation', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
    await expect(page.getByRole('link', { name: dashboardName })).toBeVisible()
  })

  test('should display navigation links', async ({ page }) => {
    await expect(page.getByRole('link', { name: dashboardName })).toBeVisible()
    await expect(page.getByRole('link', { name: timelineName })).toBeVisible()
    await expect(page.getByRole('link', { name: reportsName })).toBeVisible()

    await openMoreMenu(page)
    await expect(page.getByRole('menuitem', { name: settingsName })).toBeVisible()
    await expect(page.getByRole('menuitem', { name: privacyName })).toBeVisible()
    await expect(page.getByRole('menuitem', { name: searchName })).toBeVisible()
  })

  test('should navigate to Dashboard', async ({ page }) => {
    await page.getByRole('link', { name: dashboardName }).click()
    await expect(page).toHaveURL('/')
  })

  test('should navigate to Timeline', async ({ page }) => {
    await page.getByRole('link', { name: timelineName }).click()
    await expect(page).toHaveURL('/timeline')
  })

  test('should navigate to Reports', async ({ page }) => {
    await page.getByRole('link', { name: reportsName }).click()
    await expect(page).toHaveURL('/reports')
  })

  test('should navigate to Settings', async ({ page }) => {
    await clickMoreMenuItem(page, settingsName)
    await expect(page).toHaveURL('/settings')
  })

  test('should navigate to Privacy', async ({ page }) => {
    await clickMoreMenuItem(page, privacyName)
    await expect(page).toHaveURL('/privacy')
  })

  test('should navigate to Search', async ({ page }) => {
    await clickMoreMenuItem(page, searchName)
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
