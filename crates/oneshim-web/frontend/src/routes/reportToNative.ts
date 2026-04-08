import { logFrontend } from '../logging/frontendLogger'
import { IS_TAURI } from '../utils/platform'

export type Severity = 'warning' | 'error' | 'critical'

interface ReportOptions {
  route: string
  severity: Severity
  message: string
  stack?: string
  componentStack?: string
}

/**
 * Fire-and-forget error report to the Rust backend.
 * Step 1: forward to frontendLogger (existing IPC bridge).
 * Step 2: invoke report_frontend_error for recovery signals.
 * Both steps are async/fire-and-forget — never blocks the UI.
 */
export function reportToNative(options: ReportOptions): void {
  const { route, severity, message, stack, componentStack } = options

  // Step 1: log through the existing frontend logger bridge
  const context = [stack, componentStack].filter(Boolean).join('\n---componentStack---\n')
  logFrontend('route-error-boundary', severity === 'warning' ? 'warn' : 'error', message, context || undefined)

  // Step 2: invoke the dedicated error report command for recovery
  if (!IS_TAURI) return

  void (async () => {
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      // Tauri auto-converts camelCase JS → snake_case Rust:
      // errorMessage → error_message (matches Rust command parameter)
      await invoke('report_frontend_error', {
        route,
        severity,
        errorMessage: message,
        stack: stack ?? null,
      })
    } catch {
      // Standalone/dev mode — silently ignore
    }
  })()
}
