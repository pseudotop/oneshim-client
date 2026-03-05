import type { Meta, StoryObj } from '@storybook/react'
import { Input } from './Input'

const meta = {
  title: 'UI Primitives/Input',
  component: Input,
  argTypes: {
    inputSize: {
      control: 'select',
      options: ['sm', 'md', 'lg'],
    },
    error: { control: 'boolean' },
    disabled: { control: 'boolean' },
  },
  args: {
    placeholder: 'Enter text...',
  },
} satisfies Meta<typeof Input>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {
  args: { inputSize: 'md' },
}

export const Small: Story = {
  args: { inputSize: 'sm', placeholder: 'Small input' },
}

export const Large: Story = {
  args: { inputSize: 'lg', placeholder: 'Large input' },
}

export const ErrorState: Story = {
  args: { error: true, placeholder: 'Invalid input' },
}

export const Disabled: Story = {
  args: { disabled: true, placeholder: 'Disabled input' },
}

export const AllSizes: Story = {
  render: () => (
    <div className="max-w-md space-y-3">
      <Input inputSize="sm" placeholder="Small" />
      <Input inputSize="md" placeholder="Medium" />
      <Input inputSize="lg" placeholder="Large" />
    </div>
  ),
}
