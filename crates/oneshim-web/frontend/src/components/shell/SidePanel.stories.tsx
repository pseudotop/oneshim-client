import type { Meta, StoryObj } from '@storybook/react'
import { AppMemoryRouter } from '../../router/future'
import SidePanel from './SidePanel'

const meta = {
  title: 'Shell/SidePanel',
  component: SidePanel,
  tags: ['autodocs'],
  decorators: [
    (Story) => (
      <AppMemoryRouter initialEntries={['/']}>
        <div style={{ height: 500 }} className="flex">
          <Story />
        </div>
      </AppMemoryRouter>
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
      <AppMemoryRouter initialEntries={['/timeline']}>
        <div style={{ height: 500 }} className="flex">
          <Story />
        </div>
      </AppMemoryRouter>
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
      <AppMemoryRouter initialEntries={['/settings']}>
        <div style={{ height: 500 }} className="flex">
          <Story />
        </div>
      </AppMemoryRouter>
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
