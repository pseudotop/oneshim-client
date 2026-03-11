import { test, expect } from './helpers/test'

test.describe('ActivityBar Actions', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
  })

  const NAV_ITEMS = [
    { id: 'dashboard', path: '/' },
    { id: 'timeline', path: '/timeline' },
    { id: 'replay', path: '/replay' },
    { id: 'automation', path: '/automation' },
    { id: 'focus', path: '/focus' },
    { id: 'reports', path: '/reports' },
    { id: 'search', path: '/search' },
    { id: 'updates', path: '/updates' },
    { id: 'settings', path: '/settings' },
    { id: 'privacy', path: '/privacy' },
  ]

  for (const item of NAV_ITEMS) {
    test(`P00x-${item.id}: nav button exists and navigates`, async ({ page }) => {
      const btn = page.getByTestId(`nav-${item.id}`)
      await expect(btn).toBeVisible()
      await btn.click()
      await expect(page).toHaveURL(new RegExp(item.path === '/' ? '/$' : item.path))
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
