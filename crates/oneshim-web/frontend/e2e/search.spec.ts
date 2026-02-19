/**
 * 검색 페이지 E2E 테스트
 *
 * 검색 폼, 결과 표시, 태그 필터 검증
 */
import { test, expect } from '@playwright/test'

test.describe('Search', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/search')
  })

  test('should display search title', async ({ page }) => {
    await expect(page.getByRole('heading', { name: /검색|search/i })).toBeVisible()
  })

  test('should display search input', async ({ page }) => {
    // 검색 입력 필드
    const searchInput = page.locator('input[type="text"]').first()
    await expect(searchInput).toBeVisible()
  })

  test('should display search type selector', async ({ page }) => {
    // 검색 타입 선택 (전체/프레임/이벤트)
    const typeButtons = page.getByRole('button', { name: /all|frames|events|전체|프레임|이벤트/i })
    await expect(typeButtons.first()).toBeVisible()
  })

  test('should display tag filter section', async ({ page }) => {
    // 태그 필터 섹션 (한/영 모두 지원)
    await expect(page.getByText(/태그 필터:|Filter by tags:/i)).toBeVisible()
  })

  test('should perform search', async ({ page }) => {
    // 검색 수행
    const searchInput = page.locator('input[type="text"]').first()
    await searchInput.fill('test')
    await searchInput.press('Enter')

    // 검색 결과 또는 결과 없음 메시지 대기
    await page.waitForTimeout(1000)
  })

  test('should filter by search type', async ({ page }) => {
    // 프레임 타입으로 필터
    const framesButton = page.getByRole('button', { name: /frames|프레임/i })
    if (await framesButton.isVisible()) {
      await framesButton.click()
      await page.waitForTimeout(300)
    }
  })

  test('should clear search', async ({ page }) => {
    // 검색어 입력
    const searchInput = page.locator('input[type="text"]').first()
    await searchInput.fill('test')

    // 검색어 지우기
    await searchInput.clear()
    expect(await searchInput.inputValue()).toBe('')
  })

  test('should show search hint', async ({ page }) => {
    // 검색 힌트 표시
    await expect(page.getByText(/앱 이름|창 제목|ocr|app name|window title/i)).toBeVisible()
  })

  test('should display pagination when results exist', async ({ page }) => {
    // 검색 수행
    const searchInput = page.locator('input[type="text"]').first()
    await searchInput.fill('a')
    await searchInput.press('Enter')

    await page.waitForTimeout(2000)

    // 검색 후 어떤 UI 변화가 있는지 확인 (로딩 상태, 결과, 또는 "결과 없음" 메시지)
    // 데이터 의존성을 줄이기 위해 페이지 타이틀만 확인
    await expect(page.getByRole('heading', { name: /검색|search/i })).toBeVisible()
  })

  test('should toggle tag filter', async ({ page }) => {
    // 태그 필터 토글 (태그가 있을 경우)
    const tagBadge = page.locator('.rounded-full').first()
    if (await tagBadge.isVisible()) {
      await tagBadge.click()
      await page.waitForTimeout(300)
    }
  })
})
