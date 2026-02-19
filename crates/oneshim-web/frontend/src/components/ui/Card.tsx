/**
 * 카드 컴포넌트
 *
 * variant와 padding props로 다양한 스타일 적용
 */
import { forwardRef } from 'react'
import { cn } from '../../utils/cn'
import { cardVariants } from '../../styles/variants'

export interface CardProps extends React.HTMLAttributes<HTMLDivElement> {
  variant?: keyof typeof cardVariants.variant
  padding?: keyof typeof cardVariants.padding
}

export const Card = forwardRef<HTMLDivElement, CardProps>(
  ({ className, variant = 'default', padding = 'md', ...props }, ref) => {
    return (
      <div
        ref={ref}
        className={cn(
          'rounded-lg transition-colors',
          cardVariants.variant[variant],
          cardVariants.padding[padding],
          className
        )}
        {...props}
      />
    )
  }
)

Card.displayName = 'Card'

// CardHeader, CardTitle, CardContent 서브 컴포넌트
export function CardHeader({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn('mb-4', className)} {...props} />
}

export function CardTitle({ className, ...props }: React.HTMLAttributes<HTMLHeadingElement>) {
  return (
    <h2
      className={cn('text-lg font-semibold text-slate-900 dark:text-white', className)}
      {...props}
    />
  )
}

export function CardContent({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn(className)} {...props} />
}
