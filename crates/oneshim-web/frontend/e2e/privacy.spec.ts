import { test, expect, type Page } from './helpers/test'
import { i18nRegex } from './helpers/i18n'
import { mockStaticJson } from './helpers/mock-api'

const privacyTitleName = i18nRegex('privacy.title')
const currentDataName = i18nRegex('privacy.currentData')
const deleteByRangeName = i18nRegex('privacy.deleteByRangeTitle')
const deleteAllTitleName = i18nRegex('privacy.deleteAllTitle')
const deleteAllButtonName = i18nRegex('privacy.deleteAllButton')
const confirmDeleteAllName = i18nRegex('privacy.confirmDeleteAll')
const backupTitleName = i18nRegex('backup.title')
const backupIncludeDataName = i18nRegex('backup.includeData')
const backupSettingsName = i18nRegex('backup.settings')
const backupDownloadName = i18nRegex('backup.download')
const backupRestoreName = i18nRegex('backup.restore')
const dataInfoTitleName = i18nRegex('privacy.dataInfoTitle')
const dataTypesButtonName = i18nRegex([
  'privacy.dataTypes.events',
  'privacy.dataTypes.frames',
  'privacy.dataTypes.metrics',
])

const mockedStorageStats = {
  db_size_bytes: 10485760,
  frames_size_bytes: 7340032,
  total_size_bytes: 17825792,
  frame_count: 128,
  event_count: 342,
  metric_count: 88,
  oldest_data_date: '2026-02-01T00:00:00Z',
  newest_data_date: '2026-02-23T00:00:00Z',
}

const mockedDeleteResult = {
  success: true,
  events_deleted: 0,
  frames_deleted: 0,
  metrics_deleted: 0,
  process_snapshots_deleted: 0,
  idle_periods_deleted: 0,
  message: 'ok',
}

async function mockPrivacyApis(page: Page) {
  await mockStaticJson(page, '**/api/storage/stats**', mockedStorageStats)
  await mockStaticJson(page, '**/api/data/range', mockedDeleteResult)
  await mockStaticJson(page, '**/api/data/all', mockedDeleteResult)
  await mockStaticJson(page, '**/api/backup/restore', {
    success: true,
    restored: {
      settings: true,
      tags: 0,
      frame_tags: 0,
      events: 0,
      frames: 0,
    },
    errors: [],
  })
  await mockStaticJson(page, '**/api/backup**', { ok: true })
}

test.describe('Privacy', () => {
  test.beforeEach(async ({ page }) => {
    await mockPrivacyApis(page)
    await page.goto('/privacy')
    await expect(page.getByRole('heading', { name: privacyTitleName })).toBeVisible({ timeout: 10000 })
  })

  test('should display privacy title', async ({ page }) => {
    await expect(page.getByRole('heading', { name: privacyTitleName })).toBeVisible()
  })

  test('should display storage statistics', async ({ page }) => {
    await expect(page.getByText(currentDataName)).toBeVisible()
  })

  test('should display date range delete section', async ({ page }) => {
    await expect(page.getByText(deleteByRangeName)).toBeVisible()
  })

  test('should display date inputs for range delete', async ({ page }) => {
    const dateInputs = page.locator('input[type="date"]')
    await expect(dateInputs.first()).toBeVisible()
    await expect(dateInputs.nth(1)).toBeVisible()
  })

  test('should display data type selection buttons', async ({ page }) => {
    await expect(page.getByRole('button', { name: dataTypesButtonName }).first()).toBeVisible()
  })

  test('should display delete all data section', async ({ page }) => {
    await expect(page.getByRole('heading', { name: deleteAllTitleName }).first()).toBeVisible()
  })

  test('should display backup section and options', async ({ page }) => {
    await expect(page.getByText(backupTitleName)).toBeVisible()
    await expect(page.getByText(backupIncludeDataName)).toBeVisible()
  })

  test('should display backup action buttons', async ({ page }) => {
    await expect(page.getByRole('button', { name: backupDownloadName })).toBeVisible()
    await expect(page.getByRole('button', { name: backupRestoreName })).toBeVisible()
  })

  test('should display data collection info', async ({ page }) => {
    await expect(page.getByText(dataInfoTitleName)).toBeVisible()
  })

  test('should toggle backup option', async ({ page }) => {
    const settingsOption = page.getByRole('button', { name: backupSettingsName }).first()
    await settingsOption.click()
    await expect(settingsOption).toBeVisible()
  })

  test('should show confirmation modal for delete all', async ({ page }) => {
    await page.getByRole('button', { name: deleteAllButtonName }).first().click()
    const deleteAllHeadings = page.getByRole('heading', { name: confirmDeleteAllName })
    await expect(deleteAllHeadings).toHaveCount(2)
    await expect(deleteAllHeadings.nth(1)).toBeVisible()
  })
})
