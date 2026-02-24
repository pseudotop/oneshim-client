/**
 * Select 컴포넌트
 *
 * 드롭다운 선택 UI
 */
import { forwardRef } from 'react'
import { cn } from '../../utils/cn'
import { selectVariants } from '../../styles/variants'
import { colors, interaction, radius } from '../../styles/tokens'

export interface SelectProps extends React.SelectHTMLAttributes<HTMLSelectElement> {
  variant?: keyof typeof selectVariants.variant
  selectSize?: keyof typeof selectVariants.size
}

export const Select = forwardRef<HTMLSelectElement, SelectProps>(
  ({ className, variant = 'default', selectSize = 'md', children, ...props }, ref) => {
    return (
      <select
        ref={ref}
        className={cn(
          'w-full border',
          radius.md,
          colors.text.primary,
          interaction.focusRing,
          selectVariants.variant[variant],
          selectVariants.size[selectSize],
          className
        )}
        {...props}
      >
        {children}
      </select>
    )
  }
)

Select.displayName = 'Select'
