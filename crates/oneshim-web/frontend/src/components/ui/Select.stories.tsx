import type { Meta, StoryObj } from '@storybook/react'
import { Select } from './Select'

const meta = {
  title: 'UI Primitives/Select',
  component: Select,
  argTypes: {
    selectSize: {
      control: 'select',
      options: ['sm', 'md'],
    },
    disabled: { control: 'boolean' },
  },
} satisfies Meta<typeof Select>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {
  render: (args) => (
    <Select {...args}>
      <option value="">Select an option</option>
      <option value="1">Option 1</option>
      <option value="2">Option 2</option>
      <option value="3">Option 3</option>
    </Select>
  ),
  args: { selectSize: 'md' },
}

export const Small: Story = {
  render: (args) => (
    <Select {...args}>
      <option value="">Select...</option>
      <option value="a">Alpha</option>
      <option value="b">Bravo</option>
    </Select>
  ),
  args: { selectSize: 'sm' },
}

export const Disabled: Story = {
  render: (args) => (
    <Select {...args}>
      <option value="">Disabled select</option>
      <option value="1">Option 1</option>
    </Select>
  ),
  args: { disabled: true },
}

export const BothSizes: Story = {
  render: () => (
    <div className="max-w-xs space-y-3">
      <Select selectSize="sm">
        <option>Small select</option>
      </Select>
      <Select selectSize="md">
        <option>Medium select</option>
      </Select>
    </div>
  ),
}
