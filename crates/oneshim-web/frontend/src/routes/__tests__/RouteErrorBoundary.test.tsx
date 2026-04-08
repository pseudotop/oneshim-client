import { fireEvent, screen } from '@testing-library/react'
import type { ReactNode } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

// Hoisted mock — vi.mock is hoisted to the top of the file by Vitest, so any
// variable referenced in the factory must be hoisted as well via vi.hoisted.
const { reportToNativeMock } = vi.hoisted(() => ({
  reportToNativeMock: vi.fn(),
}))
vi.mock('../reportToNative', () => ({
  reportToNative: reportToNativeMock,
}))

import { renderWithProviders } from '../../__tests__/helpers/render-helpers'
import { OutletContextError } from '../OutletContextError'
import { RouteErrorBoundary } from '../RouteErrorBoundary'

interface ThrowerProps {
  shouldThrow: boolean
  error?: Error
}

function Thrower({ shouldThrow, error }: ThrowerProps) {
  if (shouldThrow) {
    throw error ?? new Error('Test crash')
  }
  return <div>Working</div>
}

function renderBoundary(children: ReactNode, route = '/test') {
  return renderWithProviders(<RouteErrorBoundary route={route}>{children}</RouteErrorBoundary>)
}

describe('RouteErrorBoundary', () => {
  beforeEach(() => {
    reportToNativeMock.mockReset()
    // React error boundaries log the caught error to console.error — silence
    // it so the test output stays clean.
    vi.spyOn(console, 'error').mockImplementation(() => {})
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('renders children when no error is thrown', () => {
    renderBoundary(<Thrower shouldThrow={false} />)
    expect(screen.getByText('Working')).toBeInTheDocument()
    expect(reportToNativeMock).not.toHaveBeenCalled()
  })

  it('renders fallback UI when a child throws', () => {
    renderBoundary(<Thrower shouldThrow={true} />, '/focus')

    // Working content is gone
    expect(screen.queryByText('Working')).not.toBeInTheDocument()
    // Fallback alert is mounted
    expect(screen.getByRole('alert')).toBeInTheDocument()
  })

  it('reports caught errors to the native bridge with route metadata', () => {
    renderBoundary(<Thrower shouldThrow={true} error={new Error('boom')} />, '/automation')

    expect(reportToNativeMock).toHaveBeenCalledTimes(1)
    expect(reportToNativeMock).toHaveBeenCalledWith(
      expect.objectContaining({
        route: '/automation',
        message: 'boom',
        severity: 'error',
      }),
    )
  })

  it('classifies network errors as warning severity', () => {
    const networkError = new TypeError('Failed to fetch')
    renderBoundary(<Thrower shouldThrow={true} error={networkError} />, '/dashboard')

    expect(reportToNativeMock).toHaveBeenCalledTimes(1)
    expect(reportToNativeMock).toHaveBeenCalledWith(
      expect.objectContaining({
        severity: 'warning',
      }),
    )
  })

  it('classifies OutletContextError as error severity', () => {
    const ctxError = new OutletContextError('FocusRoute')
    renderBoundary(<Thrower shouldThrow={true} error={ctxError} />, '/focus')

    expect(reportToNativeMock).toHaveBeenCalledTimes(1)
    expect(reportToNativeMock).toHaveBeenCalledWith(
      expect.objectContaining({
        severity: 'error',
      }),
    )
  })

  it('renders the retry button when an error is caught', () => {
    renderBoundary(<Thrower shouldThrow={true} />, '/test')

    const buttons = screen.getAllByRole('button')
    // Fallback shows two buttons: retry + go home
    expect(buttons.length).toBeGreaterThanOrEqual(2)
  })

  it('escalates to critical severity after repeated retries within the window', () => {
    // Use a unique route per test to avoid leaking module-level reset state
    // between cases. Three retries within 60s on the same route → critical.
    const escalationRoute = `/test-escalation-${Date.now()}-${Math.random()}`
    renderBoundary(<Thrower shouldThrow={true} />, escalationRoute)

    // Initial caught error → severity "error"
    expect(reportToNativeMock).toHaveBeenLastCalledWith(expect.objectContaining({ severity: 'error' }))

    // Each click triggers handleRetry → trackReset → eventually critical.
    // After each click the fallback re-renders, so we re-query the button.
    for (let i = 0; i < 3; i++) {
      const buttons = screen.getAllByRole('button')
      // Retry is the primary button rendered first in RouteErrorFallback.
      fireEvent.click(buttons[0])
    }

    // The third click hits the escalation threshold and produces a
    // critical-severity report.
    const criticalCall = reportToNativeMock.mock.calls.find(([payload]) => payload?.severity === 'critical')
    expect(criticalCall).toBeDefined()
    expect(criticalCall?.[0]).toMatchObject({ route: escalationRoute })
  })
})
