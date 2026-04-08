/**
 * RouteErrorBoundary — Per-route error isolation with native forwarding.
 *
 * Architecture: TWO-COMPONENT design.
 *
 *   RouteErrorBoundary (functional wrapper)
 *     ├─ Hooks: useNavigate, useQueryClient, useState(resetKey), useEffect
 *     ├─ Subscribes to recoverySignals registry for its route
 *     └─ Renders RouteErrorBoundaryInner with key={resetKey}
 *           ↓
 *   RouteErrorBoundaryInner (class component)
 *     ├─ getDerivedStateFromError + componentDidCatch (React error boundary contract)
 *     └─ Renders RouteErrorFallback when hasError
 *
 * Why two components? React error boundaries MUST be class components, but the
 * recovery flow needs hooks (useNavigate, useQueryClient, useEffect). The
 * wrapper gives hooks; the inner gives error catching. Reset is propagated via
 * the `key={resetKey}` prop, which forces React to remount the inner boundary.
 *
 * Recovery flow:
 *   1. Section throws error
 *   2. Inner componentDidCatch → onCatch callback → reportToNative
 *   3. reportToNative → Tauri invoke('report_frontend_error')
 *   4. Rust logs (cooldowned) + maybe notifies + maybe emits 'frontend-recovery'
 *   5. useTauriEventBridge listens → calls notifyRouteRecovery(route)
 *   6. Wrapper's recoverySignals subscriber → trackReset → setResetKey++
 *   7. Inner remounts (new key) → fresh render attempt
 *
 * Escalation: Module-level resetTracker counts both manual retries AND
 * automatic recoveries per route. If a route resets 3+ times within 60s,
 * severity escalates to 'critical' which triggers full-reload via Rust.
 *
 * Cleanup: On unmount, the route's resetTracker entry is cleared so a fresh
 * visit (after navigating away and back) starts with a clean trust window.
 */
import { Component, type ErrorInfo, type ReactNode, useCallback, useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { OutletContextError } from './OutletContextError'
import { RouteErrorFallback } from './RouteErrorFallback'
import { subscribeToRouteRecovery } from './recoverySignals'
import { reportToNative, type Severity } from './reportToNative'

// ── Module-level reset escalation tracking ──
//
// Counts both manual retries (button click) and automatic recoveries (Rust
// `reset-route` signal). Scoped per route. Cleared on boundary unmount.

const resetTracker = new Map<string, { count: number; firstAt: number }>()
const ESCALATION_THRESHOLD = 3
const ESCALATION_WINDOW_MS = 60_000

function trackReset(route: string): 'error' | 'critical' {
  const now = Date.now()
  const entry = resetTracker.get(route)

  if (!entry || now - entry.firstAt > ESCALATION_WINDOW_MS) {
    resetTracker.set(route, { count: 1, firstAt: now })
    return 'error'
  }

  entry.count += 1
  if (entry.count >= ESCALATION_THRESHOLD) {
    resetTracker.delete(route)
    return 'critical'
  }
  return 'error'
}

/** Test-only helper to inspect reset tracker state. */
export function _getResetTrackerSizeForTest(): number {
  return resetTracker.size
}

// ── Local helpers ──

function isNetworkError(error: Error): boolean {
  if (error instanceof TypeError && error.message.toLowerCase().includes('fetch')) return true
  const msg = error.message.toLowerCase()
  return ['failed to fetch', 'offline', 'econnrefused', 'timeout', 'network error'].some((kw) => msg.includes(kw))
}

function classifySeverity(error: Error): Severity {
  if (isNetworkError(error)) return 'warning'
  if (error instanceof OutletContextError) return 'error'
  return 'error'
}

// ── Inner class component (React error boundary requirement) ──

interface InnerProps {
  route: string
  children: ReactNode
  onCatch: (error: Error, info: ErrorInfo) => void
  onRetry: () => void
  onGoHome: () => void
}

interface InnerState {
  hasError: boolean
  error: Error | null
  componentStack: string | undefined
}

class RouteErrorBoundaryInner extends Component<InnerProps, InnerState> {
  constructor(props: InnerProps) {
    super(props)
    this.state = { hasError: false, error: null, componentStack: undefined }
  }

  static getDerivedStateFromError(error: Error): Partial<InnerState> {
    return { hasError: true, error }
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    this.setState({ componentStack: info.componentStack ?? undefined })
    this.props.onCatch(error, info)
  }

  render() {
    if (this.state.hasError && this.state.error) {
      return (
        <RouteErrorFallback
          error={this.state.error}
          route={this.props.route}
          componentStack={this.state.componentStack}
          onRetry={this.props.onRetry}
          onGoHome={this.props.onGoHome}
        />
      )
    }
    return this.props.children
  }
}

// ── Outer functional component (hooks for navigation, query client, events) ──

interface RouteErrorBoundaryProps {
  route: string
  children: ReactNode
}

export function RouteErrorBoundary({ route, children }: RouteErrorBoundaryProps) {
  const [resetKey, setResetKey] = useState(0)
  const navigate = useNavigate()

  // Subscribe to recovery signals for this specific route.
  // The registry replaces the prior `window.addEventListener` to avoid
  // global-event spoofability and to provide a typed pub/sub.
  useEffect(() => {
    const unsubscribe = subscribeToRouteRecovery(route, () => {
      // Auto-recovery path: Rust signaled `reset-route`. We must also count
      // this towards the escalation threshold — if the same crash recurs 3+
      // times in 60s, escalate to critical (full-reload) regardless of who
      // triggered the reset.
      const escalated = trackReset(route)
      if (escalated === 'critical') {
        // Tell Rust to upgrade — Rust will emit `full-reload` recovery on
        // the next allowed cycle (cooldown permitting). Skip the local
        // remount; the full-reload will replace it.
        reportToNative({
          route,
          severity: 'critical',
          message: `Auto-recovery escalation: 3+ crashes in 60s on ${route}`,
        })
        return
      }
      // Within threshold — reset locally. Note: queries are NOT invalidated
      // here because the boundary's children re-mount fresh, and react-query
      // will refetch on remount if needed. Avoiding the global invalidate
      // prevents IA-3's "kicks every cached query for the whole app".
      setResetKey((k) => k + 1)
    })
    return unsubscribe
  }, [route])

  // Cleanup the resetTracker entry on unmount so a fresh visit gets a fresh
  // trust window (IA-1: prevents counter leakage across navigation).
  useEffect(() => {
    return () => {
      resetTracker.delete(route)
    }
  }, [route])

  const handleCatch = useCallback(
    (error: Error, info: ErrorInfo) => {
      const severity = classifySeverity(error)
      reportToNative({
        route,
        severity,
        message: error.message,
        stack: error.stack,
        componentStack: info.componentStack ?? undefined,
      })
    },
    [route],
  )

  const handleRetry = useCallback(() => {
    const escalated = trackReset(route)
    if (escalated === 'critical') {
      reportToNative({
        route,
        severity: 'critical',
        message: `Manual retry escalation: 3+ resets in 60s on ${route}`,
      })
      // Don't remount locally — wait for Rust full-reload signal
      return
    }
    setResetKey((k) => k + 1)
  }, [route])

  const handleGoHome = useCallback(() => {
    navigate('/')
  }, [navigate])

  return (
    <RouteErrorBoundaryInner
      key={resetKey}
      route={route}
      onCatch={handleCatch}
      onRetry={handleRetry}
      onGoHome={handleGoHome}
    >
      {children}
    </RouteErrorBoundaryInner>
  )
}
