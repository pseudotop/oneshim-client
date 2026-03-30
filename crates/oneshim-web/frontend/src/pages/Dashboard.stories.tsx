import type { Meta, StoryObj } from '@storybook/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AppMemoryRouter } from '../router/future'
import {
  createMockHeatmapResponse,
  createMockHourlyMetrics,
  createMockProcessSnapshot,
  createMockSummary,
  createMockUpdateStatus,
} from '../stories/mock-data'
import { darkThemeGlobals, lightThemeGlobals, reviewStoryParameters } from '../stories/storybook-helpers'
import Dashboard from './Dashboard'

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, staleTime: Infinity } },
})

function createStoryQueryClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Number.POSITIVE_INFINITY } },
  })
}

const today = new Date().toISOString().split('T')[0]

const meta = {
  title: 'Pages/Dashboard',
  component: Dashboard,
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
} satisfies Meta<typeof Dashboard>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {}

export const WithMockData: Story = {
  decorators: [
    (Story) => {
      const qc = createStoryQueryClient()
      qc.setQueryData(['summary', today], createMockSummary())
      qc.setQueryData(['hourlyMetrics'], createMockHourlyMetrics())
      qc.setQueryData(['processes'], [createMockProcessSnapshot()])
      qc.setQueryData(['heatmap', 7], createMockHeatmapResponse())
      qc.setQueryData(['update-status'], createMockUpdateStatus())
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
      qc.setQueryData(
        ['summary', today],
        createMockSummary({
          total_active_secs: 0,
          total_idle_secs: 0,
          top_apps: [],
          frames_captured: 0,
          events_logged: 0,
          cpu_avg: 0,
          memory_avg_percent: 0,
        }),
      )
      qc.setQueryData(['hourlyMetrics'], [])
      qc.setQueryData(['processes'], [])
      qc.setQueryData(['heatmap', 7], createMockHeatmapResponse(7, { cells: [], max_value: 0 }))
      qc.setQueryData(['update-status'], createMockUpdateStatus({ phase: 'Idle', pending: null, message: null }))
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
