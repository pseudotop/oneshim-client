import type { Meta, StoryObj } from '@storybook/react'
import { MemoryRouter } from 'react-router-dom'
import { ThemeProvider } from '../../contexts/ThemeContext'
import CommandPalette from './CommandPalette'

const meta = {
  title: 'Shell/CommandPalette',
  component: CommandPalette,
  tags: ['autodocs'],
  parameters: {
    layout: 'fullscreen',
  },
  decorators: [
    (Story) => (
      <ThemeProvider>
        <MemoryRouter initialEntries={['/']}>
          <Story />
        </MemoryRouter>
      </ThemeProvider>
    ),
  ],
} satisfies Meta<typeof CommandPalette>

export default meta
type Story = StoryObj<typeof meta>

export const Open: Story = {
  args: {
    isOpen: true,
    onClose: () => {},
    onToggleSidebar: () => {},
  },
}

export const Closed: Story = {
  args: {
    isOpen: false,
    onClose: () => {},
    onToggleSidebar: () => {},
  },
}
