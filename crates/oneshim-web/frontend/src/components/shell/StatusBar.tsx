import { Wifi, WifiOff, Zap, Cpu, HardDrive } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useSSE } from '../../hooks/useSSE'
import { layout } from '../../styles/tokens'
import { cn } from '../../utils/cn'

declare const __APP_VERSION__: string

export default function StatusBar() {
  const { t } = useTranslation()
  const { status, latestMetrics } = useSSE()

  const connected = status === 'connected'
  const cpuText = latestMetrics ? `${latestMetrics.cpu_usage.toFixed(1)}%` : '--'
  const ramMb = latestMetrics ? `${Math.round(latestMetrics.memory_used / 1024 / 1024)}MB` : '--'

  return (
    <div className={cn(
      'app-shell-statusbar flex items-center justify-between px-2 select-none',
      layout.statusBar.height,
      layout.statusBar.bg,
      layout.statusBar.text,
    )}>
      <div className="flex items-center">
        <span className="flex items-center gap-1 px-1.5">
          {connected
            ? <><Wifi className="w-3 h-3" /><span>{t('shell.connected', 'Connected')}</span></>
            : <><WifiOff className="w-3 h-3 opacity-60" /><span>{t('shell.offline', 'Offline')}</span></>
          }
        </span>

        <div className={layout.statusBar.separator} />

        <span className="flex items-center gap-1 px-1.5">
          <Zap className="w-3 h-3" />
          <span>{t('shell.automationOn', 'Auto: ON')}</span>
        </span>
      </div>

      <div className="flex items-center">
        <span className="flex items-center gap-1 px-1.5" aria-label={`CPU: ${cpuText}`}>
          <Cpu className="w-3 h-3" />
          <span>{cpuText}</span>
        </span>

        <div className={layout.statusBar.separator} />

        <span className="flex items-center gap-1 px-1.5" aria-label={`RAM: ${ramMb}`}>
          <HardDrive className="w-3 h-3" />
          <span>{ramMb}</span>
        </span>

        <div className={layout.statusBar.separator} />

        <span className="px-1.5 opacity-70">{__APP_VERSION__}</span>
      </div>
    </div>
  )
}

StatusBar.displayName = 'StatusBar'
