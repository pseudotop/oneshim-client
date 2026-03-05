import { screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { renderWithProviders } from '../../../__tests__/helpers/render-helpers'
import ActivityBar from '../ActivityBar'

const defaultProps = {
  onToggleSidebar: vi.fn(),
  sidebarCollapsed: false,
}

describe('ActivityBar', () => {
  it('has displayName', () => {
    expect(ActivityBar.displayName).toBe('ActivityBar')
  })

  it('renders navigation landmark', () => {
    renderWithProviders(<ActivityBar {...defaultProps} />)
    expect(screen.getByRole('navigation')).toBeInTheDocument()
  })

  it('renders 10 nav buttons', () => {
    renderWithProviders(<ActivityBar {...defaultProps} />)
    const buttons = screen.getAllByRole('button')
    expect(buttons).toHaveLength(10)
  })

  it('active route has aria-current="page"', () => {
    renderWithProviders(<ActivityBar {...defaultProps} />, {
      routerProps: { initialEntries: ['/'] },
    })
    const activeButton = screen.getByRole('button', { current: 'page' })
    expect(activeButton).toBeInTheDocument()
  })

  it('clicking a nav button navigates', async () => {
    const user = userEvent.setup()
    const onToggleSidebar = vi.fn()
    renderWithProviders(<ActivityBar onToggleSidebar={onToggleSidebar} sidebarCollapsed={true} />, {
      routerProps: { initialEntries: ['/'] },
    })

    const buttons = screen.getAllByRole('button')
    // Click the timeline button (2nd nav item)
    await user.click(buttons[1])
    // Should have called onToggleSidebar (because sidebar is collapsed)
    expect(onToggleSidebar).toHaveBeenCalled()
  })

  it('each button has an aria-label', () => {
    renderWithProviders(<ActivityBar {...defaultProps} />)
    const buttons = screen.getAllByRole('button')
    buttons.forEach((btn) => {
      expect(btn).toHaveAttribute('aria-label')
    })
  })
})
