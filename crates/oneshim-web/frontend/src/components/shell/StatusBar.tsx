import { Wifi, WifiOff, Zap, Cpu, HardDrive } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useSSE } from '../../hooks/useSSE'
import { layout } from '../../styles/tokens'
import { cn } from '../../utils/cn'

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
        <button className={cn('flex items-center gap-1 px-1.5 h-full', layout.statusBar.itemHover)}>
          {connected
            ? <><Wifi className="w-3 h-3" /><span>{t('common.connected', 'Connected')}</span></>
            : <><WifiOff className="w-3 h-3 opacity-60" /><span>{t('common.offline', 'Offline')}</span></>
          }
        </button>

        <div className={layout.statusBar.separator} />

        <button className={cn('flex items-center gap-1 px-1.5 h-full', layout.statusBar.itemHover)}>
          <Zap className="w-3 h-3" />
          <span>Auto: ON</span>
        </button>
      </div>

      <div className="flex items-center">
        <button className={cn('flex items-center gap-1 px-1.5 h-full', layout.statusBar.itemHover)}>
          <Cpu className="w-3 h-3" />
          <span>{cpuText}</span>
        </button>

        <div className={layout.statusBar.separator} />

        <button className={cn('flex items-center gap-1 px-1.5 h-full', layout.statusBar.itemHover)}>
          <HardDrive className="w-3 h-3" />
          <span>{ramMb}</span>
        </button>

        <div className={layout.statusBar.separator} />

        <span className="px-1.5 opacity-70">v0.1.5</span>
      </div>
    </div>
  )
}
