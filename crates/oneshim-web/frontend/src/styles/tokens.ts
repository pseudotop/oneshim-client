/**
 * 디자인 토큰 정의
 *
 * 컴포넌트와 페이지에서 동일한 토큰을 재사용한다.
 */

// 색상 토큰 - 브랜드, 표면, 텍스트, 의미론적 색상
export const colors = {
  // 브랜드 색상
  primary: {
    DEFAULT: 'bg-teal-600 dark:bg-teal-500',
    hover: 'hover:bg-teal-700 dark:hover:bg-teal-400',
    text: 'text-teal-600 dark:text-teal-400',
    signal: 'bg-teal-500 dark:bg-teal-400',
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
  // 강조 텍스트 색상
  accent: {
    teal: 'text-teal-600 dark:text-teal-400',
    blue: 'text-blue-600 dark:text-blue-400',
    purple: 'text-purple-600 dark:text-purple-400',
    green: 'text-green-600 dark:text-green-400',
    amber: 'text-amber-600 dark:text-amber-400',
    red: 'text-red-600 dark:text-red-400',
    slate: 'text-slate-700 dark:text-slate-300',
  },
} as const

// 간격 토큰
export const spacing = {
  none: '',
  xs: 'p-2',
  sm: 'p-3',
  md: 'p-4',
  lg: 'p-6',
  xl: 'p-8',
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

// 상호작용 토큰
export const interaction = {
  interactive: 'transition-colors',
  focusRing: 'focus:outline-none focus:ring-2 focus:ring-teal-500 focus:border-transparent',
  disabled: 'disabled:opacity-50 disabled:cursor-not-allowed',
} as const

// 폼 토큰
export const form = {
  label: 'block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2',
  labelSm: 'block text-xs text-slate-600 dark:text-slate-400 mb-1',
  helper: 'mt-1 text-xs text-slate-600 dark:text-slate-500',
  checkbox: 'w-5 h-5 rounded bg-slate-900 border-slate-700 text-teal-500 focus:ring-teal-500',
  checkboxInline: 'w-5 h-5 rounded bg-slate-900 border-slate-700 text-teal-500 focus:ring-teal-500 mr-3',
  radio: 'w-4 h-4 bg-slate-900 border-slate-700 text-teal-500 focus:ring-teal-500',
  sectionDivider: 'border-slate-300 dark:border-slate-700',
} as const

// 데이터 시각화 토큰
export const dataViz = {
  stroke: {
    good: '#10b981',
    warning: '#f59e0b',
    critical: '#ef4444',
  },
} as const
