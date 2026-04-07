# Sidebar Sub-Pathname Routing Refactor

**Date**: 2026-04-07
**Status**: Approved (rev.2 — post deep review)
**Scope**: oneshim-web frontend (crates/oneshim-web/frontend)

## Problem

The current sidebar navigation uses `section-ID + scrollIntoView` for in-page navigation. This approach has several issues:

1. **5/14 pages have broken sidebar ↔ section ID mappings** (Updates, Recalibration, Coaching, Settings, Replay)
2. **3 pages have no sidebar config** (Chat, Policies, Playbooks)
3. **No deep linking** — cannot share or bookmark a specific section
4. **Browser back/forward broken** — scroll position doesn't map to history
5. **Dual maintenance** — `pageSidebarConfig` and page section IDs are separate sources of truth that drift apart
6. **Settings uses a third pattern** (`?tab=X` query params) inconsistent with the rest

## Solution

Replace section-ID scrolling with **sub-pathname routing** using a single route config object as the source of truth.

### Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Sub-pathname vs section-scroll | Sub-pathname | Deep-linkable, browser history works, cleaner separation |
| Parent path redirect | `<Navigate to={defaultChild} replace />` | Rust compatibility — Rust emits parent paths (`/settings`), frontend redirects to default child (`/settings/general`) |
| Single source of truth | `route-tree.ts` config object | Route definitions generate both `<Route>` elements and sidebar nodes — structural drift impossible |
| Approach | Route Config Object + auto-generation (Option A) | Keeps current `<Routes>` pattern, no `createBrowserRouter` migration needed |
| Pre-commit guard | `verify-route-integrity.sh` | Catches component/i18n/ActivityBar/Rust path mismatches at commit time |
| Dashboard root path | Stay at `/` | ActivityBar, CommandPalette, E2E, Rust emit all reference `/`. Moving to `/dashboard` is a breaking change with no benefit. Children use `/overview`, `/monitoring`, etc. |
| Settings form state | `SettingsFormContext` at Layout level | Tabs share `useSettingsForm` hook — extracting to sub-routes requires React Context to maintain shared save state |

## Architecture

### 1. Route Config Object (`src/routes/route-tree.ts`)

Single source of truth for routing, sidebar, and ActivityBar:

```ts
export interface RouteNode {
  path: string                              // absolute path e.g. '/settings'
  labelKey: string                          // i18n key
  icon?: ComponentType                      // ActivityBar icon
  defaultChild?: string                     // index redirect target
  component: LazyExoticComponent | ComponentType  // layout or page
  children?: RouteLeaf[]                    // sub-routes
  group?: 'monitor' | 'data' | 'manage'    // ActivityBar grouping
}

export interface RouteLeaf {
  path: string                              // relative path e.g. 'general'
  labelKey: string                          // i18n key
  component: LazyExoticComponent | ComponentType
}
```

- `children` present → Layout + `<Outlet>` pattern with index redirect
- `children` absent → single-page route (no sidebar sub-nav)
- `group` maps to ActivityBar icon groups (monitor/data/manage)

### 2. Route Rendering (`src/routes/RouteRenderer.tsx`)

Auto-generates `<Route>` elements from `routeTree`:

```tsx
<Routes>
  {routeTree.map(node =>
    node.children ? (
      <Route path={node.path} element={<node.component />}>
        <Route index element={<Navigate to={node.defaultChild!} replace />} />
        {node.children.map(child => (
          <Route path={child.path} element={<child.component />} />
        ))}
      </Route>
    ) : (
      <Route path={node.path} element={<node.component />} />
    )
  )}
  <Route path="*" element={<Navigate to="/" replace />} />
</Routes>
```

`App.tsx` replaces manual Route list with `<RouteRenderer />`.

**Validation**: RouteRenderer asserts at dev time that every node with children has a valid `defaultChild` matching one of its children paths. Logs a warning if parent path is visited without redirect.

### 3. SidePanel Auto-Generation

SidePanel derives its nodes from `routeTree` instead of `pageSidebarConfig`:

```tsx
const currentRoute = routeTree.find(r =>
  r.path === '/'
    ? location.pathname === '/' || location.pathname.startsWith('/')
    : location.pathname.startsWith(r.path)
)
const sidebarNodes = currentRoute?.children?.map(child => ({
  id: child.path,
  label: t(child.labelKey),
}))

// Active state from pathname
const activeChild = location.pathname.split('/').pop()

const handleSelect = (childPath: string) => {
  navigate(`${currentRoute.path === '/' ? '' : currentRoute.path}/${childPath}`)
}
```

Changes:
- **Delete** `pageSidebarConfig` static object
- **Delete** `?tab=X` special case logic for Settings (line 164)
- **Replace** `scrollIntoView` with `navigate()`
- **Active state** derived from `location.pathname` instead of local `selectedNodeId` or `searchParams`
- Pages without `children` → sidebar shows title only (no sub-nav)

### 4. Layout + Outlet Pattern

Pages with children use a Layout wrapper containing `<Outlet />`:

```tsx
// src/pages/settings/SettingsLayout.tsx
export default function SettingsLayout() {
  return (
    <SettingsFormProvider>
      <div className="min-h-full p-6 space-y-6">
        <h1>{t('nav.settings')}</h1>
        <Outlet />
        <UnsavedChangesBar />  {/* floating save button — reads SettingsFormContext */}
      </div>
    </SettingsFormProvider>
  )
}
```

#### Settings Form Context (critical)

Settings tabs share form state via `useSettingsForm` hook. This MUST be preserved through a Context provider at the Layout level:

```tsx
// src/pages/settings/SettingsFormContext.tsx
const SettingsFormContext = createContext<SettingsFormReturn | null>(null)

export function SettingsFormProvider({ children }) {
  const { data: settingsData } = useQuery(['settings'], fetchSettings)
  const form = useSettingsForm(settingsData)
  return (
    <SettingsFormContext.Provider value={form}>
      {children}
    </SettingsFormContext.Provider>
  )
}

export function useSettingsFormContext() {
  const ctx = useContext(SettingsFormContext)
  if (!ctx) throw new Error('useSettingsFormContext must be inside SettingsFormProvider')
  return ctx
}
```

Each tab component changes from receiving props to using `useSettingsFormContext()`.

#### Timeline Filter State

Timeline sections share filter state (viewMode, appFilter, importanceFilter, tagFilter, dateRange). Options:

- **URL query params** for filters (preferred — enables deep linking of filtered views)
- TimelineLayout reads URL params, passes to child routes via context or props via Outlet context

### 5. Page Split Plan

| Page | Current | Split Into | Sub-routes | Notes |
|------|---------|-----------|------------|-------|
| **Dashboard** (10K) | `/` with 6 sections | DashboardLayout at `/` | `overview`, `monitoring`, `insights` | Root path preserved. DashboardDay at `/day` |
| **Settings** (14K) | `/settings?tab=X` | SettingsLayout | `general`, `privacy`, `monitoring`, `ai-automation`, `data`, `coaching`, `sync`, `audio`, `advanced` | All 9 tabs as sub-routes. SettingsFormContext required |
| **Automation** (32K) | 3 sections flat | AutomationLayout | `policies`, `commands`, `history` | Sections loosely coupled — clean extraction |
| **Timeline** (32K) | filters nested | TimelineLayout | `all`, `filters` | Filter state in URL params or Layout context |
| **Updates** (10K) | 5 sections flat | UpdatesLayout | `status`, `channel` | |
| **Focus** (13K) | 4 sections flat | FocusLayout | `score`, `sessions`, `interruptions` | |
| **Reports** (16K) | 3 sections flat | ReportsLayout | `activity`, `focus`, `export` | |
| **Privacy** (22K) | 3 sections flat | PrivacyLayout | `data`, `consent`, `export` | |
| **Coaching** (6K) | 2 sections (broken) | CoachingLayout | `goals`, `history` | |
| **Recalibration** (13K) | 2 sections (broken) | RecalibrationLayout | `segments`, `overrides` | |
| **Audit** (7.8K) | 2 sections flat | AuditLayout | `summary`, `entries` | |
| **Replay** (8.8K) | 2 sections | ReplayLayout | `timeline`, `events` | |
| **Search** (17K) | 2 sidebar nodes | Single page | No children | |
| **Chat** (24K) | No sidebar config | Single page | No children (own internal nav) | |
| **Policies** (16K) | No sidebar config | Single page | No children | |
| **Playbooks** (13K) | No sidebar config | Single page | No children | |
| **DashboardDay** (6.7K) | `/dashboard/day` | Moved to `/day` | No children | Sibling of Dashboard under root |

### 6. Rust Compatibility

**No Rust code changes required.** Parent-path redirect handles all existing emit calls:

| Rust emit | Current target | After refactor |
|-----------|---------------|----------------|
| `emit("navigate", "/settings")` | Settings page | → redirect `/settings/general` |
| `emit("navigate", "/automation")` | Automation page | → redirect `/automation/policies` |
| `emit("navigate", "/updates")` | Updates page | → redirect `/updates/status` |
| `emit("navigate:chat", {...})` | `/chat?sid=X` | No change (Chat has no children) |
| `emit("tray-toggle-automation")` | `/settings` | → redirect `/settings/general` |

The generic `emit("navigate", path)` continues to work for any valid route path. CommandPalette also emits parent paths — redirects handle this transparently.

### 7. Pre-Commit Verification (`scripts/verify-route-integrity.sh`)

Runs on changes to `src/routes/`, `src/pages/`, `src/components/shell/`:

**5 checks:**

1. **Component existence** — every `component` in routeTree resolves to an importable file
2. **i18n key coverage** — every `labelKey` exists in both `en.json` and `ko.json`
3. **ActivityBar sync** — routeTree top-level entries with `group` match ActivityBar icon definitions
4. **Rust path compatibility** — all Rust `emit("navigate", "/xxx")` paths exist as routeTree parent paths
5. **defaultChild validity** — every node with children has `defaultChild` matching one of its children's `path`

Script outputs clear error messages:
```
[route-integrity] missing component: src/pages/settings/SyncTab.tsx
[route-integrity] missing i18n key: settings.tabs.sync (ko.json)
[route-integrity] ActivityBar missing route: /coaching
[route-integrity] Rust emit path not in routeTree: /unknown
[route-integrity] invalid defaultChild: /settings defaultChild=general not in children
```

**lefthook.yml addition:**
```yaml
route-integrity:
  glob: "crates/oneshim-web/frontend/src/{routes,pages,components/shell}/**/*.{ts,tsx}"
  run: ./scripts/verify-route-integrity.sh
```

## Files Changed

### New files
- `src/routes/route-tree.ts` — route config (single source of truth)
- `src/routes/RouteRenderer.tsx` — auto Route generation + validation
- `src/routes/index.ts` — exports
- `src/pages/settings/SettingsFormContext.tsx` — shared form context for Settings tabs
- `src/pages/*/Layout.tsx` — layout wrappers for 12 pages with children
- `scripts/verify-route-integrity.sh` — pre-commit verification

### Modified files
- `src/App.tsx` — replace manual Routes with `<RouteRenderer />`
- `src/components/shell/SidePanel.tsx` — delete `pageSidebarConfig`, use routeTree, remove `?tab=X` logic
- `src/components/shell/ActivityBar.tsx` — derive from routeTree (optional, can be phase 2)
- `src/pages/Settings.tsx` → `src/pages/settings/SettingsLayout.tsx` — remove `?tab=X` logic, wrap with SettingsFormProvider + Outlet
- `src/pages/setting-tabs/*.tsx` — change from props to `useSettingsFormContext()`
- `src/hooks/useTauriEventBridge.ts` — no changes needed (navigate calls still work)
- `lefthook.yml` — add `route-integrity` hook
- `e2e/*.spec.ts` — update 31 files: `?tab=X` → `/settings/X`, section ID checks where paths change

### Deleted
- `pageSidebarConfig` in SidePanel.tsx (replaced by routeTree)
- `settings-utils.ts` `isSettingsTabId` / tab routing logic (replaced by sub-routes)

## Testing Strategy

- **Unit tests**: routeTree config validation (component/i18n resolution)
- **E2E tests**: update 31 existing test files, verify redirect behavior, add parent→child redirect tests
- **Manual**: verify tray menu navigation, command palette, deep linking, Settings unsaved changes bar
- **Pre-commit**: verify-route-integrity.sh catches drift automatically

## Migration Path

1. Create `route-tree.ts` + `RouteRenderer` alongside existing routes
2. Create `SettingsFormContext` — extract shared form state from Settings.tsx
3. Migrate Settings first (already has individual Tab components, highest complexity)
4. Migrate remaining pages: create Layout, extract sub-components, add to routeTree
5. Switch App.tsx to RouteRenderer once all pages migrated
6. Update SidePanel — delete pageSidebarConfig, use routeTree
7. Update E2E tests (31 files)
8. Add pre-commit hook
9. Delete old scrollIntoView logic and settings-utils.ts

## Review Notes (rev.2)

Issues found and resolved during deep review:

| Issue | Severity | Resolution |
|-------|----------|------------|
| Dashboard at `/` not `/dashboard` | CRITICAL | Keep at `/`, update spec |
| Settings sidebar missing sync/audio/advanced | CRITICAL | Include all 9 tabs in routeTree |
| Settings shared form state breaks on extraction | IMPORTANT | SettingsFormContext at Layout level |
| Timeline filter state coupling | IMPORTANT | URL query params for filters |
| E2E 31 files need path updates | IMPORTANT | Systematic migration in plan |
| SidePanel `?tab=X` special logic | IMPORTANT | Delete, use pathname matching |
| CommandPalette relies on redirect | IMPORTANT | RouteRenderer validates defaultChild |
| defaultChild validation missing | IMPORTANT | Added as 5th pre-commit check |
