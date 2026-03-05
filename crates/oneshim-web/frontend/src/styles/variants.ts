/**
 * Component variant tokens — backed by CSS custom properties via tokens.ts.
 */
import { colors } from './tokens'

export const buttonVariants = {
  variant: {
    primary: `${colors.primary.DEFAULT} ${colors.primary.hover} ${colors.text.inverse} font-medium`,
    secondary: 'bg-surface-muted hover:bg-active text-content font-medium',
    ghost: `hover:bg-hover ${colors.text.secondary}`,
    danger: 'bg-red-600 hover:bg-red-700 text-content-inverse font-medium',
    warning: 'bg-orange-600 hover:bg-orange-700 text-content-inverse font-medium',
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
    highlight:
      'bg-gradient-to-r from-teal-100/50 to-blue-100/50 dark:from-teal-900/30 dark:to-blue-900/30 border border-teal-300/50 dark:border-teal-700/50',
    interactive: 'bg-surface-elevated hover:bg-active cursor-pointer transition-colors',
    danger: 'bg-surface-elevated border border-red-500/30',
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
    error: `${colors.surface.muted} border-red-500`,
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
    purple: 'bg-accent-purple/20 text-accent-purple',
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

export const statColorMap = {
  teal: 'bg-accent-teal/10 text-accent-teal',
  slate: 'bg-status-disconnected/10 text-content-secondary',
  blue: 'bg-accent-blue/10 text-accent-blue',
  purple: 'bg-accent-purple/10 text-accent-purple',
  green: 'bg-accent-green/10 text-accent-green',
} as const
