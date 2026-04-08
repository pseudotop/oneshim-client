import { useQueryClient } from '@tanstack/react-query'
import { Component, type ErrorInfo, type ReactNode, useCallback, useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { OutletContextError } from './OutletContextError'
import { RouteErrorFallback } from './RouteErrorFallback'
import { reportToNative, type Severity } from './reportToNative'

// ── Module-level reset escalation tracking ──

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
  const queryClient = useQueryClient()

  // Listen for programmatic reset events (from Rust recovery signals)
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent).detail as { route?: string } | undefined
      if (detail?.route === route) {
        queryClient.invalidateQueries()
        setResetKey((k) => k + 1)
      }
    }
    window.addEventListener('route-error-reset', handler)
    return () => window.removeEventListener('route-error-reset', handler)
  }, [route, queryClient])

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
        message: `Reset escalation threshold reached for route: ${route}`,
      })
    }
    queryClient.invalidateQueries()
    setResetKey((k) => k + 1)
  }, [route, queryClient])

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
