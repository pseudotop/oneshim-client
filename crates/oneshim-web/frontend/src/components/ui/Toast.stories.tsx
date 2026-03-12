import type { Meta, StoryObj } from '@storybook/react'
import { addToast, clearToasts } from '../../hooks/useToast'
import { Button } from './Button'
import { ToastContainer } from './Toast'

function ToastDemo() {
  return (
    <div className="min-h-[16rem] bg-surface-base p-6">
      <div className="flex flex-wrap gap-3">
        <Button
          onClick={() => {
            clearToasts()
            addToast('success', 'Settings saved. Some settings require app restart.', 0)
          }}
        >
          Success Toast
        </Button>
        <Button
          variant="secondary"
          onClick={() => {
            clearToasts()
            addToast('info', 'Checking for updates…', 0)
          }}
        >
          Info Toast
        </Button>
        <Button
          variant="ghost"
          onClick={() => {
            clearToasts()
            addToast('warning', 'Model discovery returned no selectable OCR models.', 0)
          }}
        >
          Warning Toast
        </Button>
        <Button
          variant="danger"
          onClick={() => {
            clearToasts()
            addToast('error', 'Failed to save settings: API key is missing.', 0)
          }}
        >
          Error Toast
        </Button>
      </div>

      <ToastContainer />
    </div>
  )
}

const meta = {
  title: 'UI Primitives/Toast',
  component: ToastContainer,
  render: () => <ToastDemo />,
  parameters: {
    layout: 'fullscreen',
  },
} satisfies Meta<typeof ToastContainer>

export default meta
type Story = StoryObj<typeof meta>

export const Playground: Story = {}
