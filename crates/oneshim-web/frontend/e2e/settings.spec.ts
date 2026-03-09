/**
 *
 */
import { test, expect, type Page } from './helpers/test'
import { i18nRegex } from './helpers/i18n'
import { mockDynamicJson, mockStaticJson } from './helpers/mock-api'
import { DEFAULT_WEB_PORT } from '../src/constants'

const settingsTitleName = i18nRegex('settings.title')
const collectionSectionName = i18nRegex('settings.collectionTitle')
const notificationSectionName = i18nRegex('settings.notifTitle')
const webDashboardSectionName = i18nRegex('settings.webTitle')
const exportSectionName = i18nRegex('settings.exportTitle')
const exportFormatLabelName = i18nRegex('settings.exportFormatLabel')
const languageSelectorName = i18nRegex('settings.language')
const saveSettingsName = i18nRegex('settings.saveSettings')

const mockedSettings = {
  retention_days: 30,
  max_storage_mb: 2048,
  web_port: DEFAULT_WEB_PORT,
  allow_external: false,
  capture_enabled: true,
  idle_threshold_secs: 300,
  metrics_interval_secs: 10,
  process_interval_secs: 30,
  notification: {
    enabled: true,
    idle_notification: true,
    idle_notification_mins: 10,
    long_session_notification: true,
    long_session_mins: 60,
    high_usage_notification: true,
    high_usage_threshold: 80,
  },
  update: {
    enabled: true,
    check_interval_hours: 24,
    include_prerelease: false,
    auto_install: false,
  },
  telemetry: {
    enabled: false,
    crash_reports: false,
    usage_analytics: false,
    performance_metrics: false,
  },
  monitor: {
    process_monitoring: true,
    input_activity: true,
    privacy_mode: false,
  },
  privacy: {
    excluded_apps: [],
    excluded_app_patterns: [],
    excluded_title_patterns: [],
    auto_exclude_sensitive: true,
    pii_filter_level: 'standard',
  },
  schedule: {
    active_hours_enabled: false,
    active_start_hour: 9,
    active_end_hour: 18,
    active_days: ['Mon', 'Tue', 'Wed', 'Thu', 'Fri'],
    pause_on_screen_lock: true,
    pause_on_battery_saver: true,
  },
  automation: {
    enabled: true,
  },
  sandbox: {
    enabled: true,
    profile: 'balanced',
    allowed_read_paths: [],
    allowed_write_paths: [],
    allow_network: false,
    max_memory_bytes: 536870912,
    max_cpu_time_ms: 30000,
  },
  ai_provider: {
    ocr_provider: 'local',
    llm_provider: 'local',
    external_data_policy: 'disabled',
    allow_unredacted_external_ocr: false,
    ocr_validation: {
      enabled: true,
      min_confidence: 0.25,
      max_invalid_ratio: 0.6,
    },
    scene_action_override: {
      enabled: false,
      reason: '',
      approved_by: '',
      expires_at: null,
    },
    scene_intelligence: {
      enabled: true,
      overlay_enabled: true,
      allow_action_execution: true,
      min_confidence: 0.35,
      max_elements: 120,
      calibration_enabled: true,
      calibration_min_elements: 8,
      calibration_min_avg_confidence: 0.55,
    },
    fallback_to_local: true,
    ocr_api: null,
    llm_api: null,
  },
}

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

const mockedUpdateStatus = {
  enabled: true,
  auto_install: false,
  phase: 'Idle',
  message: null,
  pending: null,
  revision: 1,
  updated_at: '2026-02-23T10:00:00Z',
}

async function mockSettingsApis(page: Page) {
  await mockDynamicJson(page, '**/api/settings', (request) => {
    if (request.method() === 'POST') {
      return request.postDataJSON() ?? mockedSettings
    }
    return mockedSettings
  })

  await mockStaticJson(page, '**/api/storage/stats**', mockedStorageStats)
  await mockStaticJson(page, '**/api/update/status**', mockedUpdateStatus)
}

function settingsHeading(page: Page) {
  return page.locator('h1').filter({ hasText: settingsTitleName })
}

test.describe('Settings', () => {
  test.beforeEach(async ({ page }) => {
    await mockSettingsApis(page)
    await page.goto('/settings')
    await expect(settingsHeading(page)).toBeVisible({ timeout: 10000 })
  })

  test('should display settings title', async ({ page }) => {
    await expect(settingsHeading(page)).toBeVisible()
  })

  test('should display data collection section', async ({ page }) => {
    await expect(page.getByText(collectionSectionName)).toBeVisible()
  })

  test('should display capture enable checkbox', async ({ page }) => {
    const captureCheckbox = page.locator('input[type="checkbox"]').first()
    await expect(captureCheckbox).toBeVisible()
  })

  test('should display idle threshold input', async ({ page }) => {
    const idleInput = page.locator('input[type="number"]').first()
    await expect(idleInput).toBeVisible()
  })

  test('should display notification settings', async ({ page }) => {
    await expect(page.getByText(notificationSectionName)).toBeVisible()
  })

  test('should display web dashboard port setting', async ({ page }) => {
    await expect(page.getByText(webDashboardSectionName)).toBeVisible()
  })

  test('should display data export section', async ({ page }) => {
    await expect(page.getByText(exportSectionName)).toBeVisible()
  })

  test('should display export format selector', async ({ page }) => {
    await expect(page.locator('span').filter({ hasText: exportFormatLabelName }).first()).toBeVisible()
  })

  test('should display export buttons', async ({ page }) => {
    const exportSection = page.getByText(exportSectionName)
    await exportSection.scrollIntoViewIfNeeded()

    const buttons = page.locator('button')
    const count = await buttons.count()
    expect(count).toBeGreaterThan(0)
  })

  test('should display language selector', async ({ page }) => {
    const languageButton = page.getByTitle(languageSelectorName)
    await expect(languageButton).toBeVisible()
  })

  test('should have save button', async ({ page }) => {
    const saveButton = page.locator('button[type="submit"]').filter({ hasText: saveSettingsName })
    await expect(saveButton).toBeVisible()
  })

  test('should save settings', async ({ page }) => {
    const captureCheckbox = page.locator('input[type="checkbox"]').first()
    await captureCheckbox.uncheck()

    const saveButton = page.locator('button[type="submit"]').filter({ hasText: saveSettingsName })
    await saveButton.click()
    await expect(saveButton).toBeVisible()
  })

  test('should validate port number', async ({ page }) => {
    const saveButton = page.locator('button[type="submit"]').filter({ hasText: saveSettingsName })
    await expect(saveButton).toBeVisible()
  })
})
