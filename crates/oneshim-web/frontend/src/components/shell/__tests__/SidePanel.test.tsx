import { screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { renderWithProviders } from '../../../__tests__/helpers/render-helpers'
import SidePanel from '../SidePanel'

const defaultProps = {
  collapsed: false,
  width: 260,
  onResizeStart: vi.fn(),
  onResizeByKeyboard: vi.fn(),
}

describe('SidePanel', () => {
  it('has displayName', () => {
    expect(SidePanel.displayName).toBe('SidePanel')
  })

  it('returns null when collapsed', () => {
    const { container } = renderWithProviders(<SidePanel {...defaultProps} collapsed={true} />)
    expect(container.innerHTML).toBe('')
  })

  it('renders content when not collapsed', () => {
    renderWithProviders(<SidePanel {...defaultProps} />)
    expect(screen.getByRole('tree')).toBeInTheDocument()
  })

  it('resize handle has role="separator"', () => {
    renderWithProviders(<SidePanel {...defaultProps} />)
    const separator = screen.getByRole('separator')
    expect(separator).toBeInTheDocument()
    expect(separator).toHaveAttribute('aria-orientation', 'vertical')
  })

  it('resize handle reports current width', () => {
    renderWithProviders(<SidePanel {...defaultProps} width={300} />)
    const separator = screen.getByRole('separator')
    expect(separator).toHaveAttribute('aria-valuenow', '300')
  })
})
