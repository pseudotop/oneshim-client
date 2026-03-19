# Priority 2 Phase D: Recalibration Frontend — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver inline segment recalibration in the DashboardDay timeline and a dedicated RecalibrationPage for bulk date-range corrections, wired to the existing recalibration REST API and Tauri commands.

**Architecture:** React 18 components in `oneshim-web/frontend/`. TimelineView gets a context menu per segment block. New RecalibrationPage at `/recalibration` provides bulk override management. API calls via `useRecalibration` hook hitting existing REST endpoints (`/api/recalibration/*`). Tauri IPC available as alternative transport.

**Tech Stack:** React 18, TypeScript, Tailwind CSS, TanStack Query, lucide-react icons

**Spec:** `docs/superpowers/specs/2026-03-19-priority2-accuracy-improvements-design.md` section 3.9

**Parent plan:** `docs/superpowers/plans/2026-03-19-priority2-accuracy-improvements.md` (Phase D, Tasks 12-14)

**Depends on (must be merged first):**
- Phase C Task 9: Recalibration REST handlers (`crates/oneshim-web/src/handlers/recalibration.rs` — already exists)
- Phase C Task 10: Tauri commands (`create_override`, `delete_override`, `list_overrides`, `trigger_recluster`)

---

## File Map

### New files

| File | Responsibility |
|------|----------------|
| `crates/oneshim-web/frontend/src/hooks/useRecalibration.ts` | API hooks: create/delete/list overrides, trigger recluster |
| `crates/oneshim-web/frontend/src/pages/RecalibrationPage.tsx` | Bulk recalibration page |
| `crates/oneshim-web/frontend/src/components/SegmentContextMenu.tsx` | Context menu dropdown for segment override actions |

### Modified files

| File | Change |
|------|--------|
| `crates/oneshim-web/frontend/src/api/contracts.ts` | Add `RegimeOverride`, `UserOverrideAction`, `CreateOverrideRequest`, `ListOverridesResponse` types |
| `crates/oneshim-web/frontend/src/api/client.ts` | Add `createOverride`, `deleteOverride`, `listOverrides`, `triggerRecluster` API functions |
| `crates/oneshim-web/frontend/src/components/TimelineView.tsx` | Add gear icon, context menu trigger, override badge/strikethrough visual |
| `crates/oneshim-web/frontend/src/App.tsx` | Lazy-load RecalibrationPage, add `/recalibration` route |
| `crates/oneshim-web/frontend/src/components/shell/TreeView.tsx` | Add "Recalibration" nav entry under Analysis section |
| `crates/oneshim-web/frontend/src/i18n/locales/en.json` | Add recalibration i18n keys |
| `crates/oneshim-web/frontend/src/i18n/locales/ko.json` | Add recalibration i18n keys (Korean) |

---

## Task 1: API contracts and client functions

**Files:**
- Modify: `crates/oneshim-web/frontend/src/api/contracts.ts`
- Modify: `crates/oneshim-web/frontend/src/api/client.ts`

- [ ] Add TypeScript types to `contracts.ts`:
  ```typescript
  export type UserOverrideAction =
    | { type: 'MarkAsNoise' }
    | { type: 'ReassignRegime'; target_regime_id: string }
    | { type: 'MarkAsPersonalTime'; from: string; to: string }

  export interface RegimeOverride {
    override_id: string
    segment_id: string
    original_regime_id: string | null
    user_action: UserOverrideAction
    created_at: string
  }

  export interface CreateOverrideRequest {
    segment_id: string
    original_regime_id?: string
    action: UserOverrideAction
  }

  export interface ListOverridesQuery {
    from?: string
    to?: string
  }
  ```
- [ ] Add API functions to `client.ts`:
  - `createOverride(req: CreateOverrideRequest): Promise<RegimeOverride>` — POST `/api/recalibration/override`
  - `deleteOverride(id: string): Promise<void>` — DELETE `/api/recalibration/override/:id`
  - `listOverrides(query: ListOverridesQuery): Promise<RegimeOverride[]>` — GET `/api/recalibration/overrides`
  - `triggerRecluster(): Promise<{ ok: boolean; message: string }>` — POST `/api/recalibration/recluster`
- [ ] Commit: `feat(frontend): add recalibration API contracts and client functions`

## Task 2: useRecalibration hook

**Files:**
- Create: `crates/oneshim-web/frontend/src/hooks/useRecalibration.ts`

- [ ] Create `useRecalibration` hook using TanStack Query:
  - `useOverrides(from?: string, to?: string)` — `useQuery` wrapping `listOverrides`, key: `['overrides', from, to]`
  - `useCreateOverride()` — `useMutation` wrapping `createOverride`, invalidates `['overrides']` on success
  - `useDeleteOverride()` — `useMutation` wrapping `deleteOverride`, invalidates `['overrides']` on success
  - `useRecluster()` — `useMutation` wrapping `triggerRecluster`
- [ ] Each mutation shows success/error toast via `useToast`
- [ ] Export all hooks from the file
- [ ] Commit: `feat(frontend): add useRecalibration TanStack Query hooks`

## Task 3: Inline recalibration in TimelineView

**Files:**
- Create: `crates/oneshim-web/frontend/src/components/SegmentContextMenu.tsx`
- Modify: `crates/oneshim-web/frontend/src/components/TimelineView.tsx`

- [ ] Create `SegmentContextMenu` component:
  - Props: `segmentId: string`, `currentRegimeId: string`, `regimeOptions: Array<{id: string, label: string}>`, `onMarkAsNoise: (segmentId: string) => void`, `onReassignRegime: (segmentId: string, targetRegimeId: string) => void`, `onClose: () => void`
  - Renders a positioned dropdown with:
    - "Mark as personal time" action item
    - "Change regime to..." sub-menu with regime list (exclude current)
  - Closes on click-outside or Escape
  - Accessible: `role="menu"`, `aria-label`, keyboard navigation
- [ ] Modify `TimelineView`:
  - Add `overrides?: RegimeOverride[]` and `regimeOptions?: Array<{id: string, label: string}>` to `TimelineViewProps`
  - Add optional `onCreateOverride?: (req: CreateOverrideRequest) => void` callback prop
  - Add gear icon (lucide `Settings2`) to each segment block header, visible on hover; show loading indicator (Spinner) on gear icon during mutation
  - On gear click: open `SegmentContextMenu` positioned relative to the button
  - Manage open menu state: `menuSegmentId: string | null`
  - Reference i18n keys: `recalibration.markAsPersonalTime`, `recalibration.changeRegimeTo`, `recalibration.overridden`, `recalibration.personalTime`
  - Visual indicator for overridden segments: check if `segment_id` exists in `overrides` array, if so:
    - Show small "Overridden" badge (using `Badge` from `components/ui`)
    - Apply `line-through` to the original regime label
    - Show new regime label or "Personal time" in the badge
- [ ] Commit: `feat(frontend): add inline segment recalibration to TimelineView`

## Task 3.5: Wire overrides into DashboardDay

**Files:**
- Modify: `crates/oneshim-web/frontend/src/pages/DashboardDay.tsx`

- [ ] Import `useOverrides`, `useCreateOverride` from the recalibration hook
- [ ] Fetch overrides for the current date
- [ ] Pass `overrides` and `onCreateOverride` to `TimelineView`
- [ ] Pass `regimeOptions` (from dashboard data or hardcoded initial set)
- [ ] Commit: `feat(frontend): wire override data into DashboardDay for inline recalibration`

## Task 4: RecalibrationPage

**Files:**
- Create: `crates/oneshim-web/frontend/src/pages/RecalibrationPage.tsx`

- [ ] Create page layout with three sections:
  1. **Header**: Page title + "Trigger re-clustering" button (calls `useRecluster`, shows spinner while pending, disabled during mutation)
  2. **Controls**: `DateRangePicker` (reuse existing component) for `from`/`to` filtering + "Mark range as personal time" bulk action button
  3. **Segment list**: Fetch segments via daily digest endpoint for the date range, display in a table/list:
     - Columns: Time range, Duration, Current regime (color dot + label), Dominant app, Actions
     - Actions column: "Mark as personal" button, "Change regime" dropdown (Select from `components/ui`)
     - Each action calls `useCreateOverride` mutation
  4. **Override history**: Below the segment list, show existing overrides from `useOverrides(from, to)`:
     - Columns: Segment, Original regime, Action applied, Created at, Undo button
     - Undo button calls `useDeleteOverride` mutation
- [ ] Use `Card` wrapper for each section, `Skeleton` during loading, `EmptyState` when no data
- [ ] Show toast on recluster success/failure
- [ ] `export default RecalibrationPage` for lazy loading
- [ ] Commit: `feat(frontend): add RecalibrationPage for bulk regime correction`

## Task 5: Route registration and navigation

**Files:**
- Modify: `crates/oneshim-web/frontend/src/App.tsx`
- Modify: `crates/oneshim-web/frontend/src/components/shell/TreeView.tsx`
- Modify: `crates/oneshim-web/frontend/src/i18n/locales/en.json`
- Modify: `crates/oneshim-web/frontend/src/i18n/locales/ko.json`

- [ ] In `App.tsx`:
  - Add lazy import: `const RecalibrationPage = lazy(() => import('./pages/RecalibrationPage'))`
  - Add route: `<Route path="/recalibration" element={<RecalibrationPage />} />`
- [ ] In `TreeView.tsx`: add navigation entry for "/recalibration" (use `RefreshCw` lucide icon) in the Analysis section, after DashboardDay
- [ ] Add i18n keys to `en.json`:
  ```json
  "recalibration": {
    "title": "Recalibration",
    "markAsPersonalTime": "Mark as personal time",
    "changeRegimeTo": "Change regime to...",
    "triggerRecluster": "Trigger re-clustering",
    "reclustering": "Re-clustering...",
    "reclusterSuccess": "Re-clustering completed",
    "reclusterError": "Re-clustering failed",
    "overrideCreated": "Override created",
    "overrideDeleted": "Override removed",
    "markRangePersonal": "Mark range as personal time",
    "overrideHistory": "Override History",
    "undo": "Undo",
    "noOverrides": "No overrides in this range",
    "noSegments": "No segments found for this date range",
    "overridden": "Overridden",
    "personalTime": "Personal time"
  }
  ```
- [ ] Add matching Korean keys to `ko.json`
- [ ] Commit: `feat(frontend): register recalibration route and navigation`

## Task 6: Build verification

- [ ] `cd crates/oneshim-web/frontend && npm run build` (or `pnpm build`) — verify no TS errors
- [ ] `cd crates/oneshim-web/frontend && npm run lint` — verify no lint warnings
- [ ] `cargo check -p oneshim-web` — verify Rust side still compiles with embedded frontend
- [ ] Manual smoke test: open `/recalibration` in browser, verify page renders with empty state
- [ ] Manual smoke test: open `/dashboard/day`, verify gear icon appears on hover over timeline segments
- [ ] Commit: `chore: verify P2 Phase D frontend build`

---

## Acceptance Criteria

1. Gear icon visible on hover for each timeline segment in DashboardDay
2. Context menu offers "Mark as personal time" and "Change regime to..." with regime dropdown
3. Overridden segments show visual badge and strikethrough on original regime label
4. RecalibrationPage accessible at `/recalibration` via sidebar navigation
5. Date range picker filters segments; bulk "Mark range as personal time" works
6. "Trigger re-clustering" button fires POST and shows success/error feedback
7. Override history table with undo (delete) capability
8. All i18n keys present in en/ko
9. Frontend builds with zero TS/lint errors
