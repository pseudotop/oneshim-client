import { useEffect, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { getShortcutsList } from '../hooks/useKeyboardShortcuts'

interface ShortcutsHelpProps {
  onClose: () => void
}

export default function ShortcutsHelp({ onClose }: ShortcutsHelpProps) {
  const { t } = useTranslation()
  const shortcuts = getShortcutsList()
  const dialogRef = useRef<HTMLDivElement>(null)
  const previousFocusRef = useRef<Element | null>(null)

  // Save previous focus + auto-focus dialog on open
  useEffect(() => {
    previousFocusRef.current = document.activeElement
    // Focus the close button inside the dialog
    const timer = setTimeout(() => {
      const firstFocusable = dialogRef.current?.querySelector<HTMLElement>('button')
      firstFocusable?.focus()
    }, 50)
    return () => {
      clearTimeout(timer)
      // Return focus on unmount
      if (previousFocusRef.current instanceof HTMLElement) {
        previousFocusRef.current.focus()
      }
    }
  }, [])

  // Escape to close
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose()
      }
    }

    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [onClose])

  // Focus trap
  useEffect(() => {
    const handleFocusTrap = (e: KeyboardEvent) => {
      if (e.key !== 'Tab' || !dialogRef.current) return

      const focusable = dialogRef.current.querySelectorAll<HTMLElement>(
        'button, [tabindex]:not([tabindex="-1"])'
      )
      if (focusable.length === 0) return

      const first = focusable[0]
      const last = focusable[focusable.length - 1]

      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault()
        last.focus()
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault()
        first.focus()
      }
    }

    document.addEventListener('keydown', handleFocusTrap)
    return () => document.removeEventListener('keydown', handleFocusTrap)
  }, [])

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={onClose}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label={t('shortcuts.title')}
        className="bg-white dark:bg-slate-800 rounded-lg shadow-xl max-w-md w-full mx-4"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between p-4 border-b border-slate-200 dark:border-slate-700">
          <h2 className="text-lg font-semibold text-slate-900 dark:text-white">
            {t('shortcuts.title')}
          </h2>
          <button
            onClick={onClose}
            className="p-1 text-slate-500 hover:text-slate-700 dark:text-slate-400 dark:hover:text-slate-200"
            aria-label={t('common.close', 'Close')}
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div className="p-4 space-y-2">
          {shortcuts.map(({ key, descriptionKey }) => (
            <div key={key} className="flex items-center justify-between py-1">
              <span className="text-slate-600 dark:text-slate-300">{t(descriptionKey)}</span>
              <kbd className="px-2 py-1 bg-slate-100 dark:bg-slate-700 text-slate-800 dark:text-slate-200 text-sm font-mono rounded border border-slate-300 dark:border-slate-600">
                {key}
              </kbd>
            </div>
          ))}
        </div>

        <div className="px-4 py-3 bg-slate-50 dark:bg-slate-700/50 rounded-b-lg text-center">
          <span className="text-sm text-slate-500 dark:text-slate-400">
            {t('shortcuts.closeHint')}
          </span>
        </div>
      </div>
    </div>
  )
}

ShortcutsHelp.displayName = 'ShortcutsHelp'
