/**
 *
 */

export const colors = {
  primary: {
    DEFAULT: 'bg-teal-600 dark:bg-teal-500',
    hover: 'hover:bg-teal-700 dark:hover:bg-teal-400',
    text: 'text-teal-600 dark:text-teal-400',
    signal: 'bg-teal-500 dark:bg-teal-400',
    border: 'border-teal-500',
  },
  surface: {
    base: 'bg-white dark:bg-slate-900',
    elevated: 'bg-slate-100 dark:bg-slate-800',
    muted: 'bg-slate-200 dark:bg-slate-900',
    border: 'border-slate-300 dark:border-slate-700',
    borderMuted: 'border-slate-200 dark:border-slate-700',
  },
  text: {
    primary: 'text-slate-900 dark:text-white',
    secondary: 'text-slate-600 dark:text-slate-400',
    tertiary: 'text-slate-500 dark:text-slate-500',
    inverse: 'text-white',
  },
  semantic: {
    success: 'bg-green-500/20 text-green-600 dark:text-green-400',
    warning: 'bg-yellow-500/20 text-yellow-600 dark:text-yellow-400',
    error: 'bg-red-500/20 text-red-600 dark:text-red-400',
    info: 'bg-blue-500/20 text-blue-600 dark:text-blue-400',
  },
  status: {
    connected: 'bg-green-500',
    connecting: 'bg-yellow-500',
    disconnected: 'bg-slate-500',
    error: 'bg-red-500',
  },
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

export const spacing = {
  none: '',
  xs: 'p-2',
  sm: 'p-3',
  md: 'p-4',
  lg: 'p-6',
  xl: 'p-8',
} as const

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

export const radius = {
  none: 'rounded-none',
  sm: 'rounded',
  md: 'rounded-lg',
  lg: 'rounded-xl',
  full: 'rounded-full',
} as const

export const interaction = {
  interactive: 'transition-colors',
  focusRing: 'focus:outline-none focus:ring-2 focus:ring-teal-500 focus:border-transparent',
  disabled: 'disabled:opacity-50 disabled:cursor-not-allowed',
} as const

export const form = {
  label: 'block text-sm font-medium text-slate-700 dark:text-slate-300 mb-2',
  labelSm: 'block text-xs text-slate-600 dark:text-slate-400 mb-1',
  helper: 'mt-1 text-xs text-slate-600 dark:text-slate-500',
  checkbox: 'w-5 h-5 rounded bg-slate-900 border-slate-700 text-teal-500 focus:ring-teal-500',
  checkboxInline: 'w-5 h-5 rounded bg-slate-900 border-slate-700 text-teal-500 focus:ring-teal-500 mr-3',
  radio: 'w-4 h-4 bg-slate-900 border-slate-700 text-teal-500 focus:ring-teal-500',
  sectionDivider: 'border-slate-300 dark:border-slate-700',
} as const

export const dataViz = {
  stroke: {
    good: '#10b981',
    warning: '#f59e0b',
    critical: '#ef4444',
  },
} as const

export const layout = {
  titleBar: {
    height: 'h-8',
    bg: 'bg-slate-100 dark:bg-slate-900',
    border: 'border-b border-slate-200 dark:border-slate-800',
    text: 'text-xs font-medium text-slate-600 dark:text-slate-400',
    brand: 'text-sm font-bold text-teal-600 dark:text-teal-400',
  },
  activityBar: {
    width: 'w-12',
    bg: 'bg-slate-50 dark:bg-slate-950',
    border: 'border-r border-slate-200 dark:border-slate-800',
    iconSize: 'w-5 h-5',
    iconDefault: 'text-slate-400 dark:text-slate-500',
    iconActive: 'text-teal-600 dark:text-teal-400',
    iconHover: 'text-slate-600 dark:text-slate-300',
    indicator: 'bg-teal-500',
    tooltip: 'bg-slate-800 dark:bg-slate-200 text-white dark:text-slate-900 text-xs px-2 py-1 rounded shadow-lg',
  },
  sidePanel: {
    minWidth: 200,
    maxWidth: 400,
    defaultWidth: 260,
    bg: 'bg-white dark:bg-slate-900',
    border: 'border-r border-slate-200 dark:border-slate-800',
    headerBg: 'bg-slate-50 dark:bg-slate-900/50',
    headerText: 'text-[11px] font-semibold uppercase tracking-wider text-slate-500 dark:text-slate-500',
    itemBg: 'hover:bg-slate-100 dark:hover:bg-slate-800',
    itemText: 'text-sm text-slate-700 dark:text-slate-300',
    itemActive: 'bg-slate-100 dark:bg-slate-800 text-slate-900 dark:text-white',
    resizeHandle: 'w-1 cursor-col-resize hover:bg-teal-500 active:bg-teal-500 transition-colors',
  },
  mainContent: {
    bg: 'bg-white dark:bg-slate-950',
    padding: 'p-6',
  },
  statusBar: {
    height: 'h-6',
    bg: 'bg-teal-600 dark:bg-teal-700',
    text: 'text-white text-[11px]',
    itemHover: 'hover:bg-teal-500/50 dark:hover:bg-teal-600/50',
    separator: 'w-px bg-teal-500/50 mx-1 h-3.5',
  },
  commandPalette: {
    overlay: 'bg-black/50',
    bg: 'bg-white dark:bg-slate-800',
    border: 'border border-slate-200 dark:border-slate-700',
    shadow: 'shadow-2xl',
    width: 'w-full max-w-xl',
    input: 'text-base bg-transparent text-slate-900 dark:text-white placeholder-slate-400 dark:placeholder-slate-500',
    itemBg: 'hover:bg-slate-100 dark:hover:bg-slate-700',
    itemActive: 'bg-slate-100 dark:bg-slate-700',
    itemText: 'text-sm text-slate-700 dark:text-slate-300',
    badge: 'text-[10px] px-1.5 py-0.5 rounded bg-slate-200 dark:bg-slate-600 text-slate-500 dark:text-slate-400',
  },
} as const
