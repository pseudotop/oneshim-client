import type { Meta, StoryObj } from '@storybook/react'
import { MemoryRouter } from 'react-router-dom'
import TitleBar from './TitleBar'

const meta = {
  title: 'Shell/TitleBar',
  component: TitleBar,
  tags: ['autodocs'],
  decorators: [
    (Story) => (
      <MemoryRouter initialEntries={['/']}>
        <Story />
      </MemoryRouter>
    ),
  ],
} satisfies Meta<typeof TitleBar>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {
  args: {
    onSearchOpen: () => {},
  },
}

export const TimelinePage: Story = {
  args: {
    onSearchOpen: () => {},
  },
  decorators: [
    (Story) => (
      <MemoryRouter initialEntries={['/timeline']}>
        <Story />
      </MemoryRouter>
    ),
  ],
}

export const SettingsPage: Story = {
  args: {
    onSearchOpen: () => {},
  },
  decorators: [
    (Story) => (
      <MemoryRouter initialEntries={['/settings']}>
        <Story />
      </MemoryRouter>
    ),
  ],
}
