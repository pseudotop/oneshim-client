# Desktop-Style WebView Layout Design

**Date**: 2026-03-04
**Status**: Approved
**Approach**: B+ (Shell + Page Adaptation + Selected C Elements)

## Summary

Redesign the ONESHIM React frontend from a web-style horizontal navigation layout to a VS Code/Cursor-style desktop application layout. The WebView should feel like a native desktop app with panels, sidebar, custom titlebar, and status bar.

## User Decisions

| Decision | Choice |
|----------|--------|
| Layout reference | VS Code / Cursor style |
| Titlebar | Custom titlebar (OS native hidden) |
| Sidebar navigation | All 10 pages in icon activity bar (3 groups) |
| Status bar | Full status bar (agent status, metrics, version) |
| Approach | B+: Shell + page adaptation + treeview/resize/command palette from C |

## Architecture

### Overall Layout (CSS Grid)

```
┌─────────────────────────────────────────────────────────────┐
│  TitleBar (32px)                                            │
│  [-webkit-app-region: drag]                                 │
│  macOS: traffic lights(L) + center title + search(R)        │
│  Windows: title(L) + search(C) + window buttons(R)          │
├──┬──────────┬───────────────────────────────────────────────┤
│A │ SidePanel │  MainContent                                 │
│c │ (200-     │  (flex-1, overflow-y-auto)                   │
│t │  400px,   │                                              │
│i │  resize-  │  Existing page components render here        │
│v │  able)    │  max-w removed, h-full stretch               │
│i │           │                                              │
│t │ TreeView  │                                              │
│y │ or sub-   │                                              │
│  │ menu per  │                                              │
│B │ page      │                                              │
│a │           │                                              │
│r │ (collaps- │                                              │
│  │  ible)    │                                              │
│48│           │                                              │
│px│           │                                              │
├──┴──────────┴───────────────────────────────────────────────┤
│  StatusBar (24px)                                           │
│  L: ● Connected | Auto: ON    R: CPU 2.1% | RAM 85MB | v   │
└─────────────────────────────────────────────────────────────┘
```

```css
.app-shell {
  display: grid;
  grid-template-rows: var(--titlebar-height) 1fr var(--statusbar-height);
  grid-template-columns: var(--activitybar-width) var(--sidebar-width) 1fr;
  height: 100vh;
  overflow: hidden;
}
```

### Key Principles

- `100vh` fixed — scrolling only inside MainContent
- ActivityBar: always visible (48px fixed)
- SidePanel: toggleable (Cmd+B), resizable (200-400px)
- MainContent: fills remaining space
- All dimensions/colors in design tokens — no hardcoding in components

## Design Tokens Extension

Add to existing `tokens.ts`:

```typescript
export const layout = {
  titleBar: {
    height: 'h-8',                    // 32px
    bg: 'bg-slate-100 dark:bg-slate-900',
    border: 'border-b border-slate-200 dark:border-slate-800',
    text: 'text-slate-600 dark:text-slate-400',
  },
  activityBar: {
    width: 'w-12',                     // 48px
    bg: 'bg-slate-50 dark:bg-slate-950',
    border: 'border-r border-slate-200 dark:border-slate-800',
    iconSize: 'w-5 h-5',
    iconDefault: 'text-slate-500 dark:text-slate-500',
    iconActive: 'text-teal-600 dark:text-teal-400',
    iconHover: 'hover:text-slate-700 dark:hover:text-slate-300',
    indicator: 'bg-teal-500',          // 2px left active indicator
  },
  sidePanel: {
    minWidth: 200,                     // px (CSS variable)
    maxWidth: 400,                     // px
    defaultWidth: 260,                 // px
    bg: 'bg-white dark:bg-slate-900',
    border: 'border-r border-slate-200 dark:border-slate-800',
    headerBg: 'bg-slate-50 dark:bg-slate-900',
    headerText: 'text-xs font-semibold uppercase tracking-wider text-slate-500',
    resizeHandle: 'w-1 hover:bg-teal-500 cursor-col-resize',
  },
  mainContent: {
    bg: 'bg-white dark:bg-slate-950',
    padding: 'p-6',
    scrollbar: 'overflow-y-auto scrollbar-thin',
  },
  statusBar: {
    height: 'h-6',                     // 24px
    bg: 'bg-teal-600 dark:bg-teal-700',
    text: 'text-white text-xs',
    itemHover: 'hover:bg-teal-500 dark:hover:bg-teal-600',
  },
  commandPalette: {
    overlay: 'bg-black/50',
    bg: 'bg-white dark:bg-slate-800',
    border: 'border border-slate-200 dark:border-slate-700',
    width: 'max-w-xl',                // 576px
    itemHover: 'bg-slate-100 dark:bg-slate-700',
  },
} as const;
```

CSS custom properties (in `index.css`):
```css
:root {
  --titlebar-height: 32px;
  --statusbar-height: 24px;
  --activitybar-width: 48px;
  --sidebar-width: 260px;
}
```

## Component Specifications

### 1. TitleBar.tsx

**Height**: 32px
**Platform behavior**:
- macOS: `data-tauri-drag-region` on entire bar. Traffic lights are native Tauri. Center: app title. Right: search trigger (Cmd+K).
- Windows: Left: app title. Center: search trigger. Right: custom minimize/maximize/close buttons using Tauri `appWindow` API.

**Interactions**:
- Double-click: toggle maximize (both platforms)
- Search button: opens CommandPalette

### 2. ActivityBar.tsx

**Width**: 48px fixed
**Structure** (3 groups separated by dividers):

```
Monitoring group:
  Dashboard  (LayoutDashboard icon)
  Sessions   (Clock icon)
  Events     (Zap icon)
  Processes  (Monitor icon)
---separator---
Data group:
  Frames     (Image icon)
  Stats      (BarChart3 icon)
  Tags       (Tag icon)
---separator---
Management group:
  Reports    (FileText icon)
---spacer (mt-auto)---
  Settings   (Settings icon)    [pinned bottom]
  About      (Info icon)        [pinned bottom]
```

**Active state**: 2px teal bar on left edge + icon color change to `teal-600/teal-400`
**Hover**: tooltip with page name on right side
**Click behavior**:
- Click inactive icon: navigate to page + open SidePanel
- Click active icon: toggle SidePanel visibility

### 3. SidePanel.tsx

**Width**: 200-400px, resizable via drag handle on right edge
**Default**: 260px
**Toggle**: Cmd+B (macOS) / Ctrl+B (Windows)
**State persistence**: `localStorage` key `oneshim-sidebar-width` + `oneshim-sidebar-collapsed`

**Per-page content**:

| Page | SidePanel Content |
|------|------------------|
| Dashboard | Widget list (treeview) — click scrolls to section |
| Sessions | Session list by date (treeview) — click opens detail |
| Events | Event type treeview — acts as filter |
| Processes | Process category treeview |
| Frames | Date/tag treeview |
| Stats | Chart list |
| Tags | Tag list with counts |
| Reports | Report type list |
| Settings | Settings section list (replaces tab navigation) |
| About | Info section list |

**TreeView component**: collapsible nodes with `ChevronRight`/`ChevronDown` icons, indent levels, item counts.

**Resize handle**: 1px wide, invisible by default, shows teal on hover, `cursor-col-resize`. Updates `--sidebar-width` CSS variable via JS.

### 4. StatusBar.tsx

**Height**: 24px
**Background**: brand teal (`bg-teal-600 dark:bg-teal-700`) — like VS Code's blue status bar

**Left items**:
- Agent status: green dot + "Connected" / red dot + "Offline" / yellow dot + "Error"
- Automation toggle: lightning icon + "Auto: ON/OFF" — clickable to toggle

**Right items**:
- CPU usage: `CPU 2.1%`
- RAM usage: `RAM 85MB`
- Last sync time: `↻ 3s ago`
- App version: `v0.1.5`

**Interactions**: Each item clickable for detail popover

### 5. CommandPalette.tsx

**Trigger**: Cmd+K (macOS) / Ctrl+K (Windows)
**Width**: max-w-xl (576px), centered horizontally, positioned ~25% from top
**Overlay**: semi-transparent black backdrop

**Item types**:
- `[page]` — Navigate to page (Dashboard, Sessions, etc.)
- `[action]` — Toggle Dark Mode, Toggle Automation, Toggle Sidebar
- `[search]` — Full-text search (delegates to existing search)

**Behavior**:
- Auto-focus on input
- Fuzzy match filtering as you type
- Arrow keys to navigate, Enter to select
- Escape to close
- Recent items shown when input is empty

## Page Adaptations

Each of the 10 page components needs minimal wrapper changes:

```tsx
// Before (current)
export default function Dashboard() {
  return (
    <div className="space-y-6">
      <div className="flex justify-between">
        <h1 className="text-2xl font-bold">Dashboard</h1>
      </div>
      {/* content */}
    </div>
  )
}

// After
export default function Dashboard() {
  return (
    <div className="h-full overflow-y-auto p-6 space-y-6">
      {/* h1 removed — page title shown in SidePanel header */}
      {/* content */}
    </div>
  )
}
```

Changes per page:
- Remove `max-w-7xl mx-auto` constraint (inherited from old App.tsx)
- Add `h-full overflow-y-auto` to root div
- Move page title to SidePanel header (remove h1 from page body)
- Keep all internal card/chart/table structure unchanged

## New File Structure

```
src/
├── components/
│   ├── shell/              # NEW: Desktop shell components
│   │   ├── TitleBar.tsx
│   │   ├── ActivityBar.tsx
│   │   ├── SidePanel.tsx
│   │   ├── StatusBar.tsx
│   │   ├── CommandPalette.tsx
│   │   ├── ResizeHandle.tsx
│   │   ├── TreeView.tsx     # Reusable treeview component
│   │   └── index.ts
│   └── ui/                 # Existing UI primitives (unchanged)
├── hooks/
│   ├── useShellLayout.ts   # NEW: sidebar width, collapse state
│   ├── useCommandPalette.ts # NEW: command palette state
│   └── ...existing hooks
├── styles/
│   ├── tokens.ts           # MODIFIED: add layout tokens
│   └── variants.ts         # Unchanged
└── App.tsx                 # MODIFIED: new shell layout
```

## Estimated Scope

| Item | Lines |
|------|-------|
| TitleBar.tsx | ~80 |
| ActivityBar.tsx | ~120 |
| SidePanel.tsx | ~150 |
| StatusBar.tsx | ~100 |
| CommandPalette.tsx | ~180 |
| ResizeHandle.tsx | ~40 |
| TreeView.tsx | ~100 |
| useShellLayout.ts | ~60 |
| useCommandPalette.ts | ~50 |
| tokens.ts extension | ~50 |
| index.css additions | ~20 |
| App.tsx refactor | ~80 |
| 10 page wrapper adjustments | ~100 |
| **Total** | **~1130** |

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| Cmd+K / Ctrl+K | Open Command Palette |
| Cmd+B / Ctrl+B | Toggle SidePanel |
| Cmd+1-9 / Ctrl+1-9 | Switch to page by position |
| Escape | Close Command Palette / Close SidePanel |

## Dependencies

No new npm dependencies needed:
- `lucide-react` — already installed (icons)
- `@tauri-apps/api` — already in Tauri project (window controls)
- `clsx` + `tailwind-merge` — already installed (cn utility)

## Risks and Mitigations

| Risk | Probability | Mitigation |
|------|-------------|------------|
| Tauri drag region conflicts with interactive elements | Medium | Use `data-tauri-drag-region` only on non-interactive areas; exclude buttons |
| macOS traffic light positioning | Low | Tauri handles natively; adjust padding-left on titlebar |
| Performance with many treeview items | Low | Virtualize with fixed-height items if >100 |
| SidePanel resize jank | Low | Use CSS variable + requestAnimationFrame |
| Keyboard shortcut conflicts with OS | Low | Use Cmd prefix (macOS) / Ctrl prefix (Windows) |
