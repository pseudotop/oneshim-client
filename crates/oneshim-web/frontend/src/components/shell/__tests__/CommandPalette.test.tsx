import { screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { renderWithProviders } from '../../../__tests__/helpers/render-helpers'
import CommandPalette from '../CommandPalette'

const defaultProps = {
  isOpen: true,
  onClose: vi.fn(),
  onToggleSidebar: vi.fn(),
}

describe('CommandPalette', () => {
  it('has displayName', () => {
    expect(CommandPalette.displayName).toBe('CommandPalette')
  })

  it('returns null when closed', () => {
    const { container } = renderWithProviders(<CommandPalette {...defaultProps} isOpen={false} />)
    expect(container.innerHTML).toBe('')
  })

  it('renders dialog when open', () => {
    renderWithProviders(<CommandPalette {...defaultProps} />)
    expect(screen.getByRole('dialog')).toBeInTheDocument()
    expect(screen.getByRole('dialog')).toHaveAttribute('aria-modal', 'true')
  })

  it('renders combobox input', () => {
    renderWithProviders(<CommandPalette {...defaultProps} />)
    expect(screen.getByRole('combobox')).toBeInTheDocument()
  })

  it('renders listbox with options', () => {
    renderWithProviders(<CommandPalette {...defaultProps} />)
    const options = screen.getAllByRole('option')
    expect(options.length).toBeGreaterThan(0)
  })

  it('filters items by query', async () => {
    const user = userEvent.setup()
    renderWithProviders(<CommandPalette {...defaultProps} />)

    const input = screen.getByRole('combobox')
    await user.type(input, 'Dashboard')

    const options = screen.getAllByRole('option')
    expect(options.length).toBe(1)
    expect(options[0]).toHaveTextContent(/dashboard/i)
  })

  it('shows no results message for non-matching query', async () => {
    const user = userEvent.setup()
    renderWithProviders(<CommandPalette {...defaultProps} />)

    const input = screen.getByRole('combobox')
    await user.type(input, 'xyznonexistent')

    expect(screen.queryAllByRole('option')).toHaveLength(0)
    expect(screen.getByText(/no results/i)).toBeInTheDocument()
  })

  it('calls onClose on Escape', async () => {
    const user = userEvent.setup()
    const onClose = vi.fn()
    renderWithProviders(<CommandPalette {...defaultProps} onClose={onClose} />)

    const input = screen.getByRole('combobox')
    await user.type(input, '{Escape}')
    expect(onClose).toHaveBeenCalled()
  })
})
