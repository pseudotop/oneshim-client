import type { Meta, StoryObj } from '@storybook/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import type { ReactNode } from 'react'
import CoachingGoalsTab from './CoachingGoalsTab'

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
})

function WithQueryClient({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
}

const meta = {
  title: 'Settings/CoachingGoalsTab',
  component: CoachingGoalsTab,
  decorators: [
    (Story) => (
      <WithQueryClient>
        <Story />
      </WithQueryClient>
    ),
  ],
} satisfies Meta<typeof CoachingGoalsTab>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {}

export const EmptyState: Story = {}
