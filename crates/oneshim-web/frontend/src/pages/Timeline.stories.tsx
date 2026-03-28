import type { Meta, StoryObj } from '@storybook/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { createMockPaginatedFrames, createMockTags } from '../stories/mock-data'
import Timeline from './Timeline'

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, staleTime: Infinity } },
})

function createStoryQueryClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Number.POSITIVE_INFINITY } },
  })
}

const meta = {
  title: 'Pages/Timeline',
  component: Timeline,
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
} satisfies Meta<typeof Timeline>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {}

export const WithMockData: Story = {
  decorators: [
    (Story) => {
      const qc = createStoryQueryClient()
      qc.setQueryData(['frames', 0, undefined, undefined], createMockPaginatedFrames(12, 24))
      qc.setQueryData(['tags'], createMockTags())
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
      qc.setQueryData(['frames', 0, undefined, undefined], createMockPaginatedFrames(0, 0))
      qc.setQueryData(['tags'], [])
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
