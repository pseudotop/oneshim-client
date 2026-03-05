import { Brain, Coffee, Focus, MessageSquare, Play, RotateCcw, X } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { fetchLocalSuggestions, type LocalSuggestion, submitSuggestionFeedback } from '../api/client'
import { iconSize } from '../styles/tokens'
import { Button } from './ui/Button'

const SUGGESTION_ICONS: Record<string, { icon: typeof Focus; color: string; bgColor: string; borderColor: string }> = {
  NeedFocusTime: {
    icon: Focus,
    color: 'text-accent-blue',
    bgColor: 'bg-blue-100 dark:bg-blue-900/30',
    borderColor: 'border-blue-600 dark:border-blue-400',
  },
  TakeBreak: {
    icon: Coffee,
    color: 'text-accent-amber',
    bgColor: 'bg-amber-100 dark:bg-amber-900/30',
    borderColor: 'border-amber-600 dark:border-amber-400',
  },
  RestoreContext: {
    icon: RotateCcw,
    color: 'text-accent-purple',
    bgColor: 'bg-purple-100 dark:bg-purple-900/30',
    borderColor: 'border-purple-600 dark:border-purple-400',
  },
  PatternDetected: {
    icon: Brain,
    color: 'text-accent-green',
    bgColor: 'bg-green-100 dark:bg-green-900/30',
    borderColor: 'border-green-600 dark:border-green-400',
  },
  ExcessiveCommunication: {
    icon: MessageSquare,
    color: 'text-accent-red',
    bgColor: 'bg-red-100 dark:bg-red-900/30',
    borderColor: 'border-red-600 dark:border-red-400',
  },
}

function getSuggestionMessage(suggestion: LocalSuggestion, t: (key: string) => string): string {
  const payload = suggestion.payload as Record<string, unknown>

  switch (suggestion.suggestion_type) {
    case 'NeedFocusTime':
      return t('focus.suggestions.needFocusTime').replace('{minutes}', String(payload.suggested_focus_mins || 25))
    case 'TakeBreak':
      return t('focus.suggestions.takeBreak').replace('{minutes}', String(payload.continuous_work_mins || 90))
    case 'RestoreContext':
      return t('focus.suggestions.restoreContext').replace('{app}', String(payload.interrupted_app || 'app'))
    case 'PatternDetected':
      return t('focus.suggestions.patternDetected').replace('{description}', String(payload.pattern_description || ''))
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
        const pending = data.filter((s) => !s.acted_at && !s.dismissed_at)
        setSuggestions(pending)
      })
      .catch(() => {})
      .finally(() => setLoading(false))
  }, [])

  const pendingSuggestions = suggestions.filter((s) => !dismissed.has(s.id))

  const currentSuggestion = pendingSuggestions[currentIndex]

  // biome-ignore lint/correctness/useExhaustiveDependencies: intentionally keyed on id and shown_at only — full object would cause unnecessary re-fires
  useEffect(() => {
    if (currentSuggestion && !currentSuggestion.shown_at) {
      submitSuggestionFeedback(currentSuggestion.id, 'shown').catch(() => {})
    }
  }, [currentSuggestion?.id, currentSuggestion?.shown_at])

  if (loading || pendingSuggestions.length === 0) {
    return null
  }

  const suggestionConfig = SUGGESTION_ICONS[currentSuggestion.suggestion_type] || SUGGESTION_ICONS.PatternDetected
  const Icon = suggestionConfig.icon

  const handleDismiss = async () => {
    try {
      await submitSuggestionFeedback(currentSuggestion.id, 'dismissed')
    } catch {}
    setDismissed(new Set(dismissed).add(currentSuggestion.id))
    if (currentIndex >= pendingSuggestions.length - 1) {
      setCurrentIndex(Math.max(0, currentIndex - 1))
    }
  }

  const handleAct = async () => {
    try {
      await submitSuggestionFeedback(currentSuggestion.id, 'acted')
    } catch {}
    setDismissed(new Set(dismissed).add(currentSuggestion.id))
    if (currentIndex >= pendingSuggestions.length - 1) {
      setCurrentIndex(Math.max(0, currentIndex - 1))
    }
  }

  const handleNext = () => {
    setCurrentIndex((prev) => (prev + 1) % pendingSuggestions.length)
  }

  return (
    <div
      className={`${suggestionConfig.bgColor} border-l-4 ${suggestionConfig.borderColor} mb-4 flex items-center gap-4 rounded-r-lg px-4 py-3`}
    >
      {/* UI note */}
      <div className={`flex-shrink-0 ${suggestionConfig.color}`}>
        <Icon className={iconSize.lg} />
      </div>

      {/* UI note */}
      <div className="min-w-0 flex-1">
        <p className="truncate font-medium text-content text-sm">{getSuggestionMessage(currentSuggestion, t)}</p>
        {pendingSuggestions.length > 1 && (
          <p className="mt-0.5 text-content-secondary text-xs">
            {currentIndex + 1} / {pendingSuggestions.length}
          </p>
        )}
      </div>

      {/* UI note */}
      <div className="flex flex-shrink-0 items-center gap-2">
        {/* UI note */}
        <Button variant="primary" size="sm" onClick={handleAct} className="flex items-center gap-1">
          <Play className="h-3 w-3" />
          {t('focus.suggestions.act')}
        </Button>

        {/* UI note */}
        {pendingSuggestions.length > 1 && (
          <Button variant="ghost" size="sm" onClick={handleNext}>
            {t('common.next')}
          </Button>
        )}

        {/* UI note */}
        <button
          type="button"
          onClick={handleDismiss}
          className="rounded-md p-1 text-content-secondary transition-colors hover:bg-hover"
          title={t('common.close')}
        >
          <X className="h-4 w-4" />
        </button>
      </div>
    </div>
  )
}
