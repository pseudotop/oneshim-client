import { Search } from 'lucide-react'
import { useCallback } from 'react'
import { useTranslation } from 'react-i18next'
import { useLocation } from 'react-router-dom'
import { interaction, layout } from '../../styles/tokens'
import { cn } from '../../utils/cn'
import { MOD_KEY } from '../../utils/platform'

const IS_MAC =
  typeof navigator !== 'undefined' &&
  (/mac/i.test(navigator.platform) || /mac/i.test(navigator.userAgent))

const pageTitleKeys: Record<string, string> = {
  '/': 'nav.dashboard',
  '/timeline': 'nav.timeline',
  '/reports': 'nav.reports',
  '/focus': 'nav.focus',
  '/replay': 'nav.replay',
  '/automation': 'nav.automation',
  '/updates': 'nav.updates',
  '/settings': 'nav.settings',
  '/privacy': 'nav.privacy',
  '/search': 'nav.search',
}

interface TitleBarProps {
  onSearchOpen: () => void
}

export default function TitleBar({ onSearchOpen }: TitleBarProps) {
  const { t } = useTranslation()
  const location = useLocation()

  const titleKey = pageTitleKeys[location.pathname] ?? pageTitleKeys['/']
  const pageTitle = t(titleKey)

  const handleMinimize = useCallback(async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      await getCurrentWindow().minimize()
    } catch {
      /* browser fallback */
    }
  }, [])

  const handleMaximize = useCallback(async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      const win = getCurrentWindow()
      if (await win.isMaximized()) {
        await win.unmaximize()
      } else {
        await win.maximize()
      }
    } catch {
      /* browser fallback */
    }
  }, [])

  const handleClose = useCallback(async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      await getCurrentWindow().hide()
    } catch {
      /* browser fallback */
    }
  }, [])

  return (
    <div
      className={cn(
        'app-shell-titlebar flex select-none items-center',
        layout.titleBar.height,
        layout.titleBar.bg,
        layout.titleBar.border,
      )}
      data-tauri-drag-region
    >
      {/* macOS: padding for native traffic lights (overlay titlebar) */}
      {IS_MAC && <div className="w-[78px] flex-shrink-0" data-tauri-drag-region />}

      {/* Page title — centered */}
      <div className="flex flex-1 items-center justify-center" data-tauri-drag-region>
        <span className={cn(layout.titleBar.brand, 'text-content-secondary text-xs tracking-wider')}>
          {pageTitle}
        </span>
      </div>

      {/* Search trigger */}
      <button
        type="button"
        data-testid="titlebar-search"
        onClick={onSearchOpen}
        aria-label={t('shell.searchShortcut', { key: MOD_KEY, defaultValue: `Search (${MOD_KEY}+K)` })}
        className={cn(
          'flex items-center gap-1.5 rounded px-2 py-1 text-xs',
          'text-content-muted hover:text-content-strong',
          'transition-colors hover:bg-hover/50',
          interaction.focusRing,
          'mr-2',
        )}
      >
        <Search className="h-3.5 w-3.5" aria-hidden="true" />
        <span className="hidden text-[11px] text-content-muted sm:inline">{MOD_KEY}K</span>
      </button>

      {/* Window controls — Windows/Linux only (macOS uses native traffic lights) */}
      {!IS_MAC && (
        <div className="flex h-full items-center">
          <button
            type="button"
            data-testid="titlebar-minimize"
            onClick={handleMinimize}
            className={cn('h-full px-3 text-content-secondary transition-colors hover:bg-hover', interaction.focusRing)}
            aria-label={t('shell.minimize', 'Minimize')}
          >
            <svg width="10" height="1" viewBox="0 0 10 1" aria-hidden="true">
              <rect fill="currentColor" width="10" height="1" />
            </svg>
          </button>
          <button
            type="button"
            data-testid="titlebar-maximize"
            onClick={handleMaximize}
            className={cn('h-full px-3 text-content-secondary transition-colors hover:bg-hover', interaction.focusRing)}
            aria-label={t('shell.maximize', 'Maximize')}
          >
            <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
              <rect fill="none" stroke="currentColor" width="9" height="9" x="0.5" y="0.5" />
            </svg>
          </button>
          <button
            type="button"
            data-testid="titlebar-close"
            onClick={handleClose}
            className={cn(
              'h-full px-3 text-content-secondary transition-colors hover:bg-red-500 hover:text-white',
              interaction.focusRing,
            )}
            aria-label={t('shell.closeToTray', 'Close to tray')}
          >
            <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
              <line stroke="currentColor" strokeWidth="1.2" x1="1" y1="1" x2="9" y2="9" />
              <line stroke="currentColor" strokeWidth="1.2" x1="9" y1="1" x2="1" y2="9" />
            </svg>
          </button>
        </div>
      )}
    </div>
  )
}

TitleBar.displayName = 'TitleBar'
