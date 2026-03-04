import { describe, it, expect, vi } from 'vitest'
import { screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { renderWithProviders } from '../../../__tests__/helpers/render-helpers'
import TitleBar from '../TitleBar'

describe('TitleBar', () => {
  it('has displayName', () => {
    expect(TitleBar.displayName).toBe('TitleBar')
  })

  it('renders default title', () => {
    renderWithProviders(<TitleBar onSearchOpen={vi.fn()} />)
    expect(screen.getByText('ONESHIM')).toBeInTheDocument()
  })

  it('renders custom title', () => {
    renderWithProviders(<TitleBar title="Custom" onSearchOpen={vi.fn()} />)
    expect(screen.getByText('Custom')).toBeInTheDocument()
  })

  it('search button has aria-label', () => {
    renderWithProviders(<TitleBar onSearchOpen={vi.fn()} />)
    const btn = screen.getByRole('button', { name: /search/i })
    expect(btn).toBeInTheDocument()
  })

  it('calls onSearchOpen when search button clicked', async () => {
    const user = userEvent.setup()
    const onSearchOpen = vi.fn()
    renderWithProviders(<TitleBar onSearchOpen={onSearchOpen} />)

    await user.click(screen.getByRole('button', { name: /search/i }))
    expect(onSearchOpen).toHaveBeenCalledOnce()
  })
})
