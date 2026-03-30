import type { Meta, StoryObj } from '@storybook/react'
import type { QueryClient } from '@tanstack/react-query'
import {
  createMockDesktopPermissionSnapshot,
  createMockStorageStats,
  createMockUpdateStatus,
} from '../stories/mock-data'
import {
  darkThemeGlobals,
  lightThemeGlobals,
  reviewStoryParameters,
  withStoryProviders,
} from '../stories/storybook-helpers'
import Settings from './Settings'
import { makeDefaultFormData } from './setting-tabs/stories-utils'

function seedSettings(client: QueryClient) {
  client.setQueryData(['settings'], makeDefaultFormData())
  client.setQueryData(['storage-stats'], createMockStorageStats())
  client.setQueryData(['update-status'], createMockUpdateStatus())
  client.setQueryData(
    ['desktop-permission-status'],
    createMockDesktopPermissionSnapshot({
      notifications: {
        state: 'granted',
        status_reason: 'macos_notifications_granted',
      },
    }),
  )
}

const meta = {
  title: 'Pages/Settings',
  component: Settings,
  tags: ['autodocs'],
  decorators: [
    withStoryProviders({
      initialEntries: ['/settings'],
      seedQuery: seedSettings,
      withShellLayout: true,
    }),
  ],
} satisfies Meta<typeof Settings>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {}

export const LightReview: Story = {
  globals: lightThemeGlobals,
  parameters: reviewStoryParameters,
}

export const DarkReview: Story = {
  globals: darkThemeGlobals,
  parameters: reviewStoryParameters,
}
