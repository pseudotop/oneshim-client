import type { Meta, StoryObj } from '@storybook/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import CoachingGoalsTab from './CoachingGoalsTab'

function createStoryQueryClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Number.POSITIVE_INFINITY, refetchOnWindowFocus: false } },
  })
}

const meta = {
  title: 'Settings/CoachingGoalsTab',
  component: CoachingGoalsTab,
  tags: ['autodocs'],
} satisfies Meta<typeof CoachingGoalsTab>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {
  decorators: [
    (Story) => {
      const qc = createStoryQueryClient()
      return (
        <QueryClientProvider client={qc}>
          <Story />
        </QueryClientProvider>
      )
    },
  ],
}

export const WithGoals: Story = {
  decorators: [
    (Story) => {
      const qc = createStoryQueryClient()
      qc.setQueryData(
        ['goal-progress'],
        [
          { regime_label: 'Deep Work', target_minutes: 120, actual_minutes: 85, progress_pct: 70.8 },
          { regime_label: 'Communication', target_minutes: 60, actual_minutes: 42, progress_pct: 70.0 },
          { regime_label: 'Learning', target_minutes: 30, actual_minutes: 30, progress_pct: 100.0 },
        ],
      )
      return (
        <QueryClientProvider client={qc}>
          <Story />
        </QueryClientProvider>
      )
    },
  ],
}

export const EmptyState: Story = {
  decorators: [
    (Story) => {
      const qc = createStoryQueryClient()
      qc.setQueryData(['goal-progress'], [])
      return (
        <QueryClientProvider client={qc}>
          <Story />
        </QueryClientProvider>
      )
    },
  ],
}
