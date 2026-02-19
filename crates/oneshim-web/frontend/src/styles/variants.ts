/**
 * 컴포넌트 변형 정의
 *
 * 각 컴포넌트의 variant/size 조합을 정의
 */

// 버튼 변형
export const buttonVariants = {
  variant: {
    primary: 'bg-teal-600 hover:bg-teal-700 text-white font-medium',
    secondary: 'bg-slate-200 dark:bg-slate-700 hover:bg-slate-300 dark:hover:bg-slate-600 text-slate-900 dark:text-white',
    ghost: 'hover:bg-slate-200 dark:hover:bg-slate-700 text-slate-600 dark:text-slate-400',
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

// 카드 변형
export const cardVariants = {
  variant: {
    default: 'bg-slate-100 dark:bg-slate-800',
    elevated: 'bg-slate-200 dark:bg-slate-900',
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

// 입력 변형
export const inputVariants = {
  variant: {
    default: 'bg-slate-200 dark:bg-slate-900 border-slate-300 dark:border-slate-700',
    error: 'bg-slate-200 dark:bg-slate-900 border-red-500',
  },
  size: {
    sm: 'px-3 py-1.5 text-sm',
    md: 'px-4 py-2 text-sm',
    lg: 'px-4 py-3 text-base',
  },
} as const

// 배지 변형
export const badgeVariants = {
  color: {
    default: 'bg-slate-500/20 text-slate-600 dark:text-slate-400',
    success: 'bg-green-500/20 text-green-600 dark:text-green-400',
    warning: 'bg-yellow-500/20 text-yellow-600 dark:text-yellow-400',
    error: 'bg-red-500/20 text-red-600 dark:text-red-400',
    info: 'bg-blue-500/20 text-blue-600 dark:text-blue-400',
    primary: 'bg-teal-500/20 text-teal-600 dark:text-teal-400',
    purple: 'bg-purple-500/20 text-purple-600 dark:text-purple-400',
  },
  size: {
    sm: 'px-1.5 py-0.5 text-xs',
    md: 'px-2 py-1 text-sm',
  },
} as const

// Select 변형
export const selectVariants = {
  variant: {
    default: 'bg-white dark:bg-slate-700 border-slate-300 dark:border-slate-600',
  },
  size: {
    sm: 'px-3 py-1.5 text-sm',
    md: 'px-4 py-2 text-sm',
  },
} as const

// StatCard 색상 맵
export const statColorMap = {
  teal: 'bg-teal-500/10 text-teal-600 dark:text-teal-400',
  slate: 'bg-slate-500/10 text-slate-600 dark:text-slate-400',
  blue: 'bg-blue-500/10 text-blue-600 dark:text-blue-400',
  purple: 'bg-purple-500/10 text-purple-600 dark:text-purple-400',
  green: 'bg-green-500/10 text-green-600 dark:text-green-400',
} as const
