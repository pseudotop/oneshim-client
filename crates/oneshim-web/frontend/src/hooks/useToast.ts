/**
 * Global toast notification hook using useSyncExternalStore.
 */
import { useSyncExternalStore } from 'react'

export interface Toast {
  id: string
  type: 'success' | 'error' | 'info' | 'warning'
  message: string
  duration: number
}

// Module-level store
let toasts: Toast[] = []
const listeners = new Set<() => void>()

function getSnapshot(): Toast[] {
  return toasts
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

function emitChange(): void {
  for (const listener of listeners) {
    listener()
  }
}

let nextId = 0

function addToast(type: Toast['type'], message: string, duration = 4000): string {
  const id = `toast-${Date.now()}-${nextId++}`
  const toast: Toast = { id, type, message, duration }
  toasts = [...toasts, toast]
  emitChange()

  if (duration > 0) {
    setTimeout(() => {
      removeToast(id)
    }, duration)
  }

  return id
}

function removeToast(id: string): void {
  toasts = toasts.filter((t) => t.id !== id)
  emitChange()
}

export function useToast() {
  const currentToasts = useSyncExternalStore(subscribe, getSnapshot, getSnapshot)

  return {
    toasts: currentToasts,
    show: addToast,
    remove: removeToast,
  }
}
