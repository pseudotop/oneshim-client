import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import * as AutomationStories from '../../pages/Automation.stories'
import * as CoachingStories from '../../pages/Coaching.stories'
import * as DashboardStories from '../../pages/Dashboard.stories'
import * as DashboardDayStories from '../../pages/DashboardDay.stories'
import * as FocusStories from '../../pages/Focus.stories'
import * as PrivacyStories from '../../pages/Privacy.stories'
import * as RecalibrationStories from '../../pages/RecalibrationPage.stories'
import * as ReportsStories from '../../pages/Reports.stories'
import * as SearchStories from '../../pages/Search.stories'
import * as SessionReplayStories from '../../pages/SessionReplay.stories'
import * as SettingsStories from '../../pages/Settings.stories'
import * as TimelineStories from '../../pages/Timeline.stories'
import * as UpdatesStories from '../../pages/Updates.stories'
import { colors } from '../../styles/tokens'
import { darkThemeGlobals, lightThemeGlobals, reviewStoryParameters } from '../storybook-helpers'
import * as DashboardWorkspaceTemplateStories from '../templates/DashboardWorkspaceTemplate.stories'
import * as DesktopShellTemplateStories from '../templates/DesktopShellTemplate.stories'
import * as SettingsWorkbenchTemplateStories from '../templates/SettingsWorkbenchTemplate.stories'

const routePageStories = [
  ['Automation', AutomationStories],
  ['Coaching', CoachingStories],
  ['Dashboard', DashboardStories],
  ['DashboardDay', DashboardDayStories],
  ['Focus', FocusStories],
  ['Privacy', PrivacyStories],
  ['RecalibrationPage', RecalibrationStories],
  ['Reports', ReportsStories],
  ['Search', SearchStories],
  ['SessionReplay', SessionReplayStories],
  ['Settings', SettingsStories],
  ['Timeline', TimelineStories],
  ['Updates', UpdatesStories],
] as const

const templateStories = [
  ['DesktopShell', DesktopShellTemplateStories],
  ['DashboardWorkspace', DashboardWorkspaceTemplateStories],
  ['SettingsWorkbench', SettingsWorkbenchTemplateStories],
] as const

describe('storybook review coverage', () => {
  it('keeps light and dark review variants for route-level pages', () => {
    for (const [name, storyModule] of routePageStories) {
      expect(storyModule.LightReview, `${name} should export LightReview`).toBeDefined()
      expect(storyModule.DarkReview, `${name} should export DarkReview`).toBeDefined()
      expect(storyModule.LightReview.globals).toEqual(lightThemeGlobals)
      expect(storyModule.DarkReview.globals).toEqual(darkThemeGlobals)
      expect(storyModule.LightReview.parameters).toEqual(reviewStoryParameters)
      expect(storyModule.DarkReview.parameters).toEqual(reviewStoryParameters)
    }
  })

  it('keeps light and dark review variants for template stories', () => {
    for (const [name, storyModule] of templateStories) {
      expect(storyModule.LightReview, `${name} should export LightReview`).toBeDefined()
      expect(storyModule.DarkReview, `${name} should export DarkReview`).toBeDefined()
      expect(storyModule.LightReview.globals).toEqual(lightThemeGlobals)
      expect(storyModule.DarkReview.globals).toEqual(darkThemeGlobals)
    }
  })

  it('keeps page title tokens and preview defaults safe for light-theme review', () => {
    expect(colors.text.pageTitle).toBe('text-content')
    expect(colors.text.pageSubtitle).toBe('text-content-secondary')

    const currentFile = fileURLToPath(import.meta.url)
    const previewPath = resolve(dirname(currentFile), '../../../.storybook/preview.ts')
    const previewSource = readFileSync(previewPath, 'utf8')
    expect(previewSource).toContain("defaultTheme: 'light'")
  })
})
