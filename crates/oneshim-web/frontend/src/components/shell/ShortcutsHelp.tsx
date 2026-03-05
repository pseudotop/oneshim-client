import { useEffect, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import { getShortcutsList } from '../../hooks/useKeyboardShortcuts'
import { elevation, interaction, layout } from '../../styles/tokens'
import { cn } from '../../utils/cn'

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
    const timer = setTimeout(() => {
      const firstFocusable = dialogRef.current?.querySelector<HTMLElement>('button')
      firstFocusable?.focus()
    }, 50)
    return () => {
      clearTimeout(timer)
      if (previousFocusRef.current instanceof HTMLElement) {
        previousFocusRef.current.focus()
      }
    }
  }, [])

  // Focus trap
  useEffect(() => {
    const handleFocusTrap = (e: KeyboardEvent) => {
      if (e.key !== 'Tab' || !dialogRef.current) return

      const focusable = dialogRef.current.querySelectorAll<HTMLElement>('button, [tabindex]:not([tabindex="-1"])')
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
    // biome-ignore lint/a11y/noStaticElementInteractions: backdrop overlay — Escape handled via global keyboard shortcut handler
    // biome-ignore lint/a11y/useKeyWithClickEvents: Escape key closes via global keyboard shortcut handler
    <div
      className={cn('fixed inset-0 z-50 flex items-center justify-center', layout.commandPalette.overlay)}
      onClick={onClose}
    >
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: onClick only prevents bubble to backdrop, not interactive */}
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="shortcuts-help-title"
        className={cn(
          layout.commandPalette.bg,
          layout.commandPalette.border,
          elevation.dialog,
          'mx-4 w-full max-w-md rounded-lg',
        )}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between border-muted border-b p-4">
          <h2 id="shortcuts-help-title" className="font-semibold text-content text-lg">
            {t('shortcuts.title')}
          </h2>
          <button
            type="button"
            onClick={onClose}
            className={cn('rounded p-1 text-content-muted hover:text-content', interaction.focusRing)}
            aria-label={t('common.close', 'Close')}
          >
            <svg className="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div className="space-y-2 p-4">
          {shortcuts.map(({ key, descriptionKey }) => (
            <div key={key} className="flex items-center justify-between py-1">
              <span className="text-content-secondary">{t(descriptionKey)}</span>
              <kbd className="rounded border border-DEFAULT bg-surface-elevated px-2 py-1 font-mono text-content text-sm">
                {key}
              </kbd>
            </div>
          ))}
        </div>

        <div className="rounded-b-lg bg-surface-muted px-4 py-3 text-center">
          <span className="text-content-secondary text-sm">{t('shortcuts.closeHint')}</span>
        </div>
      </div>
    </div>
  )
}

ShortcutsHelp.displayName = 'ShortcutsHelp'
