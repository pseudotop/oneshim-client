/**
 * Toast notification UI components.
 */

import { AlertTriangle, CheckCircle, Info, X, XCircle } from 'lucide-react'
import type { Toast } from '../../hooks/useToast'
import { useToast } from '../../hooks/useToast'
import { cn } from '../../utils/cn'

const iconMap = {
  success: CheckCircle,
  error: XCircle,
  info: Info,
  warning: AlertTriangle,
} as const

const colorMap = {
  success: 'bg-green-600',
  error: 'bg-red-600',
  info: 'bg-blue-600',
  warning: 'bg-amber-500',
} as const

function ToastItem({ toast, onDismiss }: { toast: Toast; onDismiss: (id: string) => void }) {
  const Icon = iconMap[toast.type]

  return (
    <div
      role="alert"
      className={cn(
        'flex items-center gap-2 rounded-lg px-4 py-3 text-sm text-white shadow-lg',
        'animate-in slide-in-from-right',
        colorMap[toast.type],
      )}
    >
      <Icon className="h-4 w-4 shrink-0" />
      <span className="flex-1">{toast.message}</span>
      <button
        type="button"
        onClick={() => onDismiss(toast.id)}
        className="shrink-0 rounded p-0.5 hover:bg-white/20"
        aria-label="Dismiss notification"
      >
        <X className="h-3.5 w-3.5" />
      </button>
    </div>
  )
}

export function ToastContainer() {
  const { toasts, remove } = useToast()

  if (toasts.length === 0) return null

  return (
    <div
      aria-live="polite"
      className="fixed right-4 bottom-16 z-50 flex flex-col gap-2"
    >
      {toasts.map((toast) => (
        <ToastItem key={toast.id} toast={toast} onDismiss={remove} />
      ))}
    </div>
  )
}
