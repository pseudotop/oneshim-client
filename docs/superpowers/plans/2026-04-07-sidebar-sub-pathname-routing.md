# Sidebar Sub-Pathname Routing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace section-ID scroll navigation with sub-pathname routing using a single route config as source of truth.

**Architecture:** `route-tree.ts` defines all routes with metadata. `RouteRenderer` auto-generates `<Route>` elements. `SidePanel` derives sidebar nodes from the same config. Pages with children use Layout + `<Outlet>` pattern. Parent paths redirect to `defaultChild` for Rust/CommandPalette compatibility.

**Tech Stack:** React 18, react-router-dom 6.27, TanStack Query 5, TypeScript, Biome

**Spec:** `docs/superpowers/specs/2026-04-07-sidebar-sub-pathname-routing-design.md`

### Critical Design Decisions (rev.2 review)

1. **Route ordering**: routeTree array MUST place leaf routes (no children) BEFORE parent routes with children. Dashboard (`/`) with `/*` MUST be last in the array to avoid catch-all consuming other routes. RouteRenderer sorts automatically: leaves first, then parents, root `/` last.

2. **Shared state pattern**: Use `useOutletContext<T>()` (react-router-dom v6 built-in) for Layout → child data passing. Layouts call `<Outlet context={sharedData} />`, children call `useOutletContext<T>()`. No re-querying.

3. **Settings complexity**: AiAutomationTab has 49 props including provider surfaces, endpoint probes, model catalogs. SettingsFormContext must include ALL of these — it wraps both `useSettingsForm()` AND `useSettingsData()` return values. Tabs access the full combined context.

4. **Dashboard sub-route mapping**:
   - `/overview` — section-overview (title, connection, realtime metrics) + TodaySummary + StatCards
   - `/monitoring` — section-metrics (CPU/memory chart) + section-processes (process list) + AppUsageChart
   - `/insights` — section-heatmap + section-focus (FocusWidget) + section-updates (UpdatePanel)

5. **Onboarding**: NOT in routeTree. Rendered conditionally outside AppShell before routes load. No changes needed.

6. **DashboardDay**: Moves from `/dashboard/day` to `/day`. Affects: App.tsx, ActivityBar.tsx, SidePanel.tsx, dashboard-day.spec.ts, recalibration.spec.ts, api/standalone.ts.

7. **CommandPalette**: Keeps hardcoded parent paths. Works via defaultChild redirects. verify-route-integrity.sh validates all CommandPalette paths have matching routeTree entries with defaultChild.

---

## File Structure

### New files
```
src/routes/
  route-tree.ts          -- RouteNode/RouteLeaf types + routeTree config array
  RouteRenderer.tsx       -- auto-generates <Route> from routeTree
  index.ts                -- re-exports

src/pages/settings/
  SettingsLayout.tsx      -- Layout with SettingsFormProvider + Outlet
  SettingsFormContext.tsx  -- shared form context for all settings tabs

src/pages/dashboard/
  DashboardLayout.tsx     -- Layout with Outlet
  OverviewSection.tsx     -- overview + stat cards + realtime metrics
  MonitoringSection.tsx   -- CPU/memory chart + process list + app usage
  InsightsSection.tsx     -- heatmap + focus + updates

src/pages/automation/
  AutomationLayout.tsx    -- Layout with Outlet
  PoliciesSection.tsx     -- status + stats + policies
  CommandsSection.tsx     -- presets/commands
  HistorySection.tsx      -- audit log

src/pages/updates/
  UpdatesLayout.tsx       -- Layout with Outlet
  StatusSection.tsx       -- version info + update panel
  ChannelSection.tsx      -- channel selection + features + policy

src/pages/focus/
  FocusLayout.tsx         -- Layout with Outlet
  ScoreSection.tsx        -- focus score + metrics
  SessionsSection.tsx     -- focus sessions list
  InterruptionsSection.tsx -- interruptions list

src/pages/reports/
  ReportsLayout.tsx       -- Layout with Outlet
  ActivityReport.tsx      -- activity report
  FocusReport.tsx         -- focus report
  ExportSection.tsx       -- data export

src/pages/privacy-page/
  PrivacyLayout.tsx       -- Layout with Outlet
  DataSection.tsx         -- data controls
  ConsentSection.tsx      -- consent management
  ExportSection.tsx       -- data export/delete

src/pages/coaching/
  CoachingLayout.tsx      -- Layout with Outlet
  GoalsSection.tsx        -- coaching goals
  HistorySection.tsx      -- coaching event history

src/pages/recalibration/
  RecalibrationLayout.tsx -- Layout with Outlet
  SegmentsSection.tsx     -- activity segments
  OverridesSection.tsx    -- override history

src/pages/audit/
  AuditLayout.tsx         -- Layout with Outlet (replaces index.tsx)
  SummarySection.tsx      -- audit summary
  EntriesSection.tsx      -- audit entries

src/pages/timeline/
  TimelineLayout.tsx      -- Layout with Outlet + filter context
  AllFrames.tsx           -- all frames view
  FiltersView.tsx         -- filtered view

src/pages/session-replay/
  ReplayLayout.tsx        -- Layout with Outlet
  TimelineSection.tsx     -- replay timeline
  EventsSection.tsx       -- event log

scripts/
  verify-route-integrity.sh -- pre-commit route validation
```

### Modified files
```
src/App.tsx                         -- replace manual Routes with <RouteRenderer />
src/components/shell/SidePanel.tsx  -- delete pageSidebarConfig, use routeTree + navigate()
src/pages/setting-tabs/*.tsx        -- change from props to useSettingsFormContext()
lefthook.yml                        -- add route-integrity hook
e2e/*.spec.ts                       -- update paths (31 files)
```

### Deleted
```
src/pages/Settings.tsx              -- replaced by settings/SettingsLayout.tsx
src/pages/settings-utils.ts         -- isSettingsTabId no longer needed
pageSidebarConfig in SidePanel.tsx  -- replaced by routeTree
```

---

## Task 1: Route Config Infrastructure

**Files:**
- Create: `src/routes/route-tree.ts`
- Create: `src/routes/RouteRenderer.tsx`
- Create: `src/routes/index.ts`

- [ ] **Step 1: Create route-tree.ts with types and initial config**

```ts
// src/routes/route-tree.ts
import type { ComponentType, LazyExoticComponent } from 'react'

export interface RouteNode {
  path: string
  labelKey: string
  icon?: ComponentType<{ className?: string }>
  defaultChild?: string
  component: LazyExoticComponent<ComponentType> | ComponentType
  children?: RouteLeaf[]
  group?: 'monitor' | 'data' | 'manage'
  bottom?: boolean
}

export interface RouteLeaf {
  path: string
  labelKey: string
  component: LazyExoticComponent<ComponentType> | ComponentType
}

// Initially empty -- populated as pages are migrated in Tasks 2-3
export const routeTree: RouteNode[] = []
```

- [ ] **Step 2: Create RouteRenderer.tsx**

```tsx
// src/routes/RouteRenderer.tsx
import { Suspense } from 'react'
import { Navigate, Route, Routes } from 'react-router-dom'
import { Spinner } from '../components/ui'
import { routeTree } from './route-tree'

function validateRouteTree() {
  if (import.meta.env.DEV) {
    for (const node of routeTree) {
      if (node.children && node.defaultChild) {
        const match = node.children.some((c) => c.path === node.defaultChild)
        if (!match) {
          console.warn(
            `[RouteRenderer] ${node.path} defaultChild="${node.defaultChild}" ` +
            `not found in children: [${node.children.map((c) => c.path).join(', ')}]`,
          )
        }
      }
    }
  }
}

export default function RouteRenderer() {
  validateRouteTree()

  // Sort: leaf routes first, parent routes after, root "/" last.
  // Prevents "/" catch-all from consuming other routes.
  const sorted = [...routeTree].sort((a, b) => {
    if (a.path === '/') return 1
    if (b.path === '/') return -1
    if (a.children && !b.children) return 1
    if (!a.children && b.children) return -1
    return 0
  })

  return (
    <Suspense
      fallback={
        <div className="flex min-h-full items-center justify-center">
          <Spinner />
        </div>
      }
    >
      <Routes>
        {sorted.map((node) =>
          node.children ? (
            <Route key={node.path} path={`${node.path}/*`} element={<node.component />}>
              {node.defaultChild && (
                <Route index element={<Navigate to={node.defaultChild} replace />} />
              )}
              {node.children.map((child) => (
                <Route key={child.path} path={child.path} element={<child.component />} />
              ))}
            </Route>
          ) : (
            <Route key={node.path} path={node.path} element={<node.component />} />
          ),
        )}
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </Suspense>
  )
}
```

Note: Parent routes with children use `path={node.path}/*` (trailing `/*`) so nested routes work with react-router-dom v6.

- [ ] **Step 3: Create index.ts re-exports**

```ts
// src/routes/index.ts
export { routeTree } from './route-tree'
export type { RouteNode, RouteLeaf } from './route-tree'
export { default as RouteRenderer } from './RouteRenderer'
```

- [ ] **Step 4: Verify TypeScript compiles**

Run: `cd crates/oneshim-web/frontend && npx tsc --noEmit`
Expected: PASS

- [ ] **Step 5: Commit**

```
git add src/routes/
git commit -m "feat: add route-tree config infrastructure + RouteRenderer"
```

---

## Task 2: Settings Sub-Route Migration

Settings is the most complex page (shared form state across 9 tabs). Do first to validate the pattern.

**Files:**
- Create: `src/pages/settings/SettingsFormContext.tsx`
- Create: `src/pages/settings/SettingsLayout.tsx`
- Modify: `src/pages/setting-tabs/GeneralTab.tsx` (and all 8 other tabs)
- Modify: `src/routes/route-tree.ts`
- Delete: `src/pages/Settings.tsx`
- Delete: `src/pages/settings-utils.ts`

- [ ] **Step 1: Create SettingsFormContext**

Read `src/pages/hooks/useSettingsForm.ts` and `src/pages/hooks/useSettingsData.ts` to determine the return types. Create the context that wraps `useSettingsForm` and makes it available to all child routes.

Key: The context must include `formData`, all `handle*Change` callbacks, `handleSubmit`, `hasUnsavedChanges`, `saveMutation`, `saveDisabled`, `handleRevertChanges`, and any `settingsData` properties (like `updateStatus`) that tabs need.

- [ ] **Step 2: Create SettingsLayout**

The layout wraps children in `SettingsFormProvider`, renders the page title, the `<form id="settings-form">` wrapper, `<Outlet />` for child routes, and the floating unsaved-changes bar.

Extract the unsaved-changes bar from current Settings.tsx (lines 109-140).

- [ ] **Step 3: Migrate all 9 tab components from props to context**

For each tab in `src/pages/setting-tabs/`:
- Remove the props interface
- Replace prop access with `const form = useSettingsFormContext()`
- Map old prop names to context properties (e.g., `props.formData` -> `form.formData`, `props.onRootChange` -> `form.handleRootChange`)

Tabs to migrate: GeneralTab, PrivacyTab, MonitoringTab, AiAutomationTab (ai-automation/index.tsx), DataStorageTab, CoachingSettingsTab, SyncTab, AudioTab, AdvancedTab.

- [ ] **Step 4: Add Settings to routeTree**

Add the Settings entry with all 9 children to `routeTree` in route-tree.ts. Use lazy imports for each tab component.

- [ ] **Step 5: Delete old Settings.tsx and settings-utils.ts**

- [ ] **Step 6: Verify**

Run: `npx tsc --noEmit`
Run: `pnpm dev` -> navigate to `/settings` -> verify redirect to `/settings/general`
Verify: Tab switching via sidebar, unsaved changes bar, save/revert functionality

- [ ] **Step 7: Commit**

```
git commit -m "feat: migrate Settings to sub-pathname routing with SettingsFormContext"
```

---

## Task 3: Migrate Remaining 11 Pages

Each page follows the same pattern: create Layout with `<Outlet />`, extract sections into sub-components, add to routeTree. Commit after each page.

**Order** (by complexity/priority):

### 3a: Automation (32K, loosely coupled)
- Create AutomationLayout, PoliciesSection, CommandsSection, HistorySection
- Sections share queryClient (global) but have independent state/queries
- Section state: `presetTab`, `runningPreset` stay in CommandsSection; `auditFilter` stays in HistorySection
- Shared queries (automationStatus, automationStats) live in Layout and pass via Outlet context or re-query in each section

### 3b: Dashboard (root `/`)
- Create DashboardLayout at path `/`
- Children: `overview`, `monitoring`, `insights`
- DashboardDay moves from `/dashboard/day` to `/day` (sibling route, not child)
- SSE connection (`useSSE`) and summary query live in Layout
- ActivityBar link for dashboard-day updates from `/dashboard/day` to `/day`

### 3c: Updates (broken sidebar)
- Create UpdatesLayout with children: `status`, `channel`
- Current sidebar declares `status, history` but page has `status, version, channel, features, policy`
- Consolidate into 2 clean sub-routes

### 3d: Focus
- Create FocusLayout with children: `score`, `sessions`, `interruptions`
- Focus queries (metrics, sessions, interruptions, suggestions) live in Layout

### 3e: Reports
- Create ReportsLayout with children: `activity`, `focus`, `export`

### 3f: Privacy
- Create PrivacyLayout with children: `data`, `consent`, `export`

### 3g: Timeline (filter state complexity)
- Create TimelineLayout with children: `all`, `filters`
- Filter state (`viewMode`, `appFilter`, `importanceFilter`, `tagFilter`, `dateRange`) managed in Layout
- Pass to children via Outlet context or URL query params

### 3h: Coaching (broken sidebar)
- Create CoachingLayout with children: `goals`, `history`

### 3i: Recalibration (broken sidebar)
- Create RecalibrationLayout with children: `segments`, `overrides`

### 3j: Audit
- Refactor `audit/index.tsx` into AuditLayout with children: `summary`, `entries`

### 3k: Replay
- Create ReplayLayout with children: `timeline`, `events`

### 3l: Single-page routes (no Layout)
- Add Search, Chat, Policies, Playbooks, DashboardDay to routeTree as leaf nodes

- [ ] **Step per page: Create Layout + extract + add to routeTree + commit**

Each commit: `feat: migrate {PageName} to sub-pathname routing`

---

## Task 4: Wire RouteRenderer + Update SidePanel

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/components/shell/SidePanel.tsx`

- [ ] **Step 1: Replace manual Routes in App.tsx**

Remove all `<Route>` elements and lazy imports. Replace `<Routes>...</Routes>` with `<RouteRenderer />`.

- [ ] **Step 2: Rewrite SidePanel**

Delete `pageSidebarConfig` and `translateNodes`. Replace with routeTree-based node derivation:
- Find current route from `routeTree` using `location.pathname`
- Map children to TreeView nodes
- Handle selection via `navigate()` instead of `scrollIntoView()`
- Active state from pathname matching
- Delete the Settings-specific `?tab=X` handling (line 164)

- [ ] **Step 3: Verify all navigation paths**

Test: ActivityBar, SidePanel, CommandPalette, Tauri tray (if available), browser URL bar, back/forward.

- [ ] **Step 4: Commit**

```
git commit -m "feat: wire RouteRenderer + update SidePanel to use routeTree"
```

---

## Task 5: Update E2E Tests

**Files:**
- Modify: `e2e/*.spec.ts` (31 files)
- Create: `e2e/routing.spec.ts`

- [ ] **Step 1: Update Settings tests**

Find all `page.goto('/settings?tab=X')` and change to `page.goto('/settings/X')`.

- [ ] **Step 2: Update section-ID tests for pages that are now sub-routes**

For each page that moved to sub-routes, update `page.goto` calls and section selectors as needed.

- [ ] **Step 3: Add parent redirect tests**

Create `e2e/routing.spec.ts` to verify parent paths redirect correctly.

- [ ] **Step 4: Run E2E suite**

Run: `pnpm exec playwright test`
Expected: All pass

- [ ] **Step 5: Commit**

```
git commit -m "test(e2e): update paths for sub-pathname routing"
```

---

## Task 6: Pre-Commit Route Integrity Verification

**Files:**
- Create: `scripts/verify-route-integrity.sh`
- Modify: `lefthook.yml`

- [ ] **Step 1: Create verify-route-integrity.sh**

Shell script with 5 checks:
1. i18n key coverage (labelKey in en.json + ko.json)
2. defaultChild validity (matches one of children paths)
3. Rust emit path compatibility (tray.rs + useTauriEventBridge.ts paths in routeTree)
4. Component import path existence (grep component imports, verify files exist)
5. No orphaned pageSidebarConfig references

Use `node -e` for JSON parsing (i18n checks), `grep` for pattern matching.

- [ ] **Step 2: Add to lefthook.yml**

```yaml
route-integrity:
  glob: "crates/oneshim-web/frontend/src/{routes,pages,components/shell}/**/*.{ts,tsx}"
  run: ./scripts/verify-route-integrity.sh
```

- [ ] **Step 3: Verify locally**

Run: `./scripts/verify-route-integrity.sh`
Expected: `[route-integrity] validation passed`

- [ ] **Step 4: Commit**

```
git add scripts/verify-route-integrity.sh lefthook.yml
git commit -m "chore: add route-integrity pre-commit hook"
```

---

## Task 7: Final Verification

- [ ] **Step 1: Full lint** `pnpm lint`
- [ ] **Step 2: TypeScript** `npx tsc --noEmit`
- [ ] **Step 3: E2E tests** `pnpm exec playwright test`
- [ ] **Step 4: Rust checks** `cargo check --workspace && cargo clippy --workspace --all-targets --quiet -- -D warnings`
- [ ] **Step 5: Route integrity** `./scripts/verify-route-integrity.sh`
- [ ] **Step 6: Web contract checks** `./scripts/verify-web-contract-boundary.sh && ./scripts/verify-http-interface-manifest.sh && ./scripts/verify-http-openapi-sync.sh`
- [ ] **Step 7: Commit if any cleanup needed**

---

## Execution Notes

- **Task 2 (Settings) validates the pattern** -- if SettingsFormContext works, all other pages are simpler
- **Task 3 is the largest** -- 11 pages, each committed separately
- **Task 4 is the switchover** -- after this, old routing is gone. Tasks 2-3 must be complete first
- **Task 6 (pre-commit) can run in parallel** with Tasks 2-3 since it only reads route-tree.ts
