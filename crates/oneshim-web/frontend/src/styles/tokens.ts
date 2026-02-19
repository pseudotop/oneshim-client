/**
 * 디자인 토큰 정의
 *
 * Tailwind 클래스로 직접 정의하여 일관성 보장
 */

// 색상 토큰 - 브랜드, 표면, 텍스트, 의미론적 색상
export const colors = {
  // 브랜드 색상
  primary: {
    DEFAULT: 'bg-teal-600 dark:bg-teal-500',
    hover: 'hover:bg-teal-700 dark:hover:bg-teal-400',
    text: 'text-teal-600 dark:text-teal-400',
    border: 'border-teal-500',
  },
  // 표면 색상
  surface: {
    base: 'bg-white dark:bg-slate-900',
    elevated: 'bg-slate-100 dark:bg-slate-800',
    muted: 'bg-slate-200 dark:bg-slate-900',
    border: 'border-slate-300 dark:border-slate-700',
    borderMuted: 'border-slate-200 dark:border-slate-700',
  },
  // 텍스트 색상
  text: {
    primary: 'text-slate-900 dark:text-white',
    secondary: 'text-slate-600 dark:text-slate-400',
    tertiary: 'text-slate-500 dark:text-slate-500',
    inverse: 'text-white',
  },
  // 의미론적 색상
  semantic: {
    success: 'bg-green-500/20 text-green-600 dark:text-green-400',
    warning: 'bg-yellow-500/20 text-yellow-600 dark:text-yellow-400',
    error: 'bg-red-500/20 text-red-600 dark:text-red-400',
    info: 'bg-blue-500/20 text-blue-600 dark:text-blue-400',
  },
  // 상태 인디케이터
  status: {
    connected: 'bg-green-500',
    connecting: 'bg-yellow-500',
    disconnected: 'bg-slate-500',
    error: 'bg-red-500',
  },
} as const

// 간격 토큰
export const spacing = {
  none: '',
  xs: 'p-2',
  sm: 'p-3',
  md: 'p-4',
  lg: 'p-6',
} as const

// 타이포그래피 토큰
export const typography = {
  h1: 'text-2xl font-bold',
  h2: 'text-xl font-semibold',
  h3: 'text-lg font-semibold',
  h4: 'text-base font-medium',
  body: 'text-sm',
  small: 'text-xs',
  stat: {
    hero: 'text-3xl font-bold',
    large: 'text-2xl font-bold',
    normal: 'text-lg font-medium',
  },
} as const

// 반경 토큰
export const radius = {
  none: 'rounded-none',
  sm: 'rounded',
  md: 'rounded-lg',
  lg: 'rounded-xl',
  full: 'rounded-full',
} as const
