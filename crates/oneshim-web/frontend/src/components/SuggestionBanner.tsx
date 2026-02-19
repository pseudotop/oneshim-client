import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { X, Focus, Coffee, RotateCcw, Brain, MessageSquare, Play } from 'lucide-react'
import { Button } from './ui/Button'
import { fetchLocalSuggestions, submitSuggestionFeedback, LocalSuggestion } from '../api/client'

/** 제안 타입별 아이콘과 색상 */
const SUGGESTION_ICONS: Record<string, { icon: typeof Focus; color: string; bgColor: string }> = {
  NeedFocusTime: {
    icon: Focus,
    color: 'text-blue-600 dark:text-blue-400',
    bgColor: 'bg-blue-100 dark:bg-blue-900/30',
  },
  TakeBreak: {
    icon: Coffee,
    color: 'text-amber-600 dark:text-amber-400',
    bgColor: 'bg-amber-100 dark:bg-amber-900/30',
  },
  RestoreContext: {
    icon: RotateCcw,
    color: 'text-purple-600 dark:text-purple-400',
    bgColor: 'bg-purple-100 dark:bg-purple-900/30',
  },
  PatternDetected: {
    icon: Brain,
    color: 'text-green-600 dark:text-green-400',
    bgColor: 'bg-green-100 dark:bg-green-900/30',
  },
  ExcessiveCommunication: {
    icon: MessageSquare,
    color: 'text-red-600 dark:text-red-400',
    bgColor: 'bg-red-100 dark:bg-red-900/30',
  },
}

/** 제안 메시지 생성 */
function getSuggestionMessage(suggestion: LocalSuggestion, t: (key: string) => string): string {
  const payload = suggestion.payload as Record<string, unknown>

  switch (suggestion.suggestion_type) {
    case 'NeedFocusTime':
      return t('focus.suggestions.needFocusTime').replace(
        '{minutes}',
        String(payload.suggested_focus_mins || 25)
      )
    case 'TakeBreak':
      return t('focus.suggestions.takeBreak').replace(
        '{minutes}',
        String(payload.continuous_work_mins || 90)
      )
    case 'RestoreContext':
      return t('focus.suggestions.restoreContext').replace(
        '{app}',
        String(payload.interrupted_app || 'app')
      )
    case 'PatternDetected':
      return t('focus.suggestions.patternDetected').replace(
        '{description}',
        String(payload.pattern_description || '')
      )
    case 'ExcessiveCommunication':
      return t('focus.suggestions.excessiveCommunication')
    default:
      return t('focus.suggestions.default')
  }
}

export default function SuggestionBanner() {
  const { t } = useTranslation()
  const [suggestions, setSuggestions] = useState<LocalSuggestion[]>([])
  const [currentIndex, setCurrentIndex] = useState(0)
  const [loading, setLoading] = useState(true)
  const [dismissed, setDismissed] = useState<Set<number>>(new Set())

  useEffect(() => {
    fetchLocalSuggestions()
      .then((data) => {
        // 미확인 제안만 필터링 (shown_at이 없거나, acted_at/dismissed_at이 없는 것)
        const pending = data.filter(
          (s) => !s.acted_at && !s.dismissed_at
        )
        setSuggestions(pending)
      })
      .catch(() => {
        // 에러 시 조용히 무시
      })
      .finally(() => setLoading(false))
  }, [])

  // 미확인 제안 목록
  const pendingSuggestions = suggestions.filter(
    (s) => !dismissed.has(s.id)
  )

  const currentSuggestion = pendingSuggestions[currentIndex]

  // 표시 시 shown 마킹
  useEffect(() => {
    if (currentSuggestion && !currentSuggestion.shown_at) {
      submitSuggestionFeedback(currentSuggestion.id, 'shown').catch(() => {})
    }
  }, [currentSuggestion])

  if (loading || pendingSuggestions.length === 0) {
    return null
  }

  const suggestionConfig = SUGGESTION_ICONS[currentSuggestion.suggestion_type] || SUGGESTION_ICONS.PatternDetected
  const Icon = suggestionConfig.icon

  const handleDismiss = async () => {
    try {
      await submitSuggestionFeedback(currentSuggestion.id, 'dismissed')
    } catch {
      // 에러 시 무시
    }
    setDismissed(new Set(dismissed).add(currentSuggestion.id))
    if (currentIndex >= pendingSuggestions.length - 1) {
      setCurrentIndex(Math.max(0, currentIndex - 1))
    }
  }

  const handleAct = async () => {
    try {
      await submitSuggestionFeedback(currentSuggestion.id, 'acted')
    } catch {
      // 에러 시 무시
    }
    setDismissed(new Set(dismissed).add(currentSuggestion.id))
    if (currentIndex >= pendingSuggestions.length - 1) {
      setCurrentIndex(Math.max(0, currentIndex - 1))
    }
  }

  const handleNext = () => {
    setCurrentIndex((prev) => (prev + 1) % pendingSuggestions.length)
  }

  return (
    <div className={`${suggestionConfig.bgColor} border-l-4 ${suggestionConfig.color.replace('text-', 'border-')} rounded-r-lg px-4 py-3 mb-4 flex items-center gap-4`}>
      {/* 아이콘 */}
      <div className={`flex-shrink-0 ${suggestionConfig.color}`}>
        <Icon className="w-6 h-6" />
      </div>

      {/* 메시지 */}
      <div className="flex-1 min-w-0">
        <p className="text-sm font-medium text-slate-900 dark:text-white truncate">
          {getSuggestionMessage(currentSuggestion, t)}
        </p>
        {pendingSuggestions.length > 1 && (
          <p className="text-xs text-slate-500 dark:text-slate-400 mt-0.5">
            {currentIndex + 1} / {pendingSuggestions.length}
          </p>
        )}
      </div>

      {/* 액션 버튼 */}
      <div className="flex-shrink-0 flex items-center gap-2">
        {/* 실행 버튼 */}
        <Button
          variant="primary"
          size="sm"
          onClick={handleAct}
          className="flex items-center gap-1"
        >
          <Play className="w-3 h-3" />
          {t('focus.suggestions.act')}
        </Button>

        {/* 다음 버튼 (여러 제안이 있을 때만) */}
        {pendingSuggestions.length > 1 && (
          <Button
            variant="ghost"
            size="sm"
            onClick={handleNext}
          >
            {t('common.next')}
          </Button>
        )}

        {/* 닫기 버튼 */}
        <button
          onClick={handleDismiss}
          className="p-1 rounded-md text-slate-500 dark:text-slate-400 hover:bg-slate-200 dark:hover:bg-slate-700 transition-colors"
          title={t('common.close')}
        >
          <X className="w-4 h-4" />
        </button>
      </div>
    </div>
  )
}
