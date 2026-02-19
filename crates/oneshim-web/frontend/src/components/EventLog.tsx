// 이벤트 로그 컴포넌트 - 타임라인 이벤트 목록 + 현재 위치 하이라이트

import { useRef, useEffect, useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { AppWindow, Monitor, Moon, Camera, ArrowRightLeft } from 'lucide-react'
import type { TimelineItem } from '../api/client'
import { formatTime } from '../utils/formatters'

interface EventLogProps {
  /** 타임라인 아이템 목록 */
  items: TimelineItem[]
  /** 현재 재생 시각 */
  currentTime: Date
  /** 이벤트 클릭 시 해당 시간으로 이동 */
  onItemClick: (time: Date) => void
}

// 이벤트 타입별 아이콘 및 색상
function getEventIcon(item: TimelineItem) {
  if (item.type === 'Frame') {
    return <Camera className="w-4 h-4 text-teal-500" />
  }
  if (item.type === 'IdlePeriod') {
    return <Moon className="w-4 h-4 text-slate-400" />
  }
  // Event 타입
  const eventType = item.event_type.toLowerCase()
  if (eventType.includes('appswitch') || eventType.includes('context')) {
    return <ArrowRightLeft className="w-4 h-4 text-blue-500" />
  }
  if (eventType.includes('window')) {
    return <AppWindow className="w-4 h-4 text-purple-500" />
  }
  return <Monitor className="w-4 h-4 text-amber-500" />
}

// 이벤트 타입 라벨
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

// 아이템 타임스탬프 추출
function getItemTime(item: TimelineItem): Date {
  if (item.type === 'IdlePeriod') {
    return new Date(item.start)
  }
  return new Date(item.timestamp)
}

export default function EventLog({ items, currentTime, onItemClick }: EventLogProps) {
  const { t } = useTranslation()
  const listRef = useRef<HTMLDivElement>(null)
  const activeItemRef = useRef<HTMLDivElement>(null)

  // 번역된 라벨
  const captureLabel = t('replay.capture', '캡처')
  const idleLabel = t('replay.idle', '유휴')
  const minLabel = t('dashboard.minutes', '분')

  // 현재 시간에 가장 가까운 아이템 인덱스
  const activeIndex = useMemo(() => {
    if (items.length === 0) return -1
    const currentMs = currentTime.getTime()

    let closestIndex = 0
    let closestDiff = Infinity

    for (let i = 0; i < items.length; i++) {
      const itemTime = getItemTime(items[i]).getTime()
      const diff = Math.abs(itemTime - currentMs)

      // 현재 시간 이전이면서 가장 가까운 아이템
      if (itemTime <= currentMs && diff < closestDiff) {
        closestDiff = diff
        closestIndex = i
      }
    }

    return closestIndex
  }, [items, currentTime])

  // 활성 아이템으로 스크롤
  useEffect(() => {
    if (activeItemRef.current && listRef.current) {
      const container = listRef.current
      const item = activeItemRef.current
      const containerRect = container.getBoundingClientRect()
      const itemRect = item.getBoundingClientRect()

      // 아이템이 보이는 영역 밖에 있으면 스크롤
      if (itemRect.top < containerRect.top || itemRect.bottom > containerRect.bottom) {
        item.scrollIntoView({ behavior: 'smooth', block: 'center' })
      }
    }
  }, [activeIndex])

  return (
    <div className="bg-white dark:bg-slate-800 rounded-lg shadow border border-slate-200 dark:border-slate-700 h-full flex flex-col">
      {/* 헤더 */}
      <div className="px-4 py-3 border-b border-slate-200 dark:border-slate-700">
        <h3 className="text-sm font-semibold text-slate-900 dark:text-white">
          {t('replay.eventLog', '이벤트 로그')}
        </h3>
        <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
          {items.length} {t('replay.items', '개 항목')}
        </p>
      </div>

      {/* 이벤트 목록 */}
      <div
        ref={listRef}
        className="flex-1 overflow-y-auto"
      >
        {items.length === 0 ? (
          <div className="flex items-center justify-center h-32 text-slate-500 dark:text-slate-400 text-sm">
            {t('common.noData', '데이터 없음')}
          </div>
        ) : (
          <div className="divide-y divide-slate-100 dark:divide-slate-700">
            {items.map((item, index) => {
              const isActive = index === activeIndex
              const itemTime = getItemTime(item)
              const timeStr = item.type === 'IdlePeriod'
                ? formatTime(item.start)
                : formatTime(item.timestamp)

              return (
                <div
                  key={`${item.type}-${index}`}
                  ref={isActive ? activeItemRef : undefined}
                  className={`px-4 py-2 cursor-pointer transition-colors ${
                    isActive
                      ? 'bg-teal-50 dark:bg-teal-900/30 border-l-2 border-teal-500'
                      : 'hover:bg-slate-50 dark:hover:bg-slate-700/50 border-l-2 border-transparent'
                  }`}
                  onClick={() => onItemClick(itemTime)}
                >
                  <div className="flex items-start space-x-3">
                    {/* 아이콘 */}
                    <div className="mt-0.5">
                      {getEventIcon(item)}
                    </div>

                    {/* 내용 */}
                    <div className="flex-1 min-w-0">
                      {/* 시간 + 타입 */}
                      <div className="flex items-center justify-between">
                        <span className="text-xs font-mono text-slate-500 dark:text-slate-400">
                          {timeStr}
                        </span>
                        <span className={`text-xs px-1.5 py-0.5 rounded ${
                          item.type === 'Frame'
                            ? 'bg-teal-100 dark:bg-teal-900/50 text-teal-700 dark:text-teal-300'
                            : item.type === 'IdlePeriod'
                            ? 'bg-slate-100 dark:bg-slate-700 text-slate-600 dark:text-slate-400'
                            : 'bg-blue-100 dark:bg-blue-900/50 text-blue-700 dark:text-blue-300'
                        }`}>
                          {getEventLabel(item, captureLabel, idleLabel, minLabel)}
                        </span>
                      </div>

                      {/* 앱/창 정보 */}
                      {item.type !== 'IdlePeriod' && (
                        <div className="mt-1">
                          {item.type === 'Frame' ? (
                            <>
                              <p className="text-sm font-medium text-slate-900 dark:text-white truncate">
                                {item.app_name}
                              </p>
                              <p className="text-xs text-slate-500 dark:text-slate-400 truncate">
                                {item.window_title}
                              </p>
                            </>
                          ) : item.app_name && (
                            <>
                              <p className="text-sm font-medium text-slate-900 dark:text-white truncate">
                                {item.app_name}
                              </p>
                              {item.window_title && (
                                <p className="text-xs text-slate-500 dark:text-slate-400 truncate">
                                  {item.window_title}
                                </p>
                              )}
                            </>
                          )}
                        </div>
                      )}

                      {/* 프레임 중요도 */}
                      {item.type === 'Frame' && (
                        <div className="mt-1 flex items-center space-x-2">
                          <div className="flex-1 h-1.5 bg-slate-200 dark:bg-slate-600 rounded-full overflow-hidden">
                            <div
                              className={`h-full ${
                                item.importance >= 0.7
                                  ? 'bg-green-500'
                                  : item.importance >= 0.4
                                  ? 'bg-amber-500'
                                  : 'bg-slate-400'
                              }`}
                              style={{ width: `${item.importance * 100}%` }}
                            />
                          </div>
                          <span className="text-xs text-slate-500 dark:text-slate-400">
                            {Math.round(item.importance * 100)}%
                          </span>
                        </div>
                      )}
                    </div>
                  </div>
                </div>
              )
            })}
          </div>
        )}
      </div>
    </div>
  )
}
