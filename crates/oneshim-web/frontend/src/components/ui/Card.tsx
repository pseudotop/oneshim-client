/**
 *
 */
import { forwardRef } from 'react'
import { cn } from '../../utils/cn'
import { cardVariants } from '../../styles/variants'
import { colors, radius, typography } from '../../styles/tokens'

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
          radius.md,
          'transition-colors',
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

export function CardHeader({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn('mb-4', className)} {...props} />
}

export function CardTitle({ className, ...props }: React.HTMLAttributes<HTMLHeadingElement>) {
  return <h2 className={cn(typography.h3, colors.text.primary, className)} {...props} />
}

export function CardContent({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn(className)} {...props} />
}
