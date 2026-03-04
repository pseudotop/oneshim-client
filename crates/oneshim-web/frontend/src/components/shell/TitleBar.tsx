import { useCallback } from 'react'
import { useTranslation } from 'react-i18next'
import { Search } from 'lucide-react'
import { layout, interaction } from '../../styles/tokens'
import { cn } from '../../utils/cn'
import { IS_MAC, MOD_KEY } from '../../utils/platform'

interface TitleBarProps {
  title?: string
  onSearchOpen: () => void
}

export default function TitleBar({ title = 'ONESHIM', onSearchOpen }: TitleBarProps) {
  const { t } = useTranslation()
  const handleMinimize = useCallback(async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      await getCurrentWindow().minimize()
    } catch { /* browser fallback — no-op */ }
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
    } catch { /* browser fallback — no-op */ }
  }, [])

  // hide() instead of close() — keeps the app running in the system tray (close-to-tray pattern)
  const handleClose = useCallback(async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      await getCurrentWindow().hide()
    } catch { /* browser fallback — no-op */ }
  }, [])

  return (
    <div
      className={cn(
        'app-shell-titlebar flex items-center select-none',
        layout.titleBar.height,
        layout.titleBar.bg,
        layout.titleBar.border,
      )}
      data-tauri-drag-region
    >
      {/* Brand / Title — centered */}
      <div className="flex-1 flex items-center justify-center" data-tauri-drag-region>
        <span className={layout.titleBar.brand}>{title}</span>
      </div>

      {/* Search trigger */}
      <button
        onClick={onSearchOpen}
        aria-label={`Search (${MOD_KEY}+K)`}
        className={cn(
          'flex items-center gap-1.5 px-2 py-1 rounded text-xs',
          'text-slate-400 dark:text-slate-500 hover:text-slate-600 dark:hover:text-slate-300',
          'hover:bg-slate-200/50 dark:hover:bg-slate-800/50 transition-colors',
          interaction.focusRing,
          'mr-2',
        )}
        title={`${MOD_KEY}+K`}
      >
        <Search className="w-3.5 h-3.5" aria-hidden="true" />
        <span className="hidden sm:inline text-[11px] text-slate-400 dark:text-slate-600">
          {MOD_KEY}K
        </span>
      </button>

      {/* Window controls — only shown on non-macOS (macOS uses decorations:false with no traffic lights) */}
      {!IS_MAC && (
        <div className="flex items-center h-full">
          <button
            onClick={handleMinimize}
            className={cn('h-full px-3 hover:bg-slate-200 dark:hover:bg-slate-700 transition-colors text-slate-500 dark:text-slate-400', interaction.focusRing)}
            aria-label={t('shell.minimize', 'Minimize')}
          >
            <svg width="10" height="1" viewBox="0 0 10 1" aria-hidden="true"><rect fill="currentColor" width="10" height="1" /></svg>
          </button>
          <button
            onClick={handleMaximize}
            className={cn('h-full px-3 hover:bg-slate-200 dark:hover:bg-slate-700 transition-colors text-slate-500 dark:text-slate-400', interaction.focusRing)}
            aria-label={t('shell.maximize', 'Maximize')}
          >
            <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><rect fill="none" stroke="currentColor" width="9" height="9" x="0.5" y="0.5" /></svg>
          </button>
          <button
            onClick={handleClose}
            className={cn('h-full px-3 hover:bg-red-500 hover:text-white transition-colors text-slate-500 dark:text-slate-400', interaction.focusRing)}
            aria-label={t('shell.closeToTray', 'Close to tray')}
          >
            <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><line stroke="currentColor" strokeWidth="1.2" x1="1" y1="1" x2="9" y2="9" /><line stroke="currentColor" strokeWidth="1.2" x1="9" y1="1" x2="1" y2="9" /></svg>
          </button>
        </div>
      )}
    </div>
  )
}

TitleBar.displayName = 'TitleBar'
