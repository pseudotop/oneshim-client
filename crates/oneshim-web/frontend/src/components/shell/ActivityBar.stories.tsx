import type { Meta, StoryObj } from '@storybook/react'
import { AppMemoryRouter } from '../../router/future'
import ActivityBar from './ActivityBar'

const meta = {
  title: 'Shell/ActivityBar',
  component: ActivityBar,
  tags: ['autodocs'],
  decorators: [
    (Story) => (
      <AppMemoryRouter initialEntries={['/']}>
        <div style={{ height: 600 }} className="flex">
          <Story />
        </div>
      </AppMemoryRouter>
    ),
  ],
} satisfies Meta<typeof ActivityBar>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {
  args: {
    onToggleSidebar: () => {},
    sidebarCollapsed: false,
  },
}

export const SidebarCollapsed: Story = {
  args: {
    onToggleSidebar: () => {},
    sidebarCollapsed: true,
  },
}

export const OnTimelinePage: Story = {
  args: {
    onToggleSidebar: () => {},
    sidebarCollapsed: false,
  },
  decorators: [
    (Story) => (
      <AppMemoryRouter initialEntries={['/timeline']}>
        <div style={{ height: 600 }} className="flex">
          <Story />
        </div>
      </AppMemoryRouter>
    ),
  ],
}
