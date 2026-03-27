import type { Meta, StoryObj } from '@storybook/react'
import MonitoringTab from './MonitoringTab'
import { makeDefaultFormData } from './stories-utils'

const meta = {
  title: 'Settings/MonitoringTab',
  component: MonitoringTab,
} satisfies Meta<typeof MonitoringTab>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {
  args: {
    formData: makeDefaultFormData(),
    onRootChange: () => {},
    onMonitorChange: () => {},
  },
}

export const CaptureDisabled: Story = {
  args: {
    formData: makeDefaultFormData({ capture_enabled: false }),
    onRootChange: () => {},
    onMonitorChange: () => {},
  },
}

export const PrivacyModeOn: Story = {
  args: {
    formData: makeDefaultFormData({
      monitor: { process_monitoring: true, input_activity: false, privacy_mode: true },
    }),
    onRootChange: () => {},
    onMonitorChange: () => {},
  },
}
