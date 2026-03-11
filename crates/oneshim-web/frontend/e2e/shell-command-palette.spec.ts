import { test, expect } from './helpers/test'

test.describe('CommandPalette Actions', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/')
  })

  test('P013: Cmd+K opens command palette', async ({ page }) => {
    await page.keyboard.press('Meta+k')
    await expect(page.locator('[aria-modal="true"]')).toBeVisible()
  })

  test('P014: search input filters commands', async ({ page }) => {
    await page.keyboard.press('Meta+k')
    const input = page.locator('input[role="combobox"]')
    await input.fill('settings')
    const options = page.locator('[id^="palette-option-"]')
    const count = await options.count()
    expect(count).toBeGreaterThanOrEqual(1)
    expect(count).toBeLessThan(15)
  })

  test('P015: Escape closes palette', async ({ page }) => {
    await page.keyboard.press('Meta+k')
    await expect(page.locator('[aria-modal="true"]')).toBeVisible()
    await page.keyboard.press('Escape')
    await expect(page.locator('[aria-modal="true"]')).not.toBeVisible()
  })

  test('P016: Enter on item navigates', async ({ page }) => {
    await page.keyboard.press('Meta+k')
    const input = page.locator('input[role="combobox"]')
    await input.fill('privacy')
    await page.keyboard.press('Enter')
    await expect(page).toHaveURL(/\/privacy/)
  })

  test('P017: Arrow Down selects next', async ({ page }) => {
    await page.keyboard.press('Meta+k')
    await page.keyboard.press('ArrowDown')
    const input = page.locator('input[role="combobox"]')
    const activedesc = await input.getAttribute('aria-activedescendant')
    expect(activedesc).toBeTruthy()
  })

  test('P018: focus trap keeps Tab within dialog', async ({ page }) => {
    await page.keyboard.press('Meta+k')
    for (let i = 0; i < 10; i++) {
      await page.keyboard.press('Tab')
    }
    const modal = page.locator('[aria-modal="true"]')
    const activeInModal = await modal.evaluate((m) => m.contains(document.activeElement))
    expect(activeInModal).toBe(true)
  })
})
