import type { Meta, StoryObj } from '@storybook/react'
import { MemoryRouter } from 'react-router-dom'
import FocusWidget from './FocusWidget'

const meta = {
  title: 'Domain Components/FocusWidget',
  component: FocusWidget,
  tags: ['autodocs'],
  decorators: [
    (Story) => (
      <MemoryRouter>
        <Story />
      </MemoryRouter>
    ),
  ],
} satisfies Meta<typeof FocusWidget>

export default meta
type Story = StoryObj<typeof meta>

/**
 * FocusWidget fetches data internally via fetchFocusMetrics.
 * In Storybook (no backend), it will display the error state
 * after the fetch fails.
 */
export const Default: Story = {}
