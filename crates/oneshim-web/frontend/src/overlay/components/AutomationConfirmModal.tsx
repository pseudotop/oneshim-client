import { useCallback, useEffect, useRef, useState } from 'react'
import { cn } from '../../utils/cn'
import { motion, typography } from '../../styles/tokens'
import type { PendingConfirmationDto } from '../types'

const AUTO_DENY_SECS = 30

/** Strip Unicode control characters that could disguise the actual command. */
function sanitizeArg(arg: string): string {
  // C0 controls, DEL, C1 controls, zero-width chars, bidi overrides,
  // line/paragraph separators, BOM
  return arg.replace(/[\u0000-\u001F\u007F-\u009F\u200B-\u200F\u2028-\u202F\uFEFF]/g, '')
}

const auditBadgeColors: Record<string, string> = {
  Critical: 'bg-semantic-error/20 text-semantic-error',
  Elevated: 'bg-semantic-warning/20 text-semantic-warning',
  Basic: 'bg-semantic-info/20 text-semantic-info',
}

interface AutomationConfirmModalProps {
  confirmation: PendingConfirmationDto
  onDismiss: () => void
}

export function AutomationConfirmModal({ confirmation, onDismiss }: AutomationConfirmModalProps) {
  const [remaining, setRemaining] = useState(AUTO_DENY_SECS)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null)

  // Countdown timer
  useEffect(() => {
    setRemaining(AUTO_DENY_SECS)
    intervalRef.current = setInterval(() => {
      setRemaining((prev) => {
        if (prev <= 1) {
          if (intervalRef.current) clearInterval(intervalRef.current)
          return 0
        }
        return prev - 1
      })
    }, 1000)
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current)
    }
  }, [confirmation.command_id])

  // Auto-deny on timeout
  useEffect(() => {
    if (remaining === 0 && !submitting) {
      void handleSubmit(false)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [remaining])

  const handleSubmit = useCallback(
    async (approved: boolean) => {
      if (submitting) return
      setSubmitting(true)
      setError(null)
      try {
        const { invoke } = await import('@tauri-apps/api/core')
        await invoke('confirm_automation_command', {
          commandId: confirmation.command_id,
          nonce: confirmation.nonce,
          approved,
        })
        onDismiss()
      } catch (e) {
        console.warn('confirm_automation_command failed:', e)
        setError('Could not submit confirmation.')
        setSubmitting(false)
      }
    },
    [confirmation.command_id, confirmation.nonce, onDismiss, submitting],
  )

  const progressPct = (remaining / AUTO_DENY_SECS) * 100
  const badgeColor = auditBadgeColors[confirmation.audit_level] ?? 'bg-content-inverse/10 text-content-secondary'

  return (
    <div className="fixed inset-0 z-overlay flex items-center justify-center">
      {/* Backdrop */}
      <div className="absolute inset-0 bg-black/30 backdrop-blur-sm" />

      {/* Modal */}
      <div className="relative w-96 max-w-[calc(100vw-2rem)] rounded-xl border border-content-inverse/10 bg-surface-sunken/95 p-5 shadow-2xl backdrop-blur-md">
        {/* Header */}
        <div className="mb-3 flex items-center justify-between">
          <h3 className={cn(typography.h4, 'text-content')}>Automation Confirmation</h3>
          <span className={cn('rounded-full px-2 py-0.5 text-[10px] font-semibold', badgeColor)}>
            {confirmation.audit_level}
          </span>
        </div>

        {/* Process info */}
        <div className="mb-3 rounded-lg bg-content-inverse/5 p-3">
          <div className="mb-1.5 flex items-center gap-2">
            <span className={cn(typography.caption, 'text-content-tertiary')}>Process</span>
            <span className={cn(typography.label, 'text-content')}>{confirmation.process_name}</span>
          </div>
          {confirmation.args.length > 0 && (
            <div className="flex items-start gap-2">
              <span className={cn(typography.caption, 'text-content-tertiary shrink-0 pt-0.5')}>Args</span>
              <code className="text-[11px] text-content-secondary break-all font-mono">
                {confirmation.args.map(sanitizeArg).join(' ')}
              </code>
            </div>
          )}
        </div>

        {/* Error */}
        {error && <p className="mb-3 text-semantic-error text-xs">{error}</p>}

        {/* Countdown progress */}
        <div className="mb-4 h-1 w-full overflow-hidden rounded-full bg-content-inverse/10">
          <div
            className={cn('h-full rounded-full', motion.all, remaining <= 5 ? 'bg-semantic-error' : 'bg-brand')}
            style={{ width: `${progressPct}%` }}
          />
        </div>

        {/* Actions */}
        <div className="flex items-center justify-between">
          <span className={cn(typography.caption, 'text-content-tertiary')}>
            Auto-deny in {remaining}s
          </span>
          <div className="flex gap-2">
            <button
              type="button"
              disabled={submitting}
              onClick={() => void handleSubmit(false)}
              className={cn(
                'rounded-lg px-4 py-1.5 text-xs font-medium',
                motion.colors,
                'bg-semantic-error/15 text-semantic-error hover:bg-semantic-error/25',
                'disabled:opacity-50 disabled:cursor-not-allowed',
              )}
            >
              Deny
            </button>
            <button
              type="button"
              disabled={submitting}
              onClick={() => void handleSubmit(true)}
              className={cn(
                'rounded-lg px-4 py-1.5 text-xs font-medium',
                motion.colors,
                'bg-semantic-success/15 text-semantic-success hover:bg-semantic-success/25',
                'disabled:opacity-50 disabled:cursor-not-allowed',
              )}
            >
              Approve
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
