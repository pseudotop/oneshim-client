import { memo } from 'react'
import { useTranslation } from 'react-i18next'
import { colors, typography } from '../../styles/tokens'
import { cn } from '../../utils/cn'

interface FocusModeIndicatorProps {
  active: boolean
}

export const FocusModeIndicator = memo(function FocusModeIndicator({ active }: FocusModeIndicatorProps) {
  const { t } = useTranslation()
  if (!active) return null

  return (
    <div className="pointer-events-none fixed top-3 left-1/2 z-overlay -translate-x-1/2">
      <div
        className={cn(
          'rounded-full bg-brand/90 px-3 py-1 text-[10px] uppercase tracking-wider shadow-md backdrop-blur-sm',
          typography.weight.semibold,
          colors.text.inverse,
        )}
      >
        {t('overlay.focusMode', 'Focus Mode')}
      </div>
    </div>
  )
})
