import { Cpu, HardDrive, Wifi, WifiOff, Zap, ZapOff } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useSSE } from '../../hooks/useSSE'
import { layout } from '../../styles/tokens'
import { cn } from '../../utils/cn'

declare const __APP_VERSION__: string

/** Query automation status via Tauri IPC; fall back to SSE-derived status in browser mode. */
function useAutomationStatus(connected: boolean) {
  const [status, setStatus] = useState(connected)

  const poll = useCallback(async () => {
    try {
      const { invoke } = await import('@tauri-apps/api/core')
      const result = await invoke<boolean>('get_automation_status')
      setStatus(result)
    } catch {
      // Browser fallback — derive from SSE connection
      setStatus(connected)
    }
  }, [connected])

  useEffect(() => {
    poll()
    const id = setInterval(poll, 5_000)
    return () => clearInterval(id)
  }, [poll])

  return status
}

export default function StatusBar() {
  const { t } = useTranslation()
  const { status, latestMetrics } = useSSE()

  const connected = status === 'connected'
  const automationOn = useAutomationStatus(connected)
  const cpuText = latestMetrics ? `${latestMetrics.cpu_usage.toFixed(1)}%` : '--'
  const ramMb = latestMetrics ? `${Math.round(latestMetrics.memory_used / 1024 / 1024)}MB` : '--'

  return (
    <div
      className={cn(
        'app-shell-statusbar flex select-none items-center justify-between px-2',
        layout.statusBar.height,
        layout.statusBar.bg,
        layout.statusBar.text,
      )}
    >
      <div className="flex items-center">
        <span className="flex items-center gap-1 px-1.5" aria-live="polite" aria-atomic="true">
          {connected ? (
            <>
              <Wifi className="h-3 w-3" aria-hidden="true" />
              <span>{t('shell.connected', 'Connected')}</span>
            </>
          ) : (
            <>
              <WifiOff className="h-3 w-3 opacity-60" aria-hidden="true" />
              <span>{t('shell.offline', 'Offline')}</span>
            </>
          )}
        </span>

        <div className={layout.statusBar.separator} />

        <output className="flex items-center gap-1 px-1.5" aria-live="polite" aria-atomic="true">
          {automationOn ? (
            <>
              <Zap className="h-3 w-3" aria-hidden="true" />
              <span>{t('shell.automationOn', 'Auto: ON')}</span>
            </>
          ) : (
            <>
              <ZapOff className="h-3 w-3 opacity-60" aria-hidden="true" />
              <span>{t('shell.automationOff', 'Auto: OFF')}</span>
            </>
          )}
        </output>
      </div>

      <div className="flex items-center">
        <span className="flex items-center gap-1 px-1.5">
          <Cpu className="h-3 w-3" aria-hidden="true" />
          <span>{cpuText}</span>
        </span>

        <div className={layout.statusBar.separator} />

        <span className="flex items-center gap-1 px-1.5">
          <HardDrive className="h-3 w-3" aria-hidden="true" />
          <span>{ramMb}</span>
        </span>

        <div className={layout.statusBar.separator} />

        <span className="px-1.5 opacity-70">{__APP_VERSION__}</span>
      </div>
    </div>
  )
}

StatusBar.displayName = 'StatusBar'
