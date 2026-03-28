import type { Meta, StoryObj } from '@storybook/react'
import OAuthConnectionPanel from './OAuthConnectionPanel'

const meta = {
  title: 'Settings/OAuthConnectionPanel',
  component: OAuthConnectionPanel,
  tags: ['autodocs'],
} satisfies Meta<typeof OAuthConnectionPanel>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {
  args: {
    providerId: 'google',
    providerName: 'Google',
  },
}

export const WithFeatureSnapshot: Story = {
  args: {
    providerId: 'github',
    providerName: 'GitHub',
    featureSnapshot: null,
    secretBackendCapabilities: null,
  },
}
