import type { Meta, StoryObj } from '@storybook/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AppMemoryRouter } from '../router/future'
import { createMockTimelineResponse } from '../stories/mock-data'
import { darkThemeGlobals, lightThemeGlobals, reviewStoryParameters } from '../stories/storybook-helpers'
import SessionReplay from './SessionReplay'

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, staleTime: Infinity } },
})

function createStoryQueryClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Number.POSITIVE_INFINITY } },
  })
}

const meta = {
  title: 'Pages/SessionReplay',
  component: SessionReplay,
  tags: ['autodocs'],
  decorators: [
    (Story) => (
      <QueryClientProvider client={queryClient}>
        <AppMemoryRouter>
          <Story />
        </AppMemoryRouter>
      </QueryClientProvider>
    ),
  ],
} satisfies Meta<typeof SessionReplay>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {}

export const WithMockData: Story = {
  decorators: [
    (Story) => {
      const qc = createStoryQueryClient()
      const now = new Date()
      const oneHourAgo = new Date(now.getTime() - 3600 * 1000)
      const fromKey = oneHourAgo.toISOString().slice(0, 16)
      const toKey = now.toISOString().slice(0, 16)
      qc.setQueryData(['timeline', fromKey, toKey], createMockTimelineResponse())
      qc.setQueryData(['settings'], {
        ai_provider: {
          scene_intelligence: { enabled: true, overlay_enabled: true, allow_action_execution: false },
        },
      })
      return (
        <QueryClientProvider client={qc}>
          <AppMemoryRouter>
            <Story />
          </AppMemoryRouter>
        </QueryClientProvider>
      )
    },
  ],
}

export const EmptyState: Story = {
  decorators: [
    (Story) => {
      const qc = createStoryQueryClient()
      const now = new Date()
      const oneHourAgo = new Date(now.getTime() - 3600 * 1000)
      const fromKey = oneHourAgo.toISOString().slice(0, 16)
      const toKey = now.toISOString().slice(0, 16)
      qc.setQueryData(
        ['timeline', fromKey, toKey],
        createMockTimelineResponse({
          items: [],
          segments: [],
          session: {
            start: oneHourAgo.toISOString(),
            end: now.toISOString(),
            duration_secs: 0,
            total_events: 0,
            total_frames: 0,
            total_idle_secs: 0,
          },
        }),
      )
      return (
        <QueryClientProvider client={qc}>
          <AppMemoryRouter>
            <Story />
          </AppMemoryRouter>
        </QueryClientProvider>
      )
    },
  ],
}

export const LightReview: Story = {
  decorators: WithMockData.decorators,
  globals: lightThemeGlobals,
  parameters: reviewStoryParameters,
}

export const DarkReview: Story = {
  decorators: WithMockData.decorators,
  globals: darkThemeGlobals,
  parameters: reviewStoryParameters,
}
