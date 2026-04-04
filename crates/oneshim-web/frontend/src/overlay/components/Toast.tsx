import { useCallback, useEffect, useState } from 'react'
import { cn } from '../../utils/cn'
import type { ToastItem } from '../types'

const MAX_TOASTS = 3
const TOAST_DURATION = 4000

const typeStyles: Record<ToastItem['type'], string> = {
  success: 'bg-semantic-success/90 text-white',
  error: 'bg-semantic-error/90 text-white',
  info: 'bg-surface-sunken/95 text-content-primary border border-border-default',
}

let addToastGlobal: ((toast: Omit<ToastItem, 'id'>) => void) | null = null

export function showToast(message: string, type: ToastItem['type'] = 'info') {
  addToastGlobal?.({ message, type })
}

export function ToastContainer() {
  const [toasts, setToasts] = useState<ToastItem[]>([])

  const addToast = useCallback((toast: Omit<ToastItem, 'id'>) => {
    const id = `${Date.now()}-${Math.random().toString(36).slice(2, 6)}`
    setToasts(prev => {
      const next = [...prev, { ...toast, id }]
      return next.slice(-MAX_TOASTS)
    })
  }, [])

  useEffect(() => {
    addToastGlobal = addToast
    return () => { addToastGlobal = null }
  }, [addToast])

  useEffect(() => {
    if (toasts.length === 0) return
    const timer = setTimeout(() => {
      setToasts(prev => prev.slice(1))
    }, TOAST_DURATION)
    return () => clearTimeout(timer)
  }, [toasts])

  return (
    <div className="fixed bottom-4 right-4 z-50 flex flex-col gap-2 pointer-events-none">
      {toasts.map(toast => (
        <div
          key={toast.id}
          className={cn(
            'px-4 py-2 rounded-lg text-xs font-medium shadow-lg backdrop-blur-sm',
            'animate-[slideIn_200ms_ease-out]',
            typeStyles[toast.type],
          )}
        >
          {toast.message}
        </div>
      ))}
    </div>
  )
}
