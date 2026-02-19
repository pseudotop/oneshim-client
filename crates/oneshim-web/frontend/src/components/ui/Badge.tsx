/**
 * 배지 컴포넌트
 *
 * color와 size props로 상태/중요도 표시
 */
import { cn } from '../../utils/cn'
import { badgeVariants } from '../../styles/variants'

export interface BadgeProps extends React.HTMLAttributes<HTMLSpanElement> {
  color?: keyof typeof badgeVariants.color
  size?: keyof typeof badgeVariants.size
}

export function Badge({ className, color = 'default', size = 'md', ...props }: BadgeProps) {
  return (
    <span
      className={cn(
        'inline-flex items-center rounded-full font-medium',
        badgeVariants.color[color],
        badgeVariants.size[size],
        className
      )}
      {...props}
    />
  )
}
