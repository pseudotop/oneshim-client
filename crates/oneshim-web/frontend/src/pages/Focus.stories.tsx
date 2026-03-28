import type { Meta, StoryObj } from '@storybook/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { ThemeProvider } from '../contexts/ThemeContext'
import { createMockFocusMetricsResponse, createMockInterruptions, createMockWorkSessions } from '../stories/mock-data'
import Focus from './Focus'

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, staleTime: Infinity } },
})

function createStoryQueryClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Number.POSITIVE_INFINITY } },
  })
}

const meta = {
  title: 'Pages/Focus',
  component: Focus,
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
} satisfies Meta<typeof Focus>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {}

export const WithMockData: Story = {
  decorators: [
    (Story) => {
      const qc = createStoryQueryClient()
      const weekAgo = new Date()
      weekAgo.setDate(weekAgo.getDate() - 7)
      const now = new Date()
      const fromIso = new Date(`${weekAgo.toISOString().split('T')[0]}T00:00:00Z`).toISOString()
      const toIso = new Date(`${now.toISOString().split('T')[0]}T23:59:59Z`).toISOString()

      qc.setQueryData(['focusMetrics'], createMockFocusMetricsResponse())
      qc.setQueryData(['workSessions', fromIso, toIso], createMockWorkSessions())
      qc.setQueryData(['interruptions', fromIso, toIso], createMockInterruptions())
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
      const weekAgo = new Date()
      weekAgo.setDate(weekAgo.getDate() - 7)
      const now = new Date()
      const fromIso = new Date(`${weekAgo.toISOString().split('T')[0]}T00:00:00Z`).toISOString()
      const toIso = new Date(`${now.toISOString().split('T')[0]}T23:59:59Z`).toISOString()

      qc.setQueryData(
        ['focusMetrics'],
        createMockFocusMetricsResponse({
          today: {
            date: new Date().toISOString().split('T')[0],
            total_active_secs: 0,
            deep_work_secs: 0,
            communication_secs: 0,
            context_switches: 0,
            interruption_count: 0,
            avg_focus_duration_secs: 0,
            max_focus_duration_secs: 0,
            focus_score: 0,
          },
          history: [],
        }),
      )
      qc.setQueryData(['workSessions', fromIso, toIso], [])
      qc.setQueryData(['interruptions', fromIso, toIso], [])
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
