import type { Meta, StoryObj } from '@storybook/react'
import { Badge, Card, CardTitle } from '../../components/ui'
import {
  darkThemeGlobals,
  lightThemeGlobals,
  ReviewHeader,
  ReviewNote,
  reviewStoryParameters,
  ShellStoryFrame,
} from '../storybook-helpers'

function DesktopShellTemplate() {
  return (
    <ShellStoryFrame route="/settings?tab=monitoring" contentClassName="min-h-[calc(100vh-3.5rem)]">
      <div className="space-y-6 p-6">
        <ReviewHeader
          eyebrow="Template Review"
          title="Desktop Shell"
          description="Shell chrome, side navigation, and content spacing review artifact for route-level contrast and density checks."
        />

        <ReviewNote>
          Use this template to review shell alignment, page-title contrast, left-rail emphasis, and status-bar
          readability before checking a specific route page.
        </ReviewNote>

        <div className="grid gap-6 xl:grid-cols-[1.7fr_1fr]">
          <Card variant="default" padding="lg">
            <div className="mb-4 flex items-center justify-between">
              <CardTitle>Monitoring readiness</CardTitle>
              <Badge color="warning">Permission follow-up</Badge>
            </div>
            <div className="space-y-3 text-content-secondary text-sm">
              <p>
                Accessibility is granted, screen recording still needs attention, and notifications remain optional.
              </p>
              <p>
                This frame is intentionally content-heavy enough to expose weak heading contrast, muted text problems,
                and overly flat card groupings.
              </p>
            </div>
          </Card>

          <Card variant="elevated" padding="lg">
            <CardTitle className="mb-4">Review checklist</CardTitle>
            <ul className="space-y-2 text-content-secondary text-sm">
              <li>Centered title stays legible in light theme.</li>
              <li>Activity bar active state remains visible without oversaturation.</li>
              <li>Side panel header and tree labels keep contrast on both themes.</li>
              <li>Status bar text remains readable against the brand bar.</li>
            </ul>
          </Card>
        </div>
      </div>
    </ShellStoryFrame>
  )
}

const meta = {
  title: 'Templates/DesktopShell',
  component: DesktopShellTemplate,
  tags: ['autodocs'],
  parameters: reviewStoryParameters,
} satisfies Meta<typeof DesktopShellTemplate>

export default meta
type Story = StoryObj<typeof meta>

export const Default: Story = {}

export const LightReview: Story = {
  globals: lightThemeGlobals,
}

export const DarkReview: Story = {
  globals: darkThemeGlobals,
}
