import { screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { renderWithProviders } from '../../../__tests__/helpers/render-helpers'
import { FieldHint, GuidanceEmptyState, SettingPreview, UnavailableFeatureCallout } from '../Guidance'

describe('Guidance components', () => {
  it('renders an empty state with compact guidance cards and actions', async () => {
    const user = userEvent.setup()
    const onStart = vi.fn()
    const onSecondary = vi.fn()

    renderWithProviders(
      <GuidanceEmptyState
        icon={<span aria-hidden="true">i</span>}
        title="No policies yet"
        description="Start with one trusted command."
        guidance={[
          { title: 'Choose one process', description: 'Use an exact command name.' },
          { title: 'Keep confirmation on', description: 'Review the first execution.' },
        ]}
        primaryAction={{ label: 'Add Policy', onClick: onStart }}
        secondaryAction={{ label: 'Read Guide', onClick: onSecondary }}
      />,
    )

    expect(screen.getByRole('heading', { name: 'No policies yet' })).toBeInTheDocument()
    expect(screen.getByText('Choose one process')).toBeInTheDocument()
    expect(screen.getByText('Keep confirmation on')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Add Policy' }))
    await user.click(screen.getByRole('button', { name: 'Read Guide' }))

    expect(onStart).toHaveBeenCalledTimes(1)
    expect(onSecondary).toHaveBeenCalledTimes(1)
  })

  it('renders field hints that can be linked with aria-describedby', () => {
    renderWithProviders(<FieldHint id="policy-id-hint">Stable internal id.</FieldHint>)

    expect(screen.getByText('Stable internal id.')).toHaveAttribute('id', 'policy-id-hint')
  })

  it('renders setting preview rows as a definition list', () => {
    renderWithProviders(
      <SettingPreview
        title="Policy preview"
        rows={[
          { label: 'Process', value: 'git' },
          { label: 'Confirmation', value: 'Confirmation required', tone: 'warning' },
        ]}
        footer="Default to confirmation until the first run looks correct."
      />,
    )

    expect(screen.getByRole('heading', { name: 'Policy preview' })).toBeInTheDocument()
    expect(screen.getByText('Process')).toBeInTheDocument()
    expect(screen.getByText('git')).toBeInTheDocument()
    expect(screen.getByText('Confirmation required')).toBeInTheDocument()
    expect(screen.getByText('Default to confirmation until the first run looks correct.')).toBeInTheDocument()
  })

  it('renders unavailable feature guidance with a disabled badge and optional action', async () => {
    const user = userEvent.setup()
    const onOpen = vi.fn()

    renderWithProviders(
      <UnavailableFeatureCallout
        title="Nightly unavailable"
        description="Nightly artifacts are not supported in this build."
        reason="Choose Stable or Pre-release for now."
        action={{ label: 'Open Updates', onClick: onOpen }}
      />,
    )

    expect(screen.getByRole('status')).toHaveTextContent('Nightly unavailable')
    expect(screen.getByText('Unavailable')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Open Updates' }))

    expect(onOpen).toHaveBeenCalledTimes(1)
  })
})
