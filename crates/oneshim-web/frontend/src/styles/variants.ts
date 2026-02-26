/**
 *
 */
import { colors } from './tokens'

export const buttonVariants = {
  variant: {
    primary: `${colors.primary.DEFAULT} ${colors.primary.hover} ${colors.text.inverse} font-medium`,
    secondary: 'bg-slate-200 dark:bg-slate-700 hover:bg-slate-300 dark:hover:bg-slate-600 text-slate-900 dark:text-white',
    ghost: `hover:bg-slate-200 dark:hover:bg-slate-700 ${colors.text.secondary}`,
    danger: 'bg-red-600 hover:bg-red-700 text-white font-medium',
    warning: 'bg-orange-600 hover:bg-orange-700 text-white font-medium',
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
    highlight: 'bg-gradient-to-r from-teal-100/50 to-blue-100/50 dark:from-teal-900/30 dark:to-blue-900/30 border border-teal-300/50 dark:border-teal-700/50',
    interactive: 'bg-slate-100 dark:bg-slate-800 hover:bg-slate-200 dark:hover:bg-slate-700 cursor-pointer transition-colors',
    danger: 'bg-slate-100 dark:bg-slate-800 border border-red-500/30',
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
    default: `${colors.surface.muted} ${colors.surface.border}`,
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
    default: 'bg-slate-500/20 text-slate-600 dark:text-slate-400',
    success: colors.semantic.success,
    warning: colors.semantic.warning,
    error: colors.semantic.error,
    info: colors.semantic.info,
    primary: 'bg-teal-500/20 text-teal-600 dark:text-teal-400',
    purple: 'bg-purple-500/20 text-purple-600 dark:text-purple-400',
  },
  size: {
    sm: 'px-1.5 py-0.5 text-xs',
    md: 'px-2 py-1 text-sm',
  },
} as const

export const selectVariants = {
  variant: {
    default: `${colors.surface.base} ${colors.surface.border}`,
  },
  size: {
    sm: 'px-3 py-1.5 text-sm',
    md: 'px-4 py-2 text-sm',
  },
} as const

export const statColorMap = {
  teal: `bg-teal-500/10 ${colors.accent.teal}`,
  slate: 'bg-slate-500/10 text-slate-600 dark:text-slate-400',
  blue: `bg-blue-500/10 ${colors.accent.blue}`,
  purple: `bg-purple-500/10 ${colors.accent.purple}`,
  green: `bg-green-500/10 ${colors.accent.green}`,
} as const
