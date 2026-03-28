import type { Meta, StoryObj } from '@storybook/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { createMockCoachingHistory, createMockGoalProgress } from '../stories/mock-data'
import Coaching from './Coaching'

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, staleTime: Infinity } },
})

function createStoryQueryClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Number.POSITIVE_INFINITY } },
  })
}

const meta = {
  title: 'Pages/Coaching',
  component: Coaching,
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
} satisfies Meta<typeof Coaching>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {}

export const WithMockData: Story = {
  decorators: [
    (Story) => {
      const qc = createStoryQueryClient()
      qc.setQueryData(['coaching-history', 50, 0], createMockCoachingHistory())
      qc.setQueryData(['goal-progress'], createMockGoalProgress())
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
      qc.setQueryData(['coaching-history', 50, 0], [])
      qc.setQueryData(['goal-progress'], [])
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
