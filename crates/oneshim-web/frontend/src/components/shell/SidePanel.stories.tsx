import type { Meta, StoryObj } from '@storybook/react'
import { MemoryRouter } from 'react-router-dom'
import SidePanel from './SidePanel'

const meta = {
  title: 'Shell/SidePanel',
  component: SidePanel,
  decorators: [
    (Story) => (
      <MemoryRouter initialEntries={['/']}>
        <div style={{ height: 500 }} className="flex">
          <Story />
        </div>
      </MemoryRouter>
    ),
  ],
} satisfies Meta<typeof SidePanel>

export default meta
type Story = StoryObj<typeof meta>

export const DashboardPage: Story = {
  args: {
    collapsed: false,
    width: 220,
    onResizeStart: () => {},
    onResizeByKeyboard: () => {},
  },
}

export const TimelinePage: Story = {
  args: {
    collapsed: false,
    width: 220,
    onResizeStart: () => {},
    onResizeByKeyboard: () => {},
  },
  decorators: [
    (Story) => (
      <MemoryRouter initialEntries={['/timeline']}>
        <div style={{ height: 500 }} className="flex">
          <Story />
        </div>
      </MemoryRouter>
    ),
  ],
}

export const SettingsPage: Story = {
  args: {
    collapsed: false,
    width: 220,
    onResizeStart: () => {},
    onResizeByKeyboard: () => {},
  },
  decorators: [
    (Story) => (
      <MemoryRouter initialEntries={['/settings']}>
        <div style={{ height: 500 }} className="flex">
          <Story />
        </div>
      </MemoryRouter>
    ),
  ],
}

export const Collapsed: Story = {
  args: {
    collapsed: true,
    width: 220,
    onResizeStart: () => {},
    onResizeByKeyboard: () => {},
  },
}
