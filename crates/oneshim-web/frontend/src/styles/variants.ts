/**
 * Component variant tokens — backed by CSS custom properties via tokens.ts.
 */
import { colors } from './tokens'

export const buttonVariants = {
  variant: {
    primary: `${colors.primary.DEFAULT} ${colors.primary.hover} ${colors.text.inverse} font-medium`,
    secondary: 'bg-surface-muted hover:bg-active text-content font-medium',
    ghost: `hover:bg-hover ${colors.text.secondary}`,
    danger: 'bg-semantic-error hover:bg-semantic-error-hover text-content-inverse font-medium',
    warning: 'bg-semantic-warning hover:bg-semantic-warning-hover text-content-inverse font-medium',
  },
  size: {
    sm: 'px-3 py-1.5 text-sm',
    md: 'px-4 py-2 text-sm',
    lg: 'px-6 py-3 text-base',
    icon: 'p-2',
  },
} as const

export const cardVariants = {
  variant: {
    default: colors.surface.elevated,
    elevated: colors.surface.muted,
    highlight: 'bg-brand-signal/10 border border-brand-signal/30',
    interactive: 'bg-surface-elevated hover:bg-active cursor-pointer transition-colors',
    danger: 'bg-surface-elevated border border-semantic-error/30',
  },
  padding: {
    none: '',
    sm: 'p-3',
    md: 'p-4',
    lg: 'p-6',
  },
} as const

export const inputVariants = {
  variant: {
    default: `${colors.surface.muted} border-DEFAULT`,
    error: 'bg-surface-muted border-semantic-error',
  },
  size: {
    sm: 'px-3 py-1.5 text-sm',
    md: 'px-4 py-2 text-sm',
    lg: 'px-4 py-3 text-base',
  },
} as const

export const badgeVariants = {
  color: {
    default: 'bg-status-disconnected/20 text-content-secondary',
    success: colors.semantic.success,
    warning: colors.semantic.warning,
    error: colors.semantic.error,
    info: colors.semantic.info,
    primary: 'bg-brand-signal/20 text-brand-text',
    purple: 'bg-brand-signal/20 text-brand-text',
  },
  size: {
    sm: 'px-1.5 py-0.5 text-xs',
    md: 'px-2 py-1 text-sm',
  },
} as const

export const selectVariants = {
  variant: {
    default: `${colors.surface.base} border-DEFAULT`,
  },
  size: {
    sm: 'px-3 py-1.5 text-sm',
    md: 'px-4 py-2 text-sm',
  },
} as const