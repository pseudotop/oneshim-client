/**
 *
 */
import { defineConfig, devices } from '@playwright/test'

const previewHost = process.env.PLAYWRIGHT_PREVIEW_HOST || '127.0.0.1'
const previewPort = process.env.PLAYWRIGHT_PREVIEW_PORT || '9090'
const managedBaseURL = `http://${previewHost}:${previewPort}`
const baseURL = process.env.PLAYWRIGHT_BASE_URL || managedBaseURL
const shouldManageWebServer = !process.env.PLAYWRIGHT_BASE_URL

export default defineConfig({
  testDir: './e2e',

  testMatch: '**/*.spec.ts',

  fullyParallel: true,

  retries: process.env.CI ? 2 : 0,

  timeout: 30000,

  reporter: [['list'], ['html', { open: 'never', outputFolder: 'playwright-report' }]],

  outputDir: 'test-results',

  use: {
    baseURL,

    trace: 'on-first-retry',

    screenshot: 'only-on-failure',

    headless: true,
  },

  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
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
