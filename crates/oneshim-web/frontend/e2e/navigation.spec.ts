import { test, expect, type Page } from '@playwright/test'

const moreMenuButtonName = /more|더보기/i

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
    await expect(page.getByRole('link', { name: /dashboard|대시보드/i })).toBeVisible()
  })

  test('should display navigation links', async ({ page }) => {
    await expect(page.getByRole('link', { name: /dashboard|대시보드/i })).toBeVisible()
    await expect(page.getByRole('link', { name: /timeline|타임라인/i })).toBeVisible()
    await expect(page.getByRole('link', { name: /reports|리포트/i })).toBeVisible()

    await openMoreMenu(page)
    await expect(page.getByRole('menuitem', { name: /settings|설정/i })).toBeVisible()
    await expect(page.getByRole('menuitem', { name: /privacy|개인정보/i })).toBeVisible()
    await expect(page.getByRole('menuitem', { name: /search|검색/i })).toBeVisible()
  })

  test('should navigate to Dashboard', async ({ page }) => {
    await page.getByRole('link', { name: /dashboard|대시보드/i }).click()
    await expect(page).toHaveURL('/')
  })

  test('should navigate to Timeline', async ({ page }) => {
    await page.getByRole('link', { name: /timeline|타임라인/i }).click()
    await expect(page).toHaveURL('/timeline')
  })

  test('should navigate to Reports', async ({ page }) => {
    await page.getByRole('link', { name: /reports|리포트/i }).click()
    await expect(page).toHaveURL('/reports')
  })

  test('should navigate to Settings', async ({ page }) => {
    await clickMoreMenuItem(page, /settings|설정/i)
    await expect(page).toHaveURL('/settings')
  })

  test('should navigate to Privacy', async ({ page }) => {
    await clickMoreMenuItem(page, /privacy|개인정보/i)
    await expect(page).toHaveURL('/privacy')
  })

  test('should navigate to Search', async ({ page }) => {
    await clickMoreMenuItem(page, /search|검색/i)
    await expect(page).toHaveURL('/search')
  })

  test('should show keyboard shortcuts help with ? key', async ({ page }) => {
    await page.locator('body').click()
    await page.keyboard.press('Shift+Slash')

    const heading = page.getByRole('heading', {
      name: /keyboard shortcuts|키보드 단축키/i,
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
