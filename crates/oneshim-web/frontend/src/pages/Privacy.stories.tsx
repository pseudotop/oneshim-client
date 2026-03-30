import type { Meta, StoryObj } from '@storybook/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { AppMemoryRouter } from '../router/future'
import { darkThemeGlobals, lightThemeGlobals, reviewStoryParameters } from '../stories/storybook-helpers'
import Privacy from './Privacy'

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, staleTime: Infinity } },
})

const meta = {
  title: 'Pages/Privacy',
  component: Privacy,
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
} satisfies Meta<typeof Privacy>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {}

export const LightReview: Story = {
  globals: lightThemeGlobals,
  parameters: reviewStoryParameters,
}

export const DarkReview: Story = {
  globals: darkThemeGlobals,
  parameters: reviewStoryParameters,
}
