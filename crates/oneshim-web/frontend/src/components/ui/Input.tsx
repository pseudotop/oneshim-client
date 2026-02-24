/**
 * 입력 컴포넌트
 *
 * variant와 size props로 스타일 적용, error 상태 지원
 */
import { forwardRef } from 'react'
import { cn } from '../../utils/cn'
import { inputVariants } from '../../styles/variants'
import { colors, interaction, radius } from '../../styles/tokens'

export interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  variant?: keyof typeof inputVariants.variant
  inputSize?: keyof typeof inputVariants.size
  error?: boolean
}

export const Input = forwardRef<HTMLInputElement, InputProps>(
  ({ className, variant, inputSize = 'md', error, ...props }, ref) => {
    return (
      <input
        ref={ref}
        className={cn(
          'w-full border placeholder-slate-500',
          radius.md,
          colors.text.primary,
          interaction.interactive,
          interaction.focusRing,
          inputVariants.size[inputSize],
          error ? inputVariants.variant.error : inputVariants.variant.default,
          className
        )}
        {...props}
      />
    )
  }
)

Input.displayName = 'Input'
