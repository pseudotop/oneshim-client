import type { Meta, StoryObj } from '@storybook/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { ShellLayoutProvider } from '../contexts/ShellLayoutContext'
import Settings from './Settings'

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, staleTime: Infinity } },
})

const meta = {
  title: 'Pages/Settings',
  component: Settings,
  decorators: [
    (Story) => (
      <QueryClientProvider client={queryClient}>
        <MemoryRouter>
          <ShellLayoutProvider sidebarCollapsed={false}>
            <Story />
          </ShellLayoutProvider>
        </MemoryRouter>
      </QueryClientProvider>
    ),
  ],
} satisfies Meta<typeof Settings>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {}
