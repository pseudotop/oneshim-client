import type { Meta, StoryObj } from '@storybook/react'
import { MemoryRouter } from 'react-router-dom'
import { darkThemeGlobals, lightThemeGlobals, reviewStoryParameters } from '../stories/storybook-helpers'
import Chat from './Chat'

const meta = {
  title: 'Pages/Chat',
  component: Chat,
  tags: ['autodocs'],
  decorators: [
    (Story) => (
      <MemoryRouter>
        <div style={{ height: '600px' }}>
          <Story />
        </div>
      </MemoryRouter>
    ),
  ],
} satisfies Meta<typeof Chat>

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
