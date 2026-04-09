import { expect, test } from './helpers/test'

test.describe('ActivityBar Actions', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
  })

  // After clicking a nav button the URL settles on the parent's defaultChild
  // (e.g. /timeline → /timeline/all, / → /overview), so the assertion needs
  // to know the post-redirect target.
  const NAV_ITEMS = [
    // monitor group
    { id: 'dashboard', expectedUrl: /\/overview$/ },
    { id: 'timeline', expectedUrl: /\/timeline\/all$/ },
    { id: 'replay', expectedUrl: /\/replay\/timeline$/ },
    { id: 'automation', expectedUrl: /\/automation\/policies$/ },
    // data group
    { id: 'focus', expectedUrl: /\/focus\/score$/ },
    { id: 'reports', expectedUrl: /\/reports\/activity$/ },
    { id: 'search', expectedUrl: /\/search$/ },
    // manage group
    { id: 'audit', expectedUrl: /\/audit\/summary$/ },
    { id: 'policies', expectedUrl: /\/policies$/ },
    { id: 'updates', expectedUrl: /\/updates\/status$/ },
    // bottom
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

  test('P013: audit and policies are in the manage group (below second divider)', async ({ page }) => {
    // After the ActivityBar rebalance, /audit and /policies sit in the manage
    // group — the third section separated by <hr> dividers.  Verify both
    // buttons appear after the automation button (last monitor item) so the
    // visual grouping is correct.
    const nav = page.locator('nav[aria-label]')
    const buttons = nav.locator('button[data-testid]')
    const ids = await buttons.evaluateAll((nodes) => nodes.map((n) => n.getAttribute('data-testid')))

    const automationIdx = ids.indexOf('nav-automation')
    const auditIdx = ids.indexOf('nav-audit')
    const policiesIdx = ids.indexOf('nav-policies')
    const updatesIdx = ids.indexOf('nav-updates')

    // audit, policies, updates must all come after automation (monitor group ends there)
    expect(auditIdx).toBeGreaterThan(automationIdx)
    expect(policiesIdx).toBeGreaterThan(automationIdx)
    expect(updatesIdx).toBeGreaterThan(automationIdx)

    // manage group order: audit → policies → updates
    expect(policiesIdx).toBeGreaterThan(auditIdx)
    expect(updatesIdx).toBeGreaterThan(policiesIdx)
  })
})
