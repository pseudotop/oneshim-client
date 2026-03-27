# Storybook & Design System Completeness

**Date**: 2026-03-27
**Status**: Reviewed (revision 1)
**Scope**: `crates/oneshim-web/frontend/`

## Problem

The design system is well-architected (tokens, variants, lint governance) but **underdocumented and underrepresented in Storybook**:

1. **No design documentation**: No DESIGN.md or TOKENS.md — token rationale is tribal knowledge
2. **Story gaps**: 6/9 primitives have stories, but Input/Select/Tabs/Toast are minimal (1-2 stories each). Skeleton has 0. Zero domain component stories.
3. **No addon-docs**: Storybook generates no API documentation from TypeScript props
4. **Z-Index not tokenized**: Hardcoded 40/50/60 values — should be named Tailwind scale

## Design — Phase 1 (This PR)

Focus on **foundation + documentation** that enables all future story writing.

### 1. DESIGN.md — Design System Guide

Create `crates/oneshim-web/frontend/DESIGN.md`:
- Principles: semantic tokens, CSS vars, no `dark:` prefix, strict lint
- Token overview: color, typography, spacing, motion, elevation
- Component patterns: forwardRef, cn() composition, variant files
- Contribution rules: "always use tokens, never hardcode"

### 2. TOKENS.md — Token Reference

Create `crates/oneshim-web/frontend/TOKENS.md`:
- Visual reference for all token categories
- Color palette table (light + dark values side by side)
- Typography scale table
- Spacing scale
- Motion/elevation tokens
- Icon size scale

### 3. Z-Index Token Scale

Extend `tailwind.config.js` with named z-index scale:
```js
zIndex: {
  dropdown: '40',
  dialog: '50',
  overlay: '50',
  tooltip: '60',
  'detection': '10000',
  'detection-inspector': '10002',
  'detection-header': '10003',
}
```

Update `src/styles/tokens.ts` elevation section to reference these names.

### 4. Expand Primitive Stories

For each underfilled primitive:
- **Input**: Add AllVariants, ErrorState, WithLabel, Disabled stories
- **Select**: Add AllSizes, WithLabel, Disabled stories
- **Tabs**: Add ManyTabs, DisabledTab, VerticalLayout stories
- **Toast**: Add AllTypes, WithAction, AutoDismiss stories
- **Skeleton**: Add all 4 variants

Target: Every UI primitive has 3+ stories showing all variants + states.

### 5. Add 5 Key Domain Component Stories

Priority domain components (highest usage, most complex):
1. **StatCard** — used on Dashboard (4 instances)
2. **MetricsChart** — main dashboard chart
3. **DateRangePicker** — used on 5+ pages
4. **ProcessList** — dashboard process list
5. **ActivityHeatmap** — dashboard heatmap

Each gets 1-2 stories showing typical usage + empty state.

### Out of Scope (Phase 2)
- addon-docs installation (requires Storybook addon compatibility check)
- addon-interactions for interaction testing
- Full domain component story coverage (23 remaining)
- Color contrast ratio documentation
- Accessibility audit per component

### Files Changed

| File | Change |
|------|--------|
| `DESIGN.md` | NEW — design system guide |
| `TOKENS.md` | NEW — token reference |
| `tailwind.config.js` | MODIFY — z-index scale |
| `src/styles/tokens.ts` | MODIFY — elevation z-index references |
| `src/components/ui/Input.stories.tsx` | MODIFY — expand stories |
| `src/components/ui/Select.stories.tsx` | MODIFY — expand stories |
| `src/components/ui/Tabs.stories.tsx` | MODIFY — expand stories |
| `src/components/ui/Toast.stories.tsx` | MODIFY — expand stories |
| `src/components/ui/Skeleton.stories.tsx` | NEW — skeleton stories |
| `src/components/StatCard.stories.tsx` | NEW — domain story |
| `src/components/MetricsChart.stories.tsx` | NEW — domain story |
| `src/components/DateRangePicker.stories.tsx` | NEW — domain story |
| `src/components/ProcessList.stories.tsx` | NEW — domain story |
| `src/components/ActivityHeatmap.stories.tsx` | NEW — domain story |
