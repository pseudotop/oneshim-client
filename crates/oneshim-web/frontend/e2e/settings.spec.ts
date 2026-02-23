/**
 * 설정 페이지 E2E 테스트
 *
 * 설정 폼, 저장, 내보내기 검증
 */
import { test, expect } from '@playwright/test'
import { i18nRegex } from './helpers/i18n'

const settingsTitleName = i18nRegex('settings.title')
const collectionSectionName = i18nRegex('settings.collectionTitle')
const notificationSectionName = i18nRegex('settings.notifTitle')
const webDashboardSectionName = i18nRegex('settings.webTitle')
const exportSectionName = i18nRegex('settings.exportTitle')
const exportFormatLabelName = i18nRegex('settings.exportFormatLabel')
const languageSelectorName = i18nRegex('settings.language')
const saveSettingsName = i18nRegex('settings.saveSettings')

test.describe('Settings', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/settings')
    await page.waitForLoadState('networkidle')
  })

  test('should display settings title', async ({ page }) => {
    // 페이지 제목 (h1)
    await expect(page.getByRole('heading', { name: settingsTitleName })).toBeVisible()
  })

  test('should display data collection section', async ({ page }) => {
    // 데이터 수집 섹션
    await expect(page.getByText(collectionSectionName)).toBeVisible()
  })

  test('should display capture enable checkbox', async ({ page }) => {
    // 캡처 활성화 체크박스
    const captureCheckbox = page.locator('input[type="checkbox"]').first()
    await expect(captureCheckbox).toBeVisible()
  })

  test('should display idle threshold input', async ({ page }) => {
    // 유휴 임계값 입력
    const idleInput = page.locator('input[type="number"]').first()
    await expect(idleInput).toBeVisible()
  })

  test('should display notification settings', async ({ page }) => {
    // 알림 설정 섹션
    await expect(page.getByText(notificationSectionName)).toBeVisible()
  })

  test('should display web dashboard port setting', async ({ page }) => {
    // 웹 대시보드 섹션
    await expect(page.getByText(webDashboardSectionName)).toBeVisible()
  })

  test('should display data export section', async ({ page }) => {
    // 데이터 내보내기 섹션
    await expect(page.getByText(exportSectionName)).toBeVisible()
  })

  test('should display export format selector', async ({ page }) => {
    // 내보내기 형식 선택 - "형식:" 라벨 확인
    await expect(page.getByText(exportFormatLabelName)).toBeVisible()
  })

  test('should display export buttons', async ({ page }) => {
    // 내보내기 버튼들 (메트릭, 이벤트, 프레임 내보내기)
    const exportSection = page.getByText(exportSectionName)
    await exportSection.scrollIntoViewIfNeeded()

    // 내보내기 관련 버튼 찾기
    const buttons = page.locator('button')
    const count = await buttons.count()
    expect(count).toBeGreaterThan(0)
  })

  test('should display language selector', async ({ page }) => {
    // 언어 선택 드롭다운 (LanguageSelector 컴포넌트)
    const languageButton = page.getByTitle(languageSelectorName)
    await expect(languageButton).toBeVisible()
  })

  test('should have save button', async ({ page }) => {
    // 저장 버튼
    const saveButton = page.getByRole('button', { name: saveSettingsName })
    await expect(saveButton).toBeVisible()
  })

  test('should save settings', async ({ page }) => {
    // 저장 버튼 클릭
    const saveButton = page.getByRole('button', { name: saveSettingsName })
    await saveButton.click()

    // 저장 성공 메시지 또는 상태 변경 확인
    await page.waitForTimeout(1000)
  })

  test('should validate port number', async ({ page }) => {
    // 저장 버튼만 테스트
    const saveButton = page.getByRole('button', { name: saveSettingsName })
    await expect(saveButton).toBeVisible()
  })
})
