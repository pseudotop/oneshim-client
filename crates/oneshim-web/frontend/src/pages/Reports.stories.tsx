import type { Meta, StoryObj } from '@storybook/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { ThemeProvider } from '../contexts/ThemeContext'
import { createMockReport } from '../stories/mock-data'
import Reports from './Reports'

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, staleTime: Infinity } },
})

function createStoryQueryClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Number.POSITIVE_INFINITY } },
  })
}

const meta = {
  title: 'Pages/Reports',
  component: Reports,
  tags: ['autodocs'],
  decorators: [
    (Story) => (
      <QueryClientProvider client={queryClient}>
        <ThemeProvider>
          <MemoryRouter>
            <Story />
          </MemoryRouter>
        </ThemeProvider>
      </QueryClientProvider>
    ),
  ],
} satisfies Meta<typeof Reports>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {}

export const WithMockData: Story = {
  decorators: [
    (Story) => {
      const qc = createStoryQueryClient()
      qc.setQueryData(['report', 'week', '', ''], createMockReport())
      return (
        <QueryClientProvider client={qc}>
          <ThemeProvider>
            <MemoryRouter>
              <Story />
            </MemoryRouter>
          </ThemeProvider>
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
        ['report', 'week', '', ''],
        createMockReport({
          total_active_secs: 0,
          total_idle_secs: 0,
          total_captures: 0,
          total_events: 0,
          daily_stats: [],
          app_stats: [],
          hourly_activity: [],
          productivity: { score: 0, active_ratio: 0, peak_hour: 0, top_app: '', trend: 0 },
        }),
      )
      return (
        <QueryClientProvider client={qc}>
          <ThemeProvider>
            <MemoryRouter>
              <Story />
            </MemoryRouter>
          </ThemeProvider>
        </QueryClientProvider>
      )
    },
  ],
}
