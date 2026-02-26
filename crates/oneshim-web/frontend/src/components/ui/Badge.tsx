/**
 *
 */
import { cn } from '../../utils/cn'
import { badgeVariants } from '../../styles/variants'
import { radius } from '../../styles/tokens'

export interface BadgeProps extends React.HTMLAttributes<HTMLSpanElement> {
  color?: keyof typeof badgeVariants.color
  size?: keyof typeof badgeVariants.size
}

export function Badge({ className, color = 'default', size = 'md', ...props }: BadgeProps) {
  return (
    <span
      className={cn(
        'inline-flex items-center font-medium',
        radius.full,
        badgeVariants.color[color],
        badgeVariants.size[size],
        className
      )}
      {...props}
    />
  )
}
