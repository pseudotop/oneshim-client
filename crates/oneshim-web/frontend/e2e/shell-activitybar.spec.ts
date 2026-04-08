import { expect, test } from './helpers/test'

test.describe('ActivityBar Actions', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
  })

  // After clicking a nav button the URL settles on the parent's defaultChild
  // (e.g. /timeline → /timeline/all, / → /overview), so the assertion needs
  // to know the post-redirect target.
  const NAV_ITEMS = [
    { id: 'dashboard', expectedUrl: /\/overview$/ },
    { id: 'timeline', expectedUrl: /\/timeline\/all$/ },
    { id: 'replay', expectedUrl: /\/replay\/timeline$/ },
    { id: 'automation', expectedUrl: /\/automation\/policies$/ },
    { id: 'focus', expectedUrl: /\/focus\/score$/ },
    { id: 'reports', expectedUrl: /\/reports\/activity$/ },
    { id: 'search', expectedUrl: /\/search$/ },
    { id: 'updates', expectedUrl: /\/updates\/status$/ },
    { id: 'settings', expectedUrl: /\/settings\/general$/ },
    { id: 'privacy', expectedUrl: /\/privacy\/data$/ },
  ]

  for (const item of NAV_ITEMS) {
    test(`P00x-${item.id}: nav button exists and navigates`, async ({ page }) => {
      const btn = page.getByTestId(`nav-${item.id}`)
      await expect(btn).toBeVisible()
      await btn.click()
      await expect(page).toHaveURL(item.expectedUrl)
    })
  }

  test('P011: only one button has aria-current="page"', async ({ page }) => {
    await page.getByTestId('nav-timeline').click()
    const activeButtons = page.locator('nav button[aria-current="page"]')
    await expect(activeButtons).toHaveCount(1)
  })

  test('P012: tooltip appears on hover', async ({ page }) => {
    const btn = page.getByTestId('nav-settings')
    await btn.hover()
    const tooltip = page.locator('#activity-bar-tooltip')
    await expect(tooltip).toBeVisible()
    await expect(tooltip).toHaveText(/.+/)
  })
})
