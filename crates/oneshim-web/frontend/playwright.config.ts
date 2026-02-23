/**
 * Playwright E2E 테스트 설정
 *
 * 로컬 웹 대시보드 E2E 테스트 구성
 */
import { defineConfig, devices } from '@playwright/test'

const previewHost = process.env.PLAYWRIGHT_PREVIEW_HOST || '127.0.0.1'
const previewPort = process.env.PLAYWRIGHT_PREVIEW_PORT || '9090'
const managedBaseURL = `http://${previewHost}:${previewPort}`
const baseURL = process.env.PLAYWRIGHT_BASE_URL || managedBaseURL
const shouldManageWebServer = !process.env.PLAYWRIGHT_BASE_URL

export default defineConfig({
  // 테스트 디렉토리
  testDir: './e2e',

  // 테스트 파일 패턴
  testMatch: '**/*.spec.ts',

  // 병렬 실행
  fullyParallel: true,

  // CI에서 재시도 비활성화
  retries: process.env.CI ? 2 : 0,

  // 타임아웃
  timeout: 30000,

  // 리포터
  reporter: [['html', { open: 'never' }], ['list']],

  // 공통 설정
  use: {
    // 기본 URL (로컬 서버)
    baseURL,

    // 추적 수집 (실패 시)
    trace: 'on-first-retry',

    // 스크린샷 (실패 시)
    screenshot: 'only-on-failure',

    // 헤드리스 모드
    headless: true,
  },

  // 브라우저 프로젝트
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
    // Firefox와 Safari는 선택적으로 활성화
    // {
    //   name: 'firefox',
    //   use: { ...devices['Desktop Firefox'] },
    // },
    // {
    //   name: 'webkit',
    //   use: { ...devices['Desktop Safari'] },
    // },
  ],

  ...(shouldManageWebServer
    ? {
        webServer: {
          command: `pnpm preview --host ${previewHost} --port ${previewPort}`,
          url: managedBaseURL,
          reuseExistingServer: !process.env.CI,
        },
      }
    : {}),
})
