import type { Meta, StoryObj } from '@storybook/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { ThemeProvider } from '../contexts/ThemeContext'
import Focus from './Focus'

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, staleTime: Infinity } },
})

const meta = {
  title: 'Pages/Focus',
  component: Focus,
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
