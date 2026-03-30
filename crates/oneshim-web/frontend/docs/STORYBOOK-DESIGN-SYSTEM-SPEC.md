# Storybook & Design System Completion Spec

> **Status**: Draft v3 (2nd review pass)
> **Scope**: `crates/oneshim-web/frontend/`
> **Effort**: ~16h across 3 phases (primitives, stories, docs)

## 1. Current State Assessment

### What Exists (Strong Foundation)

| Asset | Status | Quality |
|-------|--------|---------|
| **Storybook v10.2.15** | Builds successfully | Good config (a11y, themes, docs addons) |
| **76 story files** | All components covered | Quality varies (see gaps) |
| **Design tokens** (`tokens.ts`) | Complete | Excellent — 12 categories, CI-enforced |
| **Variants** (`variants.ts`) | Complete | 5 component variant sets (button, card, input, badge, select) |
| **DESIGN.md** | Complete | Comprehensive principles + patterns |
| **TOKENS.md** | Complete | Full visual reference |
| **Token lint** (`lint-design-tokens.sh`) | CI-enforced | Strict — blocks hardcoded values |
| **CSS custom properties** | Light/dark theming | Production-grade (no `dark:` prefix) |
| **12 UI primitives** | `components/ui/` | Excellent — forwardRef, cn(), variants |
| **7 shell components** | `components/shell/` | Good — IDE-style layout |
| **23 feature components** | `components/` | Good |
| **10 overlay components** | `overlay/components/` | Good |
| **ToggleRow** | `setting-tabs/ToggleRow.tsx` | Shared toggle component (already reused) |

### Established Architecture Patterns

These patterns MUST be followed by all new primitives:

| Pattern | Rule | Evidence |
|---------|------|----------|
| **No context API** | Hooks + callback props, never React.createContext | Tabs, Toast, all components |
| **No portals** | Fixed positioning + z-index, never React.createPortal | Toast, Lightbox, CommandPalette |
| **Flat variant objects** | `variants.variant[key]` + `variants.size[key]`, no nesting | variants.ts |
| **Separate exports** | Each component exported individually, no `Component.Sub` dot notation | ui/index.ts |
| **Manual focus mgmt** | useRef + querySelectorAll + focus(), no focus-trap library | CommandPalette, Tabs |
| **Document-level listeners** | Click-outside + keyboard via `document.addEventListener` + cleanup | SegmentContextMenu, Lightbox |
| **No new dependencies** | Built with native browser APIs only | All existing components |

### Composition Review Ladder

Storybook review is no longer limited to isolated components. The review ladder is:

| Level | Storybook area | Purpose |
|-------|----------------|---------|
| **Atom / Base** | `UI Primitives/*` | Token correctness and single-control states |
| **Molecule** | `Domain Components/*` | Small feature groups such as cards, charts, and banners |
| **Organism** | `Shell/*`, `Settings/*` | Dense interactive groupings and section-level hierarchy |
| **Template** | `Templates/*` | Cross-component compositions used to catch theme/spacing regressions |
| **Page** | `Pages/*` | Route-level surfaces with real page title + layout context |

Required review gates:
- `LightReview` and `DarkReview` stories for route-level pages that rely on shared page typography
- `Templates/*` stories for shell chrome, dashboard workspace, and settings workbench
- Storybook preview defaults to `light` so inverse-text regressions surface early

### What's Missing (Gaps)

#### Gap 1: Missing UI Primitives

4 common UI patterns implemented ad-hoc that should be extracted into `components/ui/`:

| Primitive | Actual Ad-hoc Count | Current State | Priority |
|-----------|---------------------|---------------|----------|
| **Divider** | 8+ (3 approaches) | `<hr>`, `border-t`, `border-b`, `border-l` inconsistently | HIGH |
| **Alert/InfoBox** | 10+ | `rounded-lg border bg-surface-* p-3/p-4` repeated | HIGH |
| **Dialog** | 4 overlays | CommandPalette, Lightbox, ShortcutsHelp, Privacy ConfirmModal — all duplicate focus trap + backdrop + ESC + click-outside (~60-80 lines each) | HIGH |
| **Checkbox** | 9 instances | Native checkbox + `form.checkbox` token, no wrapper component | MEDIUM |

**Deferred (insufficient justification):**
- Dropdown/Menu: Only 2 real dropdowns (SegmentContextMenu, TagInput) with very different content structures — wait for 3rd use case
- Toggle/Switch: `ToggleRow` is already a shared component reused across setting tabs
- Tooltip: No current usage; would add complexity without demand
- RadioGroup: Only 3 uses, native `<input type="radio">` + `form.radio` token is adequate

#### Gap 2: Story Quality Variance

Story quality ranges from **excellent** (UI primitives) to **minimal** (pages):

| Category | Example | Quality | Issue |
|----------|---------|---------|-------|
| **UI primitives** | Button (11 stories) | Excellent | All variants, sizes, states |
| **Shell** | ActivityBar (3 stories) | Good | Multiple states |
| **Feature components** | StatCard (1 story) | Thin | Single default story |
| **Pages** | Dashboard (`Default: Story = {}`) | Minimal | No mock data, single empty story |
| **Settings tabs** | GeneralTab (1 story) | Minimal | No interaction coverage |

**Root cause**: Page stories depend on API data (TanStack Query) but have no mock data layer.

#### Gap 3: Missing Storybook Documentation

| Missing | Impact |
|---------|--------|
| **No autodocs** | No auto-generated API docs from component props |
| **No MDX docs pages** | No in-Storybook design guides (only external .md files) |
| **No Getting Started page** | New contributors can't onboard via Storybook |
| **No component composition examples** | No patterns showing how to combine primitives |

---

## 2. Goals & Non-Goals

### Goals

1. **Extract 4 missing UI primitives** (Divider, Alert, Dialog, Checkbox) following established patterns
2. **Upgrade story quality** for thin stories to minimum bar: default + variants + key states
3. **Add autodocs** to all component stories for auto-generated API documentation
4. **Add 2 MDX documentation pages** to Storybook (Getting Started, Component Patterns)
5. **Add mock data helpers** for page stories using `queryClient.setQueryData()` pre-population
6. **Verify all stories render** in both light and dark themes via `build-storybook`

### Non-Goals

- Mobile responsive design (desktop-first app)
- Visual regression testing infrastructure (Chromatic/Percy)
- Interaction tests with `play` functions (stories are visual catalog, not test suite)
- `@storybook/addon-interactions` or `@storybook/test` packages (not installed, not needed)
- MSW (Mock Service Worker) — too heavy; use QueryClient pre-population instead
- Storybook deployment/hosting
- Refactoring existing components to use new primitives (follow-up PR)
- Dropdown/Menu primitive (only 2 uses, defer to follow-up when 3rd use case appears)
- New feature development
- New external dependencies

---

## 3. Deliverables

### Phase A: UI Primitive Extraction (~5h)

Extract 4 new primitives into `src/components/ui/`, following the established pattern:
- `forwardRef` + props extending native HTML attributes
- Variants in `variants.ts` (flat object, no nesting)
- `cn()` for class composition
- Co-located `.stories.tsx` with all variants
- Export from `components/ui/index.ts`
- **No context API, no portals, no new dependencies**

#### A1. Divider

```
src/components/ui/Divider.tsx + Divider.stories.tsx
```

- Props: `orientation?` (horizontal/vertical, default: horizontal), `className?`
- No variants needed — single semantic element
- Replaces: 8+ inconsistent `<hr>`, `border-t div`, `border-l` patterns
- Styling: `border-DEFAULT` token, `border-t` (horizontal) or `border-l h-full` (vertical)
- Accessibility: `role="separator"`, `aria-orientation`

#### A2. Alert

```
src/components/ui/Alert.tsx + Alert.stories.tsx
```

- Props: `variant` (info/success/warning/error/default), `title?`, `icon?`, `children`, `className?`
- Add `alertVariants` to `variants.ts` with 5 color variants using existing semantic tokens
- Replaces: 10+ ad-hoc info/warning box patterns (`rounded-lg border bg-surface-* p-3/p-4`)
- Accessibility: `role="alert"` for error/warning, `role="status"` for info/success/default

#### A3. Dialog

```
src/components/ui/Dialog.tsx + Dialog.stories.tsx
```

- Exported as separate components: `Dialog`, `DialogOverlay`, `DialogContent`, `DialogTitle`
- Props: `open`, `onClose`, `children`, `className?`
- Implementation: Extract shared pattern from 4 existing components:
  - CommandPalette (~70 lines of overlay/focus logic)
  - Lightbox (~50 lines of overlay/keyboard logic)
  - ShortcutsHelp (~60 lines of overlay/focus trap logic)
  - Privacy ConfirmModal (~60 lines of overlay/focus trap logic)
- Shared logic to extract:
  - Fixed positioning + z-index (no portals)
  - Manual focus trap (querySelectorAll focusable elements, Tab wrapping)
  - ESC key close (document-level keydown listener)
  - Overlay click close (backdrop onClick + child stopPropagation)
  - Body overflow hidden while open
  - Previous focus save/restore on open/close
- Add `dialogVariants` to `variants.ts` with size variants (sm/md/lg → max-width)
- Accessibility: `role="dialog"`, `aria-modal="true"`, `aria-labelledby`
- **Note**: Existing components will NOT be refactored to use Dialog in this PR

#### A4. Checkbox

```
src/components/ui/Checkbox.tsx + Checkbox.stories.tsx
```

- Props: `checked`, `onChange`, `disabled?`, `label?`, `description?`, `className?`
- Wraps native `<input type="checkbox">` with proper label association
- Uses existing `form.checkbox` + `form.checkboxInline` tokens
- Replaces: 9 inline checkbox patterns with consistent component
- Note: Coexists with `ToggleRow` — Checkbox is the base primitive, ToggleRow is the settings-specific composite
- Accessibility: native checkbox semantics, associated `<label>`

### Phase B: Story Quality Upgrade (~7h)

#### B1. Add autodocs to all stories

Add `tags: ['autodocs']` to all 76+ story meta objects. This generates automatic API documentation from component props and JSDoc comments.

```tsx
const meta = {
  title: 'UI Primitives/Button',
  component: Button,
  tags: ['autodocs'],  // ← add this
  // ...
} satisfies Meta<typeof Button>
```

#### B2. Enhance thin stories

Upgrade components with only 1 story to minimum bar:

**Minimum story bar per component type:**

| Type | Required Stories |
|------|----------------|
| **UI primitive** | Default + AllVariants + AllSizes + Disabled |
| **Shell component** | Default + key layout states (collapsed, active) |
| **Feature component** | Default + WithData + Empty |
| **Page** | Default (with mock data) + Empty state |
| **Settings tab** | Default (with mock data) |
| **Overlay component** | Default + Active state |

**Priority targets** (currently have only `Default: Story = {}`):

Pages (need mock data wrapper + meaningful states):
- `Dashboard` → WithMockData, EmptyState
- `DashboardDay` → WithMockData, EmptyState
- `Timeline` → WithMockData, EmptyTimeline
- `Reports` → WithMockData, NoReports
- `Chat` → WithMessages, EmptyChat
- `Focus` → ActiveSession, Idle
- `Coaching` → WithGoals, NoGoals
- `Settings` → WithMockData
- `SessionReplay` → WithData, Empty

Feature components (need variant/state coverage):
- `StatCard` → AllVariants, WithTrend
- `InsightCard` → AllVariants
- `EventLog` → WithEvents, Empty
- `ProcessList` → WithProcesses, Empty
- `GuiInteractionTrack` → WithData, Empty

Setting tabs (need mock config data):
- All 10 tabs → WithDefaults (inline mock config object)

#### B3. Add mock data helpers

Create `src/stories/mock-data.ts` with factory functions for story-specific mock data:

```ts
export function createMockMetrics(overrides?: Partial<MetricsResponse>): MetricsResponse
export function createMockSuggestion(overrides?: Partial<Suggestion>): Suggestion
export function createMockSession(overrides?: Partial<WorkSession>): WorkSession
export function createMockEvent(overrides?: Partial<ContextEvent>): ContextEvent
// etc.
```

Location: `src/stories/mock-data.ts` (co-located with stories, not in test utils).

**Page story pattern** — use QueryClient cache pre-population:
```tsx
const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, staleTime: Infinity } },
})

export const WithMockData: Story = {
  decorators: [
    (Story) => {
      queryClient.setQueryData(['summary'], createMockMetrics())
      queryClient.setQueryData(['processes'], createMockProcesses())
      return (
        <QueryClientProvider client={queryClient}>
          <MemoryRouter><Story /></MemoryRouter>
        </QueryClientProvider>
      )
    },
  ],
}
```

#### B4. Theme verification

Ensure `pnpm build-storybook` succeeds — this validates all stories compile and render. Manual spot-check 5 stories in both themes after build.

### Phase C: Storybook Documentation (~4h)

#### C1. Getting Started MDX page

```
src/stories/GettingStarted.mdx
```

Contents:
- Project overview (what this design system covers)
- How to run Storybook (`pnpm storybook`)
- Component naming conventions (UI Primitives / Shell / Domain Components / Pages / Overlay)
- How to add a new component (step-by-step checklist)
- Link to DESIGN.md and TOKENS.md

#### C2. Component Patterns MDX page

```
src/stories/ComponentPatterns.mdx
```

Contents:
- Primitive pattern (forwardRef, cn(), variants.ts, displayName)
- Class merging strategy (cn() = clsx + tailwind-merge)
- Dark mode approach (CSS vars, no `dark:` prefix)
- Architecture rules (no context API, no portals, no new dependencies)
- Token usage examples (colors, typography, spacing, motion)
- Composition example (combining Card + Badge + Button)

#### C3. Enhance DesignTokens story

Enhance existing `DesignTokens.stories.tsx`:
- Add side-by-side light/dark comparison for color sections (render same swatch in both themes)
- Add interactive spacing scale with actual pixel measurements
- Add icon catalog showing all lucide icons used in the project with their token sizes

---

## 4. Validation Criteria

### Phase A (Primitives) passes when:

- [ ] 4 primitives exist in `components/ui/` (Divider, Alert, Dialog, Checkbox)
- [ ] Each follows forwardRef + cn() pattern (except Dialog which uses separate named exports)
- [ ] Each has co-located `.stories.tsx` with all variants
- [ ] Each is exported from `components/ui/index.ts`
- [ ] Variants added to `variants.ts` where applicable (Alert, Dialog)
- [ ] No context API, no portals, no new dependencies
- [ ] `pnpm build` passes
- [ ] `pnpm build-storybook` passes
- [ ] `pnpm lint` passes (Biome)
- [ ] `pnpm test` passes (existing tests unbroken)

### Phase B (Stories) passes when:

- [ ] All stories have `tags: ['autodocs']`
- [ ] All priority-target components have 2+ stories
- [ ] Mock data helpers created in `src/stories/mock-data.ts`
- [ ] `pnpm build-storybook` succeeds with no errors

### Phase C (Docs) passes when:

- [ ] GettingStarted.mdx visible in Storybook sidebar
- [ ] ComponentPatterns.mdx visible in Storybook sidebar
- [ ] DesignTokens story enhanced with light/dark comparison
- [ ] `pnpm build-storybook` succeeds

### Overall completion:

- [ ] `pnpm build-storybook` clean (no errors)
- [ ] Storybook sidebar organized: Docs > Design System > UI Primitives > Shell > Domain Components > Pages > Overlay
- [ ] No component in `components/` lacks a co-located story
- [ ] All new code uses design tokens (lint passes)
- [ ] No new npm dependencies added

---

## 5. Migration Notes

### Replacing ad-hoc patterns

When extracting primitives, do NOT refactor existing components to use them in this PR. The primitives are created and documented first. Follow-up PRs can:
1. Migrate existing ad-hoc divider/alert/checkbox patterns to use new primitives
2. Refactor CommandPalette/Lightbox/ShortcutsHelp/ConfirmModal to use Dialog
3. Extract Dropdown primitive when 3rd use case appears

**Rationale**: Keeps the diff reviewable and avoids mixing new code with refactoring across 20+ files.

### Backward compatibility

All new primitives are additive. No existing component APIs change. No existing stories are removed.

### File organization

New files follow existing conventions:
- Primitives: `src/components/ui/{Name}.tsx` + `{Name}.stories.tsx`
- MDX docs: `src/stories/{Name}.mdx`
- Mock data: `src/stories/mock-data.ts`
- Variants: append to `src/styles/variants.ts`

### Relationship between Checkbox and ToggleRow

- `Checkbox` (new): Base primitive in `components/ui/`, wraps native checkbox with token styling
- `ToggleRow` (existing): Settings-specific composite in `setting-tabs/`, uses label + description + checkbox pattern
- ToggleRow MAY be refactored to use Checkbox internally in a follow-up PR

---

## 6. Estimated Effort Breakdown

| Phase | Task | Hours |
|-------|------|-------|
| **A** | Divider + Alert + Checkbox | 2h |
| **A** | Dialog (focus trap, keyboard, overlay) | 3h |
| **B** | Add autodocs to all stories | 1h |
| **B** | Mock data helpers + page story decorators | 1.5h |
| **B** | Enhance thin stories (20+ components) | 4.5h |
| **C** | GettingStarted.mdx + ComponentPatterns.mdx | 2h |
| **C** | DesignTokens story enhancement | 2h |
| | **Total** | **~16h** |

---

## 7. Risk Assessment

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Dialog focus trap edge cases | Medium | Follow CommandPalette's proven pattern exactly (4 existing implementations as reference) |
| Mock data maintenance burden | Low | Factory pattern with defaults, only override what matters |
| Storybook build size increase | Low | Already 1.3MB, 4 new primitives add minimal weight |
| Breaking existing lint rules | Low | All new code uses tokens, lint runs in CI |
| Page stories still render empty | Medium | Use QueryClient.setQueryData() in story decorators |
| QueryClient key mismatch | Medium | Read each page component to verify exact query keys before mocking |
