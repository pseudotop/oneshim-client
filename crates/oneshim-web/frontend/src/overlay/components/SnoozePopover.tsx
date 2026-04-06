import { useTranslation } from 'react-i18next'
import { cn } from '../../utils/cn'

interface SnoozeOption {
  i18nKey: string
  fallback: string
  minutes: number
}

const options: SnoozeOption[] = [
  { i18nKey: 'suggestions.snooze30min', fallback: '30 minutes', minutes: 30 },
  { i18nKey: 'suggestions.snooze1hr', fallback: '1 hour', minutes: 60 },
  { i18nKey: 'suggestions.snooze2hr', fallback: '2 hours', minutes: 120 },
  { i18nKey: 'suggestions.snooze4hr', fallback: '4 hours', minutes: 240 },
  { i18nKey: 'suggestions.snoozeTomorrow', fallback: 'Tomorrow 9 AM', minutes: 0 }, // calculated at runtime
]

interface SnoozePopoverProps {
  onSelect: (minutes: number) => void
  onCancel: () => void
}

function minutesToTomorrow9AM(): number {
  const now = new Date()
  const tomorrow = new Date(now)
  tomorrow.setDate(tomorrow.getDate() + 1)
  tomorrow.setHours(9, 0, 0, 0)
  return Math.ceil((tomorrow.getTime() - now.getTime()) / 60000)
}

export function SnoozePopover({ onSelect, onCancel }: SnoozePopoverProps) {
  const { t } = useTranslation()

  return (
    <div
      className={cn(
        'absolute bottom-full right-0 mb-1 z-10',
        'bg-surface-sunken/95 backdrop-blur-md rounded-lg shadow-lg',
        'border border-border-default p-1 min-w-[140px]',
      )}
    >
      {options.map((opt) => (
        <button
          key={opt.i18nKey}
          type="button"
          className="w-full text-left px-3 py-1.5 text-xs text-content-primary rounded hover:bg-content-inverse/10 transition-colors"
          onClick={() => onSelect(opt.minutes === 0 ? minutesToTomorrow9AM() : opt.minutes)}
        >
          {t(opt.i18nKey, opt.fallback)}
        </button>
      ))}
      <button
        type="button"
        className="w-full text-left px-3 py-1.5 text-xs text-content-secondary rounded hover:bg-content-inverse/10 transition-colors mt-0.5 border-t border-border-default"
        onClick={onCancel}
      >
        {t('suggestions.snoozeCancel', 'Cancel')}
      </button>
    </div>
  )
}
