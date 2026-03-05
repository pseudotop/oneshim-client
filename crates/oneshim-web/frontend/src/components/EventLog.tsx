import { AppWindow, ArrowRightLeft, Camera, Monitor, Moon } from 'lucide-react'
import { useEffect, useMemo, useRef } from 'react'
import { useTranslation } from 'react-i18next'
import type { TimelineItem } from '../api/client'
import { formatTime } from '../utils/formatters'

interface EventLogProps {
  items: TimelineItem[]
  currentTime: Date
  onItemClick: (time: Date) => void
}

function getEventIcon(item: TimelineItem) {
  if (item.type === 'Frame') {
    return <Camera className="h-4 w-4 text-teal-500" />
  }
  if (item.type === 'IdlePeriod') {
    return <Moon className="h-4 w-4 text-content-muted" />
  }
  const eventType = item.event_type.toLowerCase()
  if (eventType.includes('appswitch') || eventType.includes('context')) {
    return <ArrowRightLeft className="h-4 w-4 text-blue-500" />
  }
  if (eventType.includes('window')) {
    return <AppWindow className="h-4 w-4 text-purple-500" />
  }
  return <Monitor className="h-4 w-4 text-amber-500" />
}

function getEventLabel(item: TimelineItem, captureLabel: string, idleLabel: string, minLabel: string) {
  if (item.type === 'Frame') {
    return captureLabel
  }
  if (item.type === 'IdlePeriod') {
    const mins = Math.round(item.duration_secs / 60)
    return `${idleLabel} (${mins}${minLabel})`
  }
  return item.event_type
}

function getItemTime(item: TimelineItem): Date {
  if (item.type === 'IdlePeriod') {
    return new Date(item.start)
  }
  return new Date(item.timestamp)
}

export default function EventLog({ items, currentTime, onItemClick }: EventLogProps) {
  const { t } = useTranslation()
  const listRef = useRef<HTMLDivElement>(null)
  const activeItemRef = useRef<HTMLButtonElement>(null)

  const captureLabel = t('replay.capture', '캡처')
  const idleLabel = t('replay.idle', 'idle')
  const minLabel = t('dashboard.minutes', '분')

  const activeIndex = useMemo(() => {
    if (items.length === 0) return -1
    const currentMs = currentTime.getTime()

    let closestIndex = 0
    let closestDiff = Infinity

    for (let i = 0; i < items.length; i++) {
      const itemTime = getItemTime(items[i]).getTime()
      const diff = Math.abs(itemTime - currentMs)

      if (itemTime <= currentMs && diff < closestDiff) {
        closestDiff = diff
        closestIndex = i
      }
    }

    return closestIndex
  }, [items, currentTime])

  useEffect(() => {
    if (activeItemRef.current && listRef.current) {
      const container = listRef.current
      const item = activeItemRef.current
      const containerRect = container.getBoundingClientRect()
      const itemRect = item.getBoundingClientRect()

      if (itemRect.top < containerRect.top || itemRect.bottom > containerRect.bottom) {
        item.scrollIntoView({ behavior: 'smooth', block: 'center' })
      }
    }
  }, [])

  return (
    <div className="flex h-full flex-col rounded-lg border border-muted bg-surface-overlay shadow">
      {/* UI note */}
      <div className="border-muted border-b px-4 py-3">
        <h3 className="font-semibold text-content text-sm">{t('replay.eventLog', 'event 로그')}</h3>
        <p className="mt-0.5 text-content-secondary text-xs">
          {items.length} {t('replay.items', '개 항목')}
        </p>
      </div>

      {/* event list */}
      <div ref={listRef} className="flex-1 overflow-y-auto">
        {items.length === 0 ? (
          <div className="flex h-32 items-center justify-center text-content-secondary text-sm">
            {t('common.noData', '데이터 none')}
          </div>
        ) : (
          <div className="divide-y divide-border">
            {items.map((item, index) => {
              const isActive = index === activeIndex
              const itemTime = getItemTime(item)
              const timeStr = item.type === 'IdlePeriod' ? formatTime(item.start) : formatTime(item.timestamp)

              const itemKey =
                item.type === 'IdlePeriod'
                  ? `idle-${item.start}`
                  : item.type === 'Frame'
                    ? `frame-${item.id}`
                    : `event-${item.timestamp}-${item.event_type}`

              return (
                <button
                  type="button"
                  key={itemKey}
                  ref={isActive ? activeItemRef : undefined}
                  className={`w-full cursor-pointer px-4 py-2 text-left transition-colors ${
                    isActive
                      ? 'border-teal-500 border-l-2 bg-teal-50 dark:bg-teal-900/30'
                      : 'border-transparent border-l-2 hover:bg-hover'
                  }`}
                  onClick={() => onItemClick(itemTime)}
                >
                  <div className="flex items-start space-x-3">
                    {/* UI note */}
                    <div className="mt-0.5">{getEventIcon(item)}</div>

                    {/* UI note */}
                    <div className="min-w-0 flex-1">
                      {/* UI note */}
                      <div className="flex items-center justify-between">
                        <span className="font-mono text-content-secondary text-xs">{timeStr}</span>
                        <span
                          className={`rounded px-1.5 py-0.5 text-xs ${
                            item.type === 'Frame'
                              ? 'bg-teal-100 text-teal-700 dark:bg-teal-900/50 dark:text-teal-300'
                              : item.type === 'IdlePeriod'
                                ? 'bg-surface-elevated text-content-secondary'
                                : 'bg-blue-100 text-blue-700 dark:bg-blue-900/50 dark:text-blue-300'
                          }`}
                        >
                          {getEventLabel(item, captureLabel, idleLabel, minLabel)}
                        </span>
                      </div>

                      {/* UI note */}
                      {item.type !== 'IdlePeriod' && (
                        <div className="mt-1">
                          {item.type === 'Frame' ? (
                            <>
                              <p className="truncate font-medium text-content text-sm">{item.app_name}</p>
                              <p className="truncate text-content-secondary text-xs">{item.window_title}</p>
                            </>
                          ) : (
                            item.app_name && (
                              <>
                                <p className="truncate font-medium text-content text-sm">{item.app_name}</p>
                                {item.window_title && (
                                  <p className="truncate text-content-secondary text-xs">{item.window_title}</p>
                                )}
                              </>
                            )
                          )}
                        </div>
                      )}

                      {/* UI note */}
                      {item.type === 'Frame' && (
                        <div className="mt-1 flex items-center space-x-2">
                          <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-surface-muted">
                            <div
                              className={`h-full ${
                                item.importance >= 0.7
                                  ? 'bg-green-500'
                                  : item.importance >= 0.4
                                    ? 'bg-amber-500'
                                    : 'bg-accent-slate'
                              }`}
                              style={{ width: `${item.importance * 100}%` }}
                            />
                          </div>
                          <span className="text-content-secondary text-xs">{Math.round(item.importance * 100)}%</span>
                        </div>
                      )}
                    </div>
                  </div>
                </button>
              )
            })}
          </div>
        )}
      </div>
    </div>
  )
}
