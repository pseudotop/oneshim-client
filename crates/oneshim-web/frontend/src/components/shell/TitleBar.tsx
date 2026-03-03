import { useCallback } from 'react'
import { Search } from 'lucide-react'
import { layout } from '../../styles/tokens'
import { cn } from '../../utils/cn'

interface TitleBarProps {
  title?: string
  onSearchOpen: () => void
}

export default function TitleBar({ title = 'ONESHIM', onSearchOpen }: TitleBarProps) {
  const isMac = navigator.platform.toUpperCase().includes('MAC')

  const handleMinimize = useCallback(async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      await getCurrentWindow().minimize()
    } catch { /* not in Tauri */ }
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
    } catch { /* not in Tauri */ }
  }, [])

  const handleClose = useCallback(async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      await getCurrentWindow().hide()
    } catch { /* not in Tauri */ }
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
      {/* macOS: leave space for traffic lights */}
      {isMac && <div className="w-[70px] flex-shrink-0" />}

      {/* Brand / Title — centered */}
      <div className="flex-1 flex items-center justify-center" data-tauri-drag-region>
        <span className={layout.titleBar.brand}>{title}</span>
      </div>

      {/* Search trigger */}
      <button
        onClick={onSearchOpen}
        className={cn(
          'flex items-center gap-1.5 px-2 py-1 rounded text-xs',
          'text-slate-400 dark:text-slate-500 hover:text-slate-600 dark:hover:text-slate-300',
          'hover:bg-slate-200/50 dark:hover:bg-slate-800/50 transition-colors',
          'mr-2',
        )}
        title={`${isMac ? '⌘' : 'Ctrl'}+K`}
      >
        <Search className="w-3.5 h-3.5" />
        <span className="hidden sm:inline text-[11px] text-slate-400 dark:text-slate-600">
          {isMac ? '⌘K' : 'Ctrl+K'}
        </span>
      </button>

      {/* Windows: window controls */}
      {!isMac && (
        <div className="flex items-center h-full">
          <button
            onClick={handleMinimize}
            className="h-full px-3 hover:bg-slate-200 dark:hover:bg-slate-700 transition-colors text-slate-500 dark:text-slate-400"
            aria-label="Minimize"
          >
            <svg width="10" height="1" viewBox="0 0 10 1"><rect fill="currentColor" width="10" height="1" /></svg>
          </button>
          <button
            onClick={handleMaximize}
            className="h-full px-3 hover:bg-slate-200 dark:hover:bg-slate-700 transition-colors text-slate-500 dark:text-slate-400"
            aria-label="Maximize"
          >
            <svg width="10" height="10" viewBox="0 0 10 10"><rect fill="none" stroke="currentColor" width="9" height="9" x="0.5" y="0.5" /></svg>
          </button>
          <button
            onClick={handleClose}
            className="h-full px-3 hover:bg-red-500 hover:text-white transition-colors text-slate-500 dark:text-slate-400"
            aria-label="Close"
          >
            <svg width="10" height="10" viewBox="0 0 10 10"><line stroke="currentColor" strokeWidth="1.2" x1="1" y1="1" x2="9" y2="9" /><line stroke="currentColor" strokeWidth="1.2" x1="9" y1="1" x2="1" y2="9" /></svg>
          </button>
        </div>
      )}
    </div>
  )
}
