/**
 * Select 컴포넌트
 *
 * 드롭다운 선택 UI
 */
import { forwardRef } from 'react'
import { cn } from '../../utils/cn'
import { selectVariants } from '../../styles/variants'

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
          'border rounded-lg text-slate-900 dark:text-white',
          'focus:ring-2 focus:ring-teal-500 focus:border-transparent',
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
