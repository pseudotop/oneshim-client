# Desktop-Style WebView Layout Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Transform the ONESHIM React frontend from a web-style horizontal nav layout to a VS Code/Cursor-style desktop app layout with activity bar, side panel, custom titlebar, status bar, and command palette.

**Architecture:** Replace `App.tsx`'s top navbar + centered content with a CSS Grid shell: custom titlebar (32px) → activity bar (48px) + resizable side panel (260px) + main content → status bar (24px). All 10 page components get minimal wrapper changes (h-full + overflow). Design tokens in `tokens.ts` are extended for the new layout. No new npm dependencies needed (lucide-react, clsx, tailwind-merge already installed).

**Tech Stack:** React 18, TypeScript 5.6, Tailwind CSS 3.4, Vite 5.4, lucide-react, react-router-dom v6, @tauri-apps/api (for window controls)

**Working Directory:** `crates/oneshim-web/frontend/`

---

### Task 1: Add @tauri-apps/api dependency

**Files:**
- Modify: `crates/oneshim-web/frontend/package.json`

**Step 1: Install dependency**

Run:
```bash
cd crates/oneshim-web/frontend && pnpm add @tauri-apps/api@^2
```

**Step 2: Verify installation**

Run: `cd crates/oneshim-web/frontend && pnpm ls @tauri-apps/api`
Expected: Shows `@tauri-apps/api 2.x.x`

**Step 3: Commit**

```bash
git add crates/oneshim-web/frontend/package.json crates/oneshim-web/frontend/pnpm-lock.yaml
git commit -m "feat(frontend): add @tauri-apps/api for desktop window controls"
```

---

### Task 2: Extend design tokens with layout tokens

**Files:**
- Modify: `crates/oneshim-web/frontend/src/styles/tokens.ts`
- Modify: `crates/oneshim-web/frontend/src/index.css`

**Step 1: Add layout tokens to `tokens.ts`**

Append after the existing `dataViz` export at the bottom of the file:

```typescript
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
```

**Step 2: Add CSS custom properties to `index.css`**

Add these custom properties inside the `:root` block in `src/index.css`:

```css
:root {
  /* ... existing vars ... */
  --titlebar-height: 32px;
  --statusbar-height: 24px;
  --activitybar-width: 48px;
  --sidebar-width: 260px;
}
```

Also add at the bottom of the file:

```css
/* Desktop shell layout */
.app-shell {
  display: grid;
  grid-template-rows: var(--titlebar-height) 1fr var(--statusbar-height);
  grid-template-columns: var(--activitybar-width) auto 1fr;
  height: 100vh;
  overflow: hidden;
}

.app-shell-titlebar {
  grid-column: 1 / -1;
}

.app-shell-statusbar {
  grid-column: 1 / -1;
}

/* Tauri drag region */
[data-tauri-drag-region] {
  -webkit-app-region: drag;
}

[data-tauri-drag-region] button,
[data-tauri-drag-region] input,
[data-tauri-drag-region] a {
  -webkit-app-region: no-drag;
}
```

**Step 3: Verify build**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: Build succeeds with no errors

**Step 4: Commit**

```bash
git add crates/oneshim-web/frontend/src/styles/tokens.ts crates/oneshim-web/frontend/src/index.css
git commit -m "feat(frontend): extend design tokens with layout system for desktop shell"
```

---

### Task 3: Create useShellLayout hook

**Files:**
- Create: `crates/oneshim-web/frontend/src/hooks/useShellLayout.ts`

**Step 1: Create the hook**

```typescript
import { useState, useCallback, useEffect, useRef } from 'react'
import { layout } from '../styles/tokens'

interface ShellLayoutState {
  sidebarWidth: number
  sidebarCollapsed: boolean
  activePage: string
}

const STORAGE_KEY_WIDTH = 'oneshim-sidebar-width'
const STORAGE_KEY_COLLAPSED = 'oneshim-sidebar-collapsed'

function loadPersistedState(): Pick<ShellLayoutState, 'sidebarWidth' | 'sidebarCollapsed'> {
  try {
    const width = localStorage.getItem(STORAGE_KEY_WIDTH)
    const collapsed = localStorage.getItem(STORAGE_KEY_COLLAPSED)
    return {
      sidebarWidth: width ? Math.min(Math.max(Number(width), layout.sidePanel.minWidth), layout.sidePanel.maxWidth) : layout.sidePanel.defaultWidth,
      sidebarCollapsed: collapsed === 'true',
    }
  } catch {
    return { sidebarWidth: layout.sidePanel.defaultWidth, sidebarCollapsed: false }
  }
}

export function useShellLayout() {
  const persisted = loadPersistedState()
  const [sidebarWidth, setSidebarWidth] = useState(persisted.sidebarWidth)
  const [sidebarCollapsed, setSidebarCollapsed] = useState(persisted.sidebarCollapsed)
  const [isResizing, setIsResizing] = useState(false)
  const startXRef = useRef(0)
  const startWidthRef = useRef(0)

  // Persist sidebar state
  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY_WIDTH, String(sidebarWidth))
      localStorage.setItem(STORAGE_KEY_COLLAPSED, String(sidebarCollapsed))
    } catch { /* ignore */ }
  }, [sidebarWidth, sidebarCollapsed])

  // Update CSS variable
  useEffect(() => {
    const width = sidebarCollapsed ? 0 : sidebarWidth
    document.documentElement.style.setProperty('--sidebar-width', `${width}px`)
  }, [sidebarWidth, sidebarCollapsed])

  const toggleSidebar = useCallback(() => {
    setSidebarCollapsed(prev => !prev)
  }, [])

  const onResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault()
    setIsResizing(true)
    startXRef.current = e.clientX
    startWidthRef.current = sidebarWidth
  }, [sidebarWidth])

  useEffect(() => {
    if (!isResizing) return

    const onMouseMove = (e: MouseEvent) => {
      const delta = e.clientX - startXRef.current
      const newWidth = Math.min(
        Math.max(startWidthRef.current + delta, layout.sidePanel.minWidth),
        layout.sidePanel.maxWidth
      )
      setSidebarWidth(newWidth)
    }

    const onMouseUp = () => {
      setIsResizing(false)
    }

    document.addEventListener('mousemove', onMouseMove)
    document.addEventListener('mouseup', onMouseUp)
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'

    return () => {
      document.removeEventListener('mousemove', onMouseMove)
      document.removeEventListener('mouseup', onMouseUp)
      document.body.style.cursor = ''
      document.body.style.userSelect = ''
    }
  }, [isResizing])

  return {
    sidebarWidth,
    sidebarCollapsed,
    isResizing,
    toggleSidebar,
    onResizeStart,
  }
}
```

**Step 2: Verify build**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: Build succeeds (unused export warning is OK at this stage)

**Step 3: Commit**

```bash
git add crates/oneshim-web/frontend/src/hooks/useShellLayout.ts
git commit -m "feat(frontend): add useShellLayout hook for sidebar resize and persistence"
```

---

### Task 4: Create TitleBar component

**Files:**
- Create: `crates/oneshim-web/frontend/src/components/shell/TitleBar.tsx`

**Step 1: Create the component**

```tsx
import { useCallback } from 'react'
import { Search } from 'lucide-react'
import { layout } from '../../styles/tokens'
import { cn } from '../../utils/cn'

interface TitleBarProps {
  title?: string
  onSearchOpen: () => void
}

export default function TitleBar({ title = 'ONESHIM', onSearchOpen }: TitleBarProps) {
  const isMac = navigator.platform.toUpperCase().includes('MAC')

  const handleMinimize = useCallback(async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      await getCurrentWindow().minimize()
    } catch { /* not in Tauri */ }
  }, [])

  const handleMaximize = useCallback(async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      const win = getCurrentWindow()
      if (await win.isMaximized()) {
        await win.unmaximize()
      } else {
        await win.maximize()
      }
    } catch { /* not in Tauri */ }
  }, [])

  const handleClose = useCallback(async () => {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      await getCurrentWindow().hide()
    } catch { /* not in Tauri */ }
  }, [])

  return (
    <div
      className={cn(
        'app-shell-titlebar flex items-center select-none',
        layout.titleBar.height,
        layout.titleBar.bg,
        layout.titleBar.border,
      )}
      data-tauri-drag-region
    >
      {/* macOS: leave space for traffic lights */}
      {isMac && <div className="w-[70px] flex-shrink-0" />}

      {/* Brand / Title — centered */}
      <div className="flex-1 flex items-center justify-center" data-tauri-drag-region>
        <span className={layout.titleBar.brand}>{title}</span>
      </div>

      {/* Search trigger */}
      <button
        onClick={onSearchOpen}
        className={cn(
          'flex items-center gap-1.5 px-2 py-1 rounded text-xs',
          'text-slate-400 dark:text-slate-500 hover:text-slate-600 dark:hover:text-slate-300',
          'hover:bg-slate-200/50 dark:hover:bg-slate-800/50 transition-colors',
          'mr-2',
        )}
        title={`${isMac ? '⌘' : 'Ctrl'}+K`}
      >
        <Search className="w-3.5 h-3.5" />
        <span className="hidden sm:inline text-[11px] text-slate-400 dark:text-slate-600">
          {isMac ? '⌘K' : 'Ctrl+K'}
        </span>
      </button>

      {/* Windows: window controls */}
      {!isMac && (
        <div className="flex items-center h-full">
          <button
            onClick={handleMinimize}
            className="h-full px-3 hover:bg-slate-200 dark:hover:bg-slate-700 transition-colors text-slate-500 dark:text-slate-400"
            aria-label="Minimize"
          >
            <svg width="10" height="1" viewBox="0 0 10 1"><rect fill="currentColor" width="10" height="1" /></svg>
          </button>
          <button
            onClick={handleMaximize}
            className="h-full px-3 hover:bg-slate-200 dark:hover:bg-slate-700 transition-colors text-slate-500 dark:text-slate-400"
            aria-label="Maximize"
          >
            <svg width="10" height="10" viewBox="0 0 10 10"><rect fill="none" stroke="currentColor" width="9" height="9" x="0.5" y="0.5" /></svg>
          </button>
          <button
            onClick={handleClose}
            className="h-full px-3 hover:bg-red-500 hover:text-white transition-colors text-slate-500 dark:text-slate-400"
            aria-label="Close"
          >
            <svg width="10" height="10" viewBox="0 0 10 10"><line stroke="currentColor" strokeWidth="1.2" x1="1" y1="1" x2="9" y2="9" /><line stroke="currentColor" strokeWidth="1.2" x1="9" y1="1" x2="1" y2="9" /></svg>
          </button>
        </div>
      )}
    </div>
  )
}
```

**Step 2: Verify build**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: Build succeeds

**Step 3: Commit**

```bash
git add crates/oneshim-web/frontend/src/components/shell/TitleBar.tsx
git commit -m "feat(frontend): add TitleBar component with platform-aware window controls"
```

---

### Task 5: Create ActivityBar component

**Files:**
- Create: `crates/oneshim-web/frontend/src/components/shell/ActivityBar.tsx`

**Step 1: Create the component**

```tsx
import { useState } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import {
  LayoutDashboard, Clock, Zap, Monitor,
  Image, BarChart3, Tag, FileText,
  Settings, Info,
} from 'lucide-react'
import { layout } from '../../styles/tokens'
import { cn } from '../../utils/cn'

interface NavItem {
  id: string
  to: string
  icon: React.ElementType
  label: string
  group: 'monitor' | 'data' | 'manage'
}

const navItems: NavItem[] = [
  { id: 'dashboard', to: '/',          icon: LayoutDashboard, label: 'Dashboard',  group: 'monitor' },
  { id: 'timeline',  to: '/timeline',  icon: Clock,           label: 'Timeline',   group: 'monitor' },
  { id: 'replay',    to: '/replay',    icon: Zap,             label: 'Replay',     group: 'monitor' },
  { id: 'automation',to: '/automation',icon: Monitor,         label: 'Automation', group: 'monitor' },
  { id: 'frames',    to: '/focus',     icon: Image,           label: 'Focus',      group: 'data' },
  { id: 'stats',     to: '/reports',   icon: BarChart3,       label: 'Reports',    group: 'data' },
  { id: 'tags',      to: '/search',    icon: Tag,             label: 'Search',     group: 'data' },
  { id: 'updates',   to: '/updates',   icon: FileText,        label: 'Updates',    group: 'manage' },
]

const bottomItems: NavItem[] = [
  { id: 'settings', to: '/settings', icon: Settings, label: 'Settings', group: 'manage' },
  { id: 'privacy',  to: '/privacy',  icon: Info,     label: 'Privacy',  group: 'manage' },
]

interface ActivityBarProps {
  onToggleSidebar: () => void
  sidebarCollapsed: boolean
}

export default function ActivityBar({ onToggleSidebar, sidebarCollapsed }: ActivityBarProps) {
  const location = useLocation()
  const navigate = useNavigate()
  const [tooltip, setTooltip] = useState<string | null>(null)
  const [tooltipY, setTooltipY] = useState(0)

  const isActive = (to: string) => {
    if (to === '/') return location.pathname === '/'
    return location.pathname.startsWith(to)
  }

  const handleClick = (item: NavItem) => {
    if (isActive(item.to) && !sidebarCollapsed) {
      onToggleSidebar()
    } else {
      navigate(item.to)
      if (sidebarCollapsed) onToggleSidebar()
    }
  }

  const renderItem = (item: NavItem) => {
    const Icon = item.icon
    const active = isActive(item.to)

    return (
      <button
        key={item.id}
        onClick={() => handleClick(item)}
        onMouseEnter={(e) => {
          setTooltip(item.label)
          setTooltipY(e.currentTarget.getBoundingClientRect().top)
        }}
        onMouseLeave={() => setTooltip(null)}
        className={cn(
          'relative w-full flex items-center justify-center h-11 transition-colors',
          active ? layout.activityBar.iconActive : layout.activityBar.iconDefault,
          !active && `hover:${layout.activityBar.iconHover}`,
        )}
        title={item.label}
      >
        {active && (
          <div className={cn('absolute left-0 top-1.5 bottom-1.5 w-0.5 rounded-r', layout.activityBar.indicator)} />
        )}
        <Icon className={layout.activityBar.iconSize} />
      </button>
    )
  }

  const groups = {
    monitor: navItems.filter(i => i.group === 'monitor'),
    data: navItems.filter(i => i.group === 'data'),
    manage: navItems.filter(i => i.group === 'manage'),
  }

  return (
    <div className={cn('flex flex-col items-center py-1', layout.activityBar.bg, layout.activityBar.border, layout.activityBar.width)}>
      {/* Monitor group */}
      {groups.monitor.map(renderItem)}
      <div className="w-6 border-t border-slate-200 dark:border-slate-800 my-1" />

      {/* Data group */}
      {groups.data.map(renderItem)}
      <div className="w-6 border-t border-slate-200 dark:border-slate-800 my-1" />

      {/* Manage group */}
      {groups.manage.map(renderItem)}

      {/* Spacer */}
      <div className="flex-1" />

      {/* Bottom pinned */}
      <div className="w-6 border-t border-slate-200 dark:border-slate-800 my-1" />
      {bottomItems.map(renderItem)}

      {/* Tooltip */}
      {tooltip && (
        <div
          className={cn('fixed z-50 pointer-events-none', layout.activityBar.tooltip)}
          style={{ left: 56, top: tooltipY + 4 }}
        >
          {tooltip}
        </div>
      )}
    </div>
  )
}
```

**Step 2: Verify build**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: Build succeeds

**Step 3: Commit**

```bash
git add crates/oneshim-web/frontend/src/components/shell/ActivityBar.tsx
git commit -m "feat(frontend): add ActivityBar component with icon navigation and groups"
```

---

### Task 6: Create SidePanel and TreeView components

**Files:**
- Create: `crates/oneshim-web/frontend/src/components/shell/TreeView.tsx`
- Create: `crates/oneshim-web/frontend/src/components/shell/SidePanel.tsx`

**Step 1: Create TreeView**

```tsx
import { useState } from 'react'
import { ChevronRight, ChevronDown } from 'lucide-react'
import { layout } from '../../styles/tokens'
import { cn } from '../../utils/cn'

export interface TreeNode {
  id: string
  label: string
  icon?: React.ReactNode
  count?: number
  children?: TreeNode[]
}

interface TreeViewProps {
  nodes: TreeNode[]
  selectedId?: string
  onSelect?: (id: string) => void
  depth?: number
}

export default function TreeView({ nodes, selectedId, onSelect, depth = 0 }: TreeViewProps) {
  const [expanded, setExpanded] = useState<Set<string>>(() => {
    // Expand first level by default
    return new Set(nodes.filter(n => n.children?.length).map(n => n.id))
  })

  const toggleExpand = (id: string) => {
    setExpanded(prev => {
      const next = new Set(prev)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  return (
    <div className="text-sm">
      {nodes.map(node => {
        const hasChildren = node.children && node.children.length > 0
        const isExpanded = expanded.has(node.id)
        const isSelected = selectedId === node.id

        return (
          <div key={node.id}>
            <button
              onClick={() => {
                if (hasChildren) toggleExpand(node.id)
                onSelect?.(node.id)
              }}
              className={cn(
                'w-full flex items-center gap-1.5 py-1 px-2 rounded-sm transition-colors',
                isSelected ? layout.sidePanel.itemActive : layout.sidePanel.itemBg,
                layout.sidePanel.itemText,
              )}
              style={{ paddingLeft: `${depth * 12 + 8}px` }}
            >
              {hasChildren ? (
                isExpanded ? <ChevronDown className="w-3.5 h-3.5 flex-shrink-0 text-slate-400" /> : <ChevronRight className="w-3.5 h-3.5 flex-shrink-0 text-slate-400" />
              ) : (
                <span className="w-3.5 flex-shrink-0" />
              )}
              {node.icon && <span className="flex-shrink-0">{node.icon}</span>}
              <span className="truncate flex-1 text-left">{node.label}</span>
              {node.count !== undefined && (
                <span className="text-[10px] text-slate-400 dark:text-slate-600 tabular-nums">{node.count}</span>
              )}
            </button>
            {hasChildren && isExpanded && (
              <TreeView
                nodes={node.children!}
                selectedId={selectedId}
                onSelect={onSelect}
                depth={depth + 1}
              />
            )}
          </div>
        )
      })}
    </div>
  )
}
```

**Step 2: Create SidePanel**

```tsx
import { useLocation } from 'react-router-dom'
import { layout } from '../../styles/tokens'
import { cn } from '../../utils/cn'
import TreeView, { type TreeNode } from './TreeView'

// Page-specific sidebar content definitions
const pageSidebarConfig: Record<string, { title: string; nodes: TreeNode[] }> = {
  '/': {
    title: 'Dashboard',
    nodes: [
      { id: 'overview', label: 'Overview' },
      { id: 'metrics', label: 'System Metrics' },
      { id: 'processes', label: 'Active Processes' },
      { id: 'focus', label: 'Focus Score' },
      { id: 'heatmap', label: 'Activity Heatmap' },
      { id: 'updates', label: 'Update Status' },
    ],
  },
  '/timeline': {
    title: 'Timeline',
    nodes: [
      { id: 'all', label: 'All Frames' },
      { id: 'filters', label: 'Filters', children: [
        { id: 'by-app', label: 'By Application' },
        { id: 'by-tag', label: 'By Tag' },
        { id: 'by-importance', label: 'By Importance' },
      ]},
    ],
  },
  '/reports': {
    title: 'Reports',
    nodes: [
      { id: 'activity', label: 'Activity Report' },
      { id: 'focus', label: 'Focus Report' },
      { id: 'export', label: 'Export Data' },
    ],
  },
  '/focus': {
    title: 'Focus',
    nodes: [
      { id: 'score', label: 'Current Score' },
      { id: 'trend', label: 'Weekly Trend' },
      { id: 'sessions', label: 'Focus Sessions' },
      { id: 'interruptions', label: 'Interruptions' },
    ],
  },
  '/replay': {
    title: 'Session Replay',
    nodes: [
      { id: 'timeline', label: 'Timeline' },
      { id: 'events', label: 'Event Log' },
    ],
  },
  '/automation': {
    title: 'Automation',
    nodes: [
      { id: 'policies', label: 'Policies' },
      { id: 'commands', label: 'Commands' },
      { id: 'history', label: 'Execution History' },
    ],
  },
  '/updates': {
    title: 'Updates',
    nodes: [
      { id: 'status', label: 'Current Status' },
      { id: 'history', label: 'Update History' },
    ],
  },
  '/settings': {
    title: 'Settings',
    nodes: [
      { id: 'general', label: 'General' },
      { id: 'notification', label: 'Notifications' },
      { id: 'privacy', label: 'Privacy' },
      { id: 'schedule', label: 'Schedule' },
      { id: 'ai', label: 'AI Provider' },
      { id: 'about', label: 'About' },
    ],
  },
  '/privacy': {
    title: 'Privacy',
    nodes: [
      { id: 'data', label: 'Data Controls' },
      { id: 'consent', label: 'Consent' },
      { id: 'export', label: 'Data Export' },
    ],
  },
  '/search': {
    title: 'Search',
    nodes: [
      { id: 'recent', label: 'Recent Searches' },
      { id: 'tags', label: 'Browse Tags' },
    ],
  },
}

interface SidePanelProps {
  collapsed: boolean
  width: number
  onResizeStart: (e: React.MouseEvent) => void
}

export default function SidePanel({ collapsed, width, onResizeStart }: SidePanelProps) {
  const location = useLocation()

  if (collapsed) return null

  // Find matching config (try exact match, then prefix match)
  const path = location.pathname
  const config = pageSidebarConfig[path] ?? Object.entries(pageSidebarConfig).find(
    ([key]) => key !== '/' && path.startsWith(key)
  )?.[1] ?? pageSidebarConfig['/']

  return (
    <div className="relative flex" style={{ width }}>
      <div className={cn('flex-1 flex flex-col overflow-hidden', layout.sidePanel.bg, layout.sidePanel.border)}>
        {/* Header */}
        <div className={cn('px-4 py-2 flex-shrink-0', layout.sidePanel.headerBg)}>
          <span className={layout.sidePanel.headerText}>{config.title}</span>
        </div>

        {/* Tree content */}
        <div className="flex-1 overflow-y-auto px-1 py-1">
          <TreeView nodes={config.nodes} />
        </div>
      </div>

      {/* Resize handle */}
      <div
        className={cn('flex-shrink-0', layout.sidePanel.resizeHandle)}
        onMouseDown={onResizeStart}
      />
    </div>
  )
}
```

**Step 3: Verify build**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: Build succeeds

**Step 4: Commit**

```bash
git add crates/oneshim-web/frontend/src/components/shell/TreeView.tsx crates/oneshim-web/frontend/src/components/shell/SidePanel.tsx
git commit -m "feat(frontend): add SidePanel with TreeView navigation per page"
```

---

### Task 7: Create StatusBar component

**Files:**
- Create: `crates/oneshim-web/frontend/src/components/shell/StatusBar.tsx`

**Step 1: Create the component**

```tsx
import { Wifi, WifiOff, Zap, ZapOff, Cpu, HardDrive } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useSSE } from '../../hooks/useSSE'
import { layout } from '../../styles/tokens'
import { cn } from '../../utils/cn'

export default function StatusBar() {
  const { t } = useTranslation()
  const { status, latestMetrics } = useSSE()

  const connected = status === 'connected'
  const cpuText = latestMetrics ? `${latestMetrics.cpu_usage.toFixed(1)}%` : '--'
  const ramMb = latestMetrics ? `${Math.round(latestMetrics.memory_used / 1024 / 1024)}MB` : '--'

  return (
    <div className={cn(
      'app-shell-statusbar flex items-center justify-between px-2 select-none',
      layout.statusBar.height,
      layout.statusBar.bg,
      layout.statusBar.text,
    )}>
      {/* Left section */}
      <div className="flex items-center">
        {/* Connection status */}
        <button className={cn('flex items-center gap-1 px-1.5 h-full', layout.statusBar.itemHover)}>
          {connected
            ? <><Wifi className="w-3 h-3" /><span>{t('common.connected', 'Connected')}</span></>
            : <><WifiOff className="w-3 h-3 opacity-60" /><span>{t('common.offline', 'Offline')}</span></>
          }
        </button>

        <div className={layout.statusBar.separator} />

        {/* Automation status */}
        <button className={cn('flex items-center gap-1 px-1.5 h-full', layout.statusBar.itemHover)}>
          <Zap className="w-3 h-3" />
          <span>Auto: ON</span>
        </button>
      </div>

      {/* Right section */}
      <div className="flex items-center">
        <button className={cn('flex items-center gap-1 px-1.5 h-full', layout.statusBar.itemHover)}>
          <Cpu className="w-3 h-3" />
          <span>{cpuText}</span>
        </button>

        <div className={layout.statusBar.separator} />

        <button className={cn('flex items-center gap-1 px-1.5 h-full', layout.statusBar.itemHover)}>
          <HardDrive className="w-3 h-3" />
          <span>{ramMb}</span>
        </button>

        <div className={layout.statusBar.separator} />

        <span className="px-1.5 opacity-70">v0.1.5</span>
      </div>
    </div>
  )
}
```

**Step 2: Verify build**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: Build succeeds

**Step 3: Commit**

```bash
git add crates/oneshim-web/frontend/src/components/shell/StatusBar.tsx
git commit -m "feat(frontend): add StatusBar component with connection, metrics, and version"
```

---

### Task 8: Create CommandPalette component

**Files:**
- Create: `crates/oneshim-web/frontend/src/hooks/useCommandPalette.ts`
- Create: `crates/oneshim-web/frontend/src/components/shell/CommandPalette.tsx`

**Step 1: Create the hook**

```typescript
import { useState, useEffect, useCallback } from 'react'

export function useCommandPalette() {
  const [isOpen, setIsOpen] = useState(false)

  const open = useCallback(() => setIsOpen(true), [])
  const close = useCallback(() => setIsOpen(false), [])
  const toggle = useCallback(() => setIsOpen(prev => !prev), [])

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault()
        toggle()
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [toggle])

  return { isOpen, open, close }
}
```

**Step 2: Create the component**

```tsx
import { useState, useEffect, useRef, useMemo } from 'react'
import { useNavigate } from 'react-router-dom'
import { useTranslation } from 'react-i18next'
import {
  LayoutDashboard, Clock, Zap, Monitor,
  Image, BarChart3, Tag, FileText,
  Settings, Info, Moon, Sun, PanelLeft, Search,
} from 'lucide-react'
import { useTheme } from '../../contexts/ThemeContext'
import { layout } from '../../styles/tokens'
import { cn } from '../../utils/cn'

interface PaletteItem {
  id: string
  label: string
  icon: React.ReactNode
  type: 'page' | 'action'
  action: () => void
}

interface CommandPaletteProps {
  isOpen: boolean
  onClose: () => void
  onToggleSidebar: () => void
}

export default function CommandPalette({ isOpen, onClose, onToggleSidebar }: CommandPaletteProps) {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const { theme, toggleTheme } = useTheme()
  const [query, setQuery] = useState('')
  const [selectedIndex, setSelectedIndex] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)

  const items = useMemo<PaletteItem[]>(() => [
    { id: 'dashboard',  label: 'Dashboard',        icon: <LayoutDashboard className="w-4 h-4" />, type: 'page',   action: () => navigate('/') },
    { id: 'timeline',   label: 'Timeline',         icon: <Clock className="w-4 h-4" />,           type: 'page',   action: () => navigate('/timeline') },
    { id: 'reports',    label: 'Reports',           icon: <BarChart3 className="w-4 h-4" />,      type: 'page',   action: () => navigate('/reports') },
    { id: 'focus',      label: 'Focus',             icon: <Image className="w-4 h-4" />,           type: 'page',   action: () => navigate('/focus') },
    { id: 'replay',     label: 'Session Replay',    icon: <Zap className="w-4 h-4" />,             type: 'page',   action: () => navigate('/replay') },
    { id: 'automation', label: 'Automation',        icon: <Monitor className="w-4 h-4" />,         type: 'page',   action: () => navigate('/automation') },
    { id: 'updates',    label: 'Updates',           icon: <FileText className="w-4 h-4" />,        type: 'page',   action: () => navigate('/updates') },
    { id: 'settings',   label: 'Settings',          icon: <Settings className="w-4 h-4" />,        type: 'page',   action: () => navigate('/settings') },
    { id: 'privacy',    label: 'Privacy',           icon: <Info className="w-4 h-4" />,             type: 'page',   action: () => navigate('/privacy') },
    { id: 'search',     label: 'Search',            icon: <Search className="w-4 h-4" />,          type: 'page',   action: () => navigate('/search') },
    { id: 'theme',      label: theme === 'dark' ? 'Switch to Light Mode' : 'Switch to Dark Mode', icon: theme === 'dark' ? <Sun className="w-4 h-4" /> : <Moon className="w-4 h-4" />, type: 'action', action: toggleTheme },
    { id: 'sidebar',    label: 'Toggle Sidebar',    icon: <PanelLeft className="w-4 h-4" />,       type: 'action', action: onToggleSidebar },
  ], [navigate, theme, toggleTheme, onToggleSidebar])

  const filtered = useMemo(() => {
    if (!query) return items
    const q = query.toLowerCase()
    return items.filter(item => item.label.toLowerCase().includes(q))
  }, [items, query])

  // Reset state when opening
  useEffect(() => {
    if (isOpen) {
      setQuery('')
      setSelectedIndex(0)
      setTimeout(() => inputRef.current?.focus(), 50)
    }
  }, [isOpen])

  // Clamp selected index
  useEffect(() => {
    if (selectedIndex >= filtered.length) {
      setSelectedIndex(Math.max(0, filtered.length - 1))
    }
  }, [filtered.length, selectedIndex])

  const executeItem = (item: PaletteItem) => {
    item.action()
    onClose()
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault()
        setSelectedIndex(prev => (prev + 1) % filtered.length)
        break
      case 'ArrowUp':
        e.preventDefault()
        setSelectedIndex(prev => (prev - 1 + filtered.length) % filtered.length)
        break
      case 'Enter':
        e.preventDefault()
        if (filtered[selectedIndex]) executeItem(filtered[selectedIndex])
        break
      case 'Escape':
        e.preventDefault()
        onClose()
        break
    }
  }

  if (!isOpen) return null

  return (
    <div className={cn('fixed inset-0 z-50 flex items-start justify-center pt-[15vh]', layout.commandPalette.overlay)} onClick={onClose}>
      <div
        className={cn(
          layout.commandPalette.width,
          layout.commandPalette.bg,
          layout.commandPalette.border,
          layout.commandPalette.shadow,
          'rounded-lg overflow-hidden',
        )}
        onClick={e => e.stopPropagation()}
      >
        {/* Input */}
        <div className="flex items-center px-4 py-3 border-b border-slate-200 dark:border-slate-700">
          <Search className="w-4 h-4 text-slate-400 mr-3 flex-shrink-0" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={e => { setQuery(e.target.value); setSelectedIndex(0) }}
            onKeyDown={handleKeyDown}
            placeholder={t('commandPalette.placeholder', 'Type a command or search...')}
            className={cn('flex-1 outline-none', layout.commandPalette.input)}
          />
        </div>

        {/* Results */}
        <div className="max-h-80 overflow-y-auto py-1">
          {filtered.length === 0 ? (
            <div className="px-4 py-6 text-center text-sm text-slate-400">
              {t('commandPalette.noResults', 'No results found')}
            </div>
          ) : (
            filtered.map((item, index) => (
              <button
                key={item.id}
                onClick={() => executeItem(item)}
                onMouseEnter={() => setSelectedIndex(index)}
                className={cn(
                  'w-full flex items-center gap-3 px-4 py-2 text-left transition-colors',
                  layout.commandPalette.itemText,
                  index === selectedIndex && layout.commandPalette.itemActive,
                  layout.commandPalette.itemBg,
                )}
              >
                <span className="flex-shrink-0 text-slate-400">{item.icon}</span>
                <span className="flex-1 truncate">{item.label}</span>
                <span className={layout.commandPalette.badge}>{item.type}</span>
              </button>
            ))
          )}
        </div>
      </div>
    </div>
  )
}
```

**Step 3: Verify build**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: Build succeeds

**Step 4: Commit**

```bash
git add crates/oneshim-web/frontend/src/hooks/useCommandPalette.ts crates/oneshim-web/frontend/src/components/shell/CommandPalette.tsx
git commit -m "feat(frontend): add CommandPalette with fuzzy search and keyboard navigation"
```

---

### Task 9: Create shell index file

**Files:**
- Create: `crates/oneshim-web/frontend/src/components/shell/index.ts`

**Step 1: Create barrel export**

```typescript
export { default as TitleBar } from './TitleBar'
export { default as ActivityBar } from './ActivityBar'
export { default as SidePanel } from './SidePanel'
export { default as StatusBar } from './StatusBar'
export { default as CommandPalette } from './CommandPalette'
export { default as TreeView } from './TreeView'
export type { TreeNode } from './TreeView'
```

**Step 2: Verify build**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: Build succeeds

**Step 3: Commit**

```bash
git add crates/oneshim-web/frontend/src/components/shell/index.ts
git commit -m "feat(frontend): add shell components barrel export"
```

---

### Task 10: Refactor App.tsx with desktop shell layout

This is the core task — replacing the web-style layout with the desktop shell.

**Files:**
- Modify: `crates/oneshim-web/frontend/src/App.tsx`

**Step 1: Rewrite App.tsx**

Replace the entire `App.tsx` with:

```tsx
import { lazy, Suspense } from 'react'
import { Routes, Route } from 'react-router-dom'
import { TitleBar, ActivityBar, SidePanel, StatusBar, CommandPalette } from './components/shell'
import { useShellLayout } from './hooks/useShellLayout'
import { useCommandPalette } from './hooks/useCommandPalette'
import { useKeyboardShortcuts } from './hooks/useKeyboardShortcuts'
import { layout } from './styles/tokens'
import ErrorBoundary from './components/ErrorBoundary'
import { Spinner } from './components/ui'
import { cn } from './utils/cn'

// Lazy load page components for code splitting
const Dashboard = lazy(() => import('./pages/Dashboard'))
const Timeline = lazy(() => import('./pages/Timeline'))
const Reports = lazy(() => import('./pages/Reports'))
const Focus = lazy(() => import('./pages/Focus'))
const Settings = lazy(() => import('./pages/Settings'))
const Privacy = lazy(() => import('./pages/Privacy'))
const Search = lazy(() => import('./pages/Search'))
const SessionReplay = lazy(() => import('./pages/SessionReplay'))
const Automation = lazy(() => import('./pages/Automation'))
const Updates = lazy(() => import('./pages/Updates'))

function App() {
  const { sidebarWidth, sidebarCollapsed, toggleSidebar, onResizeStart } = useShellLayout()
  const { isOpen: isPaletteOpen, open: openPalette, close: closePalette } = useCommandPalette()

  useKeyboardShortcuts({
    onEscape: () => {
      if (isPaletteOpen) closePalette()
    },
  })

  // Cmd+B to toggle sidebar
  // (handled in useShellLayout via separate effect would be cleaner,
  //  but keeping keyboard shortcut registration centralized)

  return (
    <div className="app-shell bg-white dark:bg-slate-950 text-slate-900 dark:text-white">
      {/* Row 1: TitleBar */}
      <TitleBar onSearchOpen={openPalette} />

      {/* Row 2: ActivityBar + SidePanel + MainContent */}
      <ActivityBar
        onToggleSidebar={toggleSidebar}
        sidebarCollapsed={sidebarCollapsed}
      />

      <SidePanel
        collapsed={sidebarCollapsed}
        width={sidebarWidth}
        onResizeStart={onResizeStart}
      />

      <main className={cn('overflow-y-auto', layout.mainContent.bg)}>
        <ErrorBoundary>
          <Suspense
            fallback={
              <div className="flex items-center justify-center h-full">
                <Spinner size="lg" />
              </div>
            }
          >
            <Routes>
              <Route path="/" element={<Dashboard />} />
              <Route path="/timeline" element={<Timeline />} />
              <Route path="/reports" element={<Reports />} />
              <Route path="/focus" element={<Focus />} />
              <Route path="/replay" element={<SessionReplay />} />
              <Route path="/automation" element={<Automation />} />
              <Route path="/updates" element={<Updates />} />
              <Route path="/settings" element={<Settings />} />
              <Route path="/privacy" element={<Privacy />} />
              <Route path="/search" element={<Search />} />
            </Routes>
          </Suspense>
        </ErrorBoundary>
      </main>

      {/* Row 3: StatusBar */}
      <StatusBar />

      {/* Overlay: Command Palette */}
      <CommandPalette
        isOpen={isPaletteOpen}
        onClose={closePalette}
        onToggleSidebar={toggleSidebar}
      />
    </div>
  )
}

export default App
```

**Step 2: Verify build**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: Build succeeds

**Step 3: Verify dev server**

Run: `cd crates/oneshim-web/frontend && pnpm dev`
Expected: Opens in browser with new desktop layout. Activity bar on left, side panel, main content.

**Step 4: Commit**

```bash
git add crates/oneshim-web/frontend/src/App.tsx
git commit -m "feat(frontend): replace web-style navbar with desktop shell layout

Replaces horizontal top navigation with VS Code-style layout:
- Custom titlebar with platform-aware window controls
- Activity bar with icon navigation (3 groups)
- Resizable side panel with per-page tree view
- Full status bar with connection/metrics/version
- Command palette (Cmd+K)"
```

---

### Task 11: Adapt page wrappers for full-height layout

Each page needs: remove top-level max-width constraint, add `h-full overflow-y-auto`, keep internal structure unchanged.

**Files:**
- Modify: `crates/oneshim-web/frontend/src/pages/Dashboard.tsx`
- Modify: `crates/oneshim-web/frontend/src/pages/Timeline.tsx`
- Modify: `crates/oneshim-web/frontend/src/pages/Reports.tsx`
- Modify: `crates/oneshim-web/frontend/src/pages/Focus.tsx`
- Modify: `crates/oneshim-web/frontend/src/pages/Settings.tsx`
- Modify: `crates/oneshim-web/frontend/src/pages/Privacy.tsx`
- Modify: `crates/oneshim-web/frontend/src/pages/Search.tsx`
- Modify: `crates/oneshim-web/frontend/src/pages/SessionReplay.tsx`
- Modify: `crates/oneshim-web/frontend/src/pages/Automation.tsx`
- Modify: `crates/oneshim-web/frontend/src/pages/Updates.tsx`

**Step 1: For each page, find and update the outermost return wrapper**

The pattern for each page is the same. Find the outermost `<div>` in the return statement and change:

```tsx
// Before: (typical pattern)
return (
  <div className="space-y-6">

// After:
return (
  <div className="h-full overflow-y-auto p-6 space-y-6">
```

Specific changes per page (the outermost div class in each return statement):

1. **Dashboard.tsx** line ~100: `"space-y-6"` → `"h-full overflow-y-auto p-6 space-y-6"`
2. **Timeline.tsx**: `"space-y-6"` → `"h-full overflow-y-auto p-6 space-y-6"`
3. **Reports.tsx**: `"space-y-6"` → `"h-full overflow-y-auto p-6 space-y-6"`
4. **Focus.tsx**: `"space-y-6"` → `"h-full overflow-y-auto p-6 space-y-6"`
5. **Settings.tsx**: `"space-y-6"` → `"h-full overflow-y-auto p-6 space-y-6"`
6. **Privacy.tsx**: `"space-y-6"` → `"h-full overflow-y-auto p-6 space-y-6"`
7. **Search.tsx**: `"space-y-6"` → `"h-full overflow-y-auto p-6 space-y-6"`
8. **SessionReplay.tsx**: Check actual wrapper class, add `h-full overflow-y-auto p-6`
9. **Automation.tsx**: Check actual wrapper class, add `h-full overflow-y-auto p-6`
10. **Updates.tsx**: Check actual wrapper class, add `h-full overflow-y-auto p-6`

**Step 2: Verify build**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: Build succeeds

**Step 3: Verify dev server — check each page navigates correctly**

Run: `cd crates/oneshim-web/frontend && pnpm dev`
Expected: Each page fills the main content area, scrolls independently, no content overflow.

**Step 4: Commit**

```bash
git add crates/oneshim-web/frontend/src/pages/
git commit -m "feat(frontend): adapt all 10 page wrappers for desktop shell full-height layout"
```

---

### Task 12: Add Cmd+B sidebar toggle to keyboard shortcuts

**Files:**
- Modify: `crates/oneshim-web/frontend/src/hooks/useKeyboardShortcuts.ts`

**Step 1: Add Cmd+B / Ctrl+B handler**

In the `handleKeyDown` function, add at the top (before the `target.tagName` checks):

```typescript
// Cmd+B / Ctrl+B: toggle sidebar (works even when focused in inputs)
if ((event.metaKey || event.ctrlKey) && event.key === 'b') {
  event.preventDefault()
  handlers.onToggleSidebar?.()
  return
}
```

And add `onToggleSidebar` to the `ShortcutHandlers` interface:

```typescript
interface ShortcutHandlers {
  onHelp?: () => void
  onEscape?: () => void
  onToggleSidebar?: () => void  // NEW
  onArrowLeft?: () => void
  // ... rest unchanged
}
```

**Step 2: Update `getShortcutsList()` to include the new shortcut:**

Add:
```typescript
{ key: '⌘B', description: 'サイドバー表示/非表示' },
{ key: '⌘K', description: 'コマンドパレット' },
```

**Step 3: Wire it in App.tsx**

Update the `useKeyboardShortcuts` call in `App.tsx`:

```typescript
useKeyboardShortcuts({
  onEscape: () => {
    if (isPaletteOpen) closePalette()
  },
  onToggleSidebar: toggleSidebar,
})
```

**Step 4: Verify build**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: Build succeeds

**Step 5: Commit**

```bash
git add crates/oneshim-web/frontend/src/hooks/useKeyboardShortcuts.ts crates/oneshim-web/frontend/src/App.tsx
git commit -m "feat(frontend): add Cmd+B sidebar toggle and Cmd+K command palette shortcuts"
```

---

### Task 13: Add i18n keys for new shell components

**Files:**
- Modify: `crates/oneshim-web/frontend/src/i18n/locales/en.json`
- Modify: `crates/oneshim-web/frontend/src/i18n/locales/ko.json`
- Modify: `crates/oneshim-web/frontend/src/i18n/locales/ja.json` (if exists)
- Modify: `crates/oneshim-web/frontend/src/i18n/locales/zh.json` (if exists)

**Step 1: Add keys to each locale file**

Add to the appropriate section in each locale:

English (`en.json`):
```json
{
  "commandPalette": {
    "placeholder": "Type a command or search...",
    "noResults": "No results found"
  },
  "statusBar": {
    "connected": "Connected",
    "offline": "Offline",
    "autoOn": "Auto: ON",
    "autoOff": "Auto: OFF"
  },
  "shortcuts": {
    "toggleSidebar": "Toggle Sidebar",
    "commandPalette": "Command Palette"
  }
}
```

Korean (`ko.json`):
```json
{
  "commandPalette": {
    "placeholder": "명령어 또는 검색어를 입력하세요...",
    "noResults": "결과가 없습니다"
  },
  "statusBar": {
    "connected": "연결됨",
    "offline": "오프라인",
    "autoOn": "자동화: 켜짐",
    "autoOff": "자동화: 꺼짐"
  },
  "shortcuts": {
    "toggleSidebar": "사이드바 토글",
    "commandPalette": "커맨드 팔레트"
  }
}
```

**Step 2: Verify build**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: Build succeeds

**Step 3: Commit**

```bash
git add crates/oneshim-web/frontend/src/i18n/
git commit -m "feat(frontend): add i18n keys for command palette, status bar, and shortcuts"
```

---

### Task 14: Update Tauri configuration for custom titlebar

**Files:**
- Modify: `src-tauri/tauri.conf.json`

**Step 1: Set decorations to false for custom titlebar**

In `tauri.conf.json`, inside `app.windows[0]`, add:

```json
"decorations": false,
"transparent": false
```

This hides the OS native titlebar so the custom WebView titlebar takes over.

**Step 2: Verify the Tauri config is valid**

Run: `cd src-tauri && cargo check`
Expected: Succeeds (Tauri validates config at build time)

**Step 3: Commit**

```bash
git add src-tauri/tauri.conf.json
git commit -m "feat(tauri): hide native titlebar for custom WebView titlebar"
```

---

### Task 15: Final integration test and cleanup

**Step 1: Full build**

Run: `cd crates/oneshim-web/frontend && pnpm build`
Expected: Build succeeds

**Step 2: Workspace build**

Run: `cargo check --workspace`
Expected: All crates pass

**Step 3: Run existing tests**

Run: `cargo test --workspace`
Expected: All tests pass

**Step 4: Manual verification checklist (dev mode)**

Run: `cd crates/oneshim-web/frontend && pnpm dev`

Verify:
- [ ] TitleBar visible at top (32px), app name centered
- [ ] ActivityBar on left with 10 icons in 3 groups
- [ ] Clicking icon navigates to page and opens SidePanel
- [ ] SidePanel shows per-page treeview content
- [ ] SidePanel resizable by dragging right edge (200-400px)
- [ ] Cmd+B toggles sidebar
- [ ] Cmd+K opens command palette
- [ ] Command palette fuzzy search works
- [ ] Arrow keys + Enter navigate in command palette
- [ ] StatusBar at bottom shows connection status, CPU, RAM, version
- [ ] All 10 pages render correctly in main content area
- [ ] Dark mode toggle works
- [ ] Language switching works

**Step 5: Final commit (if any cleanup needed)**

```bash
git add -A
git commit -m "chore(frontend): desktop shell layout integration cleanup"
```
