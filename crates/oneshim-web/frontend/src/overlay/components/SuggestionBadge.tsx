import { useTranslation } from 'react-i18next'
import { cn } from '../../utils/cn'

interface SuggestionBadgeProps {
  count: number
  onClick: () => void
}

export function SuggestionBadge({ count, onClick }: SuggestionBadgeProps) {
  const { t } = useTranslation()
  if (count === 0) return null

  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        'fixed top-4 right-4 z-50',
        'flex items-center gap-1.5 px-3 py-1.5 rounded-full',
        'bg-brand text-white text-xs font-medium',
        'shadow-lg cursor-pointer',
        'hover:bg-brand/90 transition-colors',
        'animate-pulse',
      )}
    >
      <span>{count}</span>
      <span>{t('suggestions.badgeNew', 'new')}</span>
    </button>
  )
}
