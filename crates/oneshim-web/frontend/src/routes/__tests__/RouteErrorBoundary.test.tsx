import { act, fireEvent, screen } from '@testing-library/react'
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
import { _resetRecoverySignalsForTest, notifyRouteRecovery } from '../recoverySignals'

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
    _resetRecoverySignalsForTest()
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

  it('resets via notifyRouteRecovery for the matching route', () => {
    const recoveryRoute = `/test-recovery-${Date.now()}-${Math.random()}`
    // Module-level state controls thrower behavior. After error caught,
    // we flip the flag, then notify the registry for this route.
    const state = { shouldThrow: true }
    const ControlledThrower = () => {
      if (state.shouldThrow) throw new Error('controlled crash')
      return <div>Recovered</div>
    }

    renderBoundary(<ControlledThrower />, recoveryRoute)
    // Fallback should be visible after the throw
    expect(screen.getByRole('alert')).toBeInTheDocument()

    // Now stop throwing and notify the registry for THIS route.
    // Wrap in act() so React flushes the resulting setResetKey state update.
    state.shouldThrow = false
    act(() => {
      notifyRouteRecovery(recoveryRoute)
    })

    // Boundary remounts and renders the now-working component
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
    expect(screen.getByText('Recovered')).toBeInTheDocument()
  })

  it('ignores notifyRouteRecovery for a different route', () => {
    const myRoute = `/test-my-route-${Date.now()}`
    const otherRoute = `/test-other-route-${Date.now()}`
    renderBoundary(<Thrower shouldThrow={true} />, myRoute)
    expect(screen.getByRole('alert')).toBeInTheDocument()

    // Notify for a DIFFERENT route — boundary should NOT reset
    act(() => {
      notifyRouteRecovery(otherRoute)
    })

    // Fallback still visible
    expect(screen.getByRole('alert')).toBeInTheDocument()
  })

  it('escalates auto-recovery to critical after 3+ signals within window', () => {
    // The auto-recovery path (via notifyRouteRecovery) should also count
    // towards the escalation threshold. After 3 auto-recoveries on the same
    // crashing component, severity escalates to critical and the local
    // remount is skipped (Rust will trigger full-reload).
    const route = `/test-auto-escalation-${Date.now()}`
    renderBoundary(<Thrower shouldThrow={true} />, route)
    expect(screen.getByRole('alert')).toBeInTheDocument()
    // Initial caught error
    expect(reportToNativeMock).toHaveBeenLastCalledWith(expect.objectContaining({ severity: 'error' }))

    // Trigger 3 auto-recoveries in rapid succession (synchronous within
    // the 60s window). On the 3rd, escalation must fire.
    act(() => {
      notifyRouteRecovery(route)
      notifyRouteRecovery(route)
      notifyRouteRecovery(route)
    })

    // The escalation path reports critical without local remount
    const criticalCall = reportToNativeMock.mock.calls.find(([payload]) => payload?.severity === 'critical')
    expect(criticalCall).toBeDefined()
    expect(criticalCall?.[0]).toMatchObject({ route })
  })

  it('clears resetTracker entry on unmount', () => {
    const route = `/test-cleanup-${Date.now()}`
    const { unmount } = renderBoundary(<Thrower shouldThrow={true} />, route)
    // Trigger one tracked reset to populate the entry
    const buttons = screen.getAllByRole('button')
    fireEvent.click(buttons[0])

    unmount()

    // After unmount, a fresh visit must start with a clean window.
    // Render a new boundary at the same route — the prior count must NOT carry over.
    reportToNativeMock.mockClear()
    renderBoundary(<Thrower shouldThrow={true} />, route)
    // Only one error should have been reported (the new mount), not an escalation
    expect(reportToNativeMock).toHaveBeenCalledTimes(1)
    expect(reportToNativeMock).toHaveBeenLastCalledWith(expect.objectContaining({ severity: 'error' }))
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
