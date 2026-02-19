/**
 * Playwright E2E 테스트 설정
 *
 * 로컬 웹 대시보드 E2E 테스트 구성
 */
import { defineConfig, devices } from '@playwright/test'

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
    baseURL: 'http://localhost:9090',

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

  // 웹 서버 설정 (테스트 전 자동 시작)
  // 참고: 실제 테스트 시 Rust 서버가 실행 중이어야 함
  // webServer: {
  //   command: 'pnpm preview',
  //   url: 'http://localhost:9090',
  //   reuseExistingServer: !process.env.CI,
  // },
})
