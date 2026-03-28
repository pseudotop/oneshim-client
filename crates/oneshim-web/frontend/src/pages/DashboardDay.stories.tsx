import type { Meta, StoryObj } from '@storybook/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { createMockDailyDigest } from '../stories/mock-data'
import DashboardDay from './DashboardDay'

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
  title: 'Pages/DashboardDay',
  component: DashboardDay,
  tags: ['autodocs'],
  decorators: [
    (Story) => (
      <QueryClientProvider client={queryClient}>
        <MemoryRouter>
          <Story />
        </MemoryRouter>
      </QueryClientProvider>
    ),
  ],
} satisfies Meta<typeof DashboardDay>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {}

export const WithMockData: Story = {
  decorators: [
    (Story) => {
      const qc = createStoryQueryClient()
      qc.setQueryData(['dashboard-day', today], createMockDailyDigest())
      qc.setQueryData(['overrides', `${today}T00:00:00Z`, `${today}T23:59:59Z`], [])
      return (
        <QueryClientProvider client={qc}>
          <MemoryRouter>
            <Story />
          </MemoryRouter>
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
        ['dashboard-day', today],
        createMockDailyDigest({
          insight: null,
          timeline: [],
          statistics: {
            deep_work_hours: 0,
            communication_hours: 0,
            meeting_hours: 0,
            context_switches: 0,
            longest_focus_mins: 0,
            longest_focus_content: '',
            regime_distribution: {},
          },
        }),
      )
      qc.setQueryData(['overrides', `${today}T00:00:00Z`, `${today}T23:59:59Z`], [])
      return (
        <QueryClientProvider client={qc}>
          <MemoryRouter>
            <Story />
          </MemoryRouter>
        </QueryClientProvider>
      )
    },
  ],
}
