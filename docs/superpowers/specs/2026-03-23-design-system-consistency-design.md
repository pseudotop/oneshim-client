# Design System Consistency — Full Token Adoption (Color, Typography, Spacing, Motion, Icons)

**Date**: 2026-03-23
**Status**: Implemented
**Scope**: `crates/oneshim-web/frontend/src/`

## Problem

The frontend design system has structural foundations (CSS custom properties, tokens.ts, variants.ts) but ~70% of components bypass it. Key issues:

1. **7 accent colors by name** (`accent.teal`, `.blue`, `.purple`...) with no usage guidelines — components pick colors arbitrarily
2. **Typography tokens incomplete** — missing `label`, `caption`, `metadata`, `mono`; 70% of components hardcode Tailwind classes
3. **Variants leak raw colors** — Button danger=`bg-red-600`, Card highlight=hardcoded gradient
4. **Inconsistent dark mode** — some components use CSS vars (auto-switch), others hardcode `dark:` prefixes

These match the "AI slop" anti-patterns identified in reference materials:
- Rainbow accent colors instead of one brand color
- No system rationale for color choices
- Inconsistent typography weight/size across similar elements

## Design

### 1. Color Architecture: 3 Layers

**Before** (5 layers, 7 accent colors):
```
primary → surface → text → semantic → accent(7) → status
```

**After** (3 functional layers):
```
Layer 1: Brand (teal) — single accent for all non-semantic color
Layer 2: Semantic — success/warning/error/info (meaning-based only)
Layer 3: Chart — data visualization palette (hex, no dark-mode switching)
  + surface, text layers retained as-is
```

#### tokens.ts changes

**Remove entirely:**
- `colors.accent` object (all 7 entries)
- `statColorMap` object

**No changes:**
- `colors.primary` — already brand teal, correct
- `colors.surface` — neutral grays, correct
- `colors.text` — slate hierarchy, correct
- `colors.semantic` — success/warning/error/info, correct
- `colors.status` — connected/connecting/disconnected/error, correct
- `palette` / `chartPalette` — chart-only hex values, correct
- `dataViz.stroke` — good/warning/critical, correct

#### index.css changes

**Remove CSS vars** (both `:root` and `.dark`):
```
--accent-teal, --accent-blue, --accent-purple, --accent-green, --accent-emerald
--accent-amber, --accent-red, --accent-orange, --accent-slate
```

**Add** (for Button/Card semantic backgrounds):
```css
--semantic-error-hover: <red-700 light / red-400 dark>
--semantic-warning-hover: <amber-600 light / amber-400 dark>
```

#### tailwind.config.js changes

**Remove** `accent` from `extend.colors`.

**Add** semantic hover colors to `extend.colors.semantic`.

### 2. Typography Token Completion

**Current tokens** (tokens.ts):
```
h1(2xl bold), h2(xl semibold), h3(lg semibold), h4(base medium)
body(sm), small(xs), micro(11px), nano(10px)
stat.hero(3xl bold), stat.large(2xl bold), stat.normal(lg medium)
```

**Add:**
```ts
typography = {
  // existing headings — no change
  h1: 'text-2xl font-bold',
  h2: 'text-xl font-semibold',
  h3: 'text-lg font-semibold',
  h4: 'text-base font-medium',

  // body text — add label and caption
  body: 'text-sm',
  label: 'text-sm font-medium',         // NEW — form labels, list items, interactive text
  caption: 'text-xs',                        // NEW — secondary info, timestamps, metadata (apply color separately)
  small: 'text-xs',
  micro: 'text-[11px]',
  nano: 'text-[10px]',
  mono: 'font-mono text-sm',            // NEW — code, event IDs, technical values
  overline: 'text-[11px] font-semibold uppercase tracking-wider', // NEW — category labels above headings

  // stats — no change
  stat: {
    hero: 'text-3xl font-bold',
    large: 'text-2xl font-bold',
    normal: 'text-lg font-medium',
  },
}
```

**New tokens rationale:**
- `label` — replaces 50+ instances of `font-medium text-sm` across components
- `caption` — replaces ad-hoc `text-xs text-content-secondary` / `text-xs text-gray-500`
- `mono` — replaces 3+ instances of `font-mono text-sm` in EventLog, Timeline, ShortcutsHelp
- `overline` — replaces ad-hoc `text-[11px] font-semibold uppercase tracking-wider` used in sidePanel headers, section dividers (already in layout tokens but not in typography)

### 3. Spacing Token Redesign

**Current state:** 5 tokens defined, **0% adoption** (414 hardcoded spacing classes).

The current `spacing` tokens map to `p-*` only, which is too narrow — components need `gap`, `space-y`, `px`/`py`, and `m-*` variants. The tokens are also poorly named for their actual usage.

**Replace current spacing tokens with a semantic scale:**

```ts
export const spacing = {
  // Semantic spacing scale — use for all gap, padding, margin
  '0': '0',        // 0px
  '1': '0.25rem',  // 4px — tight: icon-text gap, inline elements
  '2': '0.5rem',   // 8px — compact: list items, badge padding
  '3': '0.75rem',  // 12px — default: form fields, card inner padding
  '4': '1rem',     // 16px — comfortable: section gaps, card padding
  '6': '1.5rem',   // 24px — spacious: page section spacing
  '8': '2rem',     // 32px — wide: major section dividers
  '12': '3rem',    // 48px — hero: page top/bottom padding
} as const
```

**Usage pattern — tokens provide the SCALE, Tailwind provides the DIRECTION:**

Components apply spacing tokens via the Tailwind scale values, NOT the token object directly. The token definition serves as the **approved scale** — components use only these step values:

```tsx
// ✅ Approved — using scale values from spacing tokens
className="gap-2 p-4"       // compact gap, comfortable padding
className="space-y-6 px-4"  // spacious vertical, comfortable horizontal

// ❌ Forbidden — off-scale values
className="gap-5 p-7"       // 5 and 7 are not in the scale
className="gap-[13px]"      // arbitrary pixel values
```

**Approved scale steps:** `0, 1, 2, 3, 4, 6, 8, 12`

**Forbidden off-scale values (to lint):** `p-5, p-7, p-9, p-10, p-11, gap-5, gap-7, m-5, m-7`, etc. — any spacing class using a step NOT in `{0,1,2,3,4,6,8,12}`.

**Exception:** `px-1.5`, `py-0.5`, `py-1.5` are allowed for sub-scale component internals (button padding, badge padding) and are already used in `variants.ts` size definitions.

### 4. Motion Token Standardization

**Current state:** 3 tokens defined, **5% adoption** (53 hardcoded `transition-*` classes).

The existing `motion` tokens define durations but don't specify WHAT to transition. Components hardcode `transition-colors`, `transition-all`, `transition-opacity` separately from duration.

**Redesign as composite transition tokens:**

```ts
export const motion = {
  // Composite tokens: property + duration + easing in one
  colors: 'transition-colors duration-150 ease-out',    // RENAMED from fast — hover states, toggles
  transform: 'transition-transform duration-300 ease-out', // NEW — scale, translate, rotate
  opacity: 'transition-opacity duration-200 ease-out',   // NEW — fade in/out
  all: 'transition-all duration-300 ease-out',           // NEW — layout shifts (use sparingly)
  none: 'transition-none',                               // NEW — explicitly disable

  // Duration-only tokens (for keyframe animations)
  duration: {
    fast: 'duration-150',
    normal: 'duration-300',
    slow: 'duration-500',
  },
} as const
```

**Usage:**
```tsx
// ✅ Token-based
className={motion.colors}        // hover color changes
className={motion.opacity}       // fade effects
className={motion.transform}     // scale/position changes

// ❌ Hardcoded
className="transition-colors duration-150 ease-out"  // use motion.colors
className="transition-all"                            // use motion.all (with caution)
```

**Keyframe animations** (`animate-spin`, `animate-pulse`, `animate-toast-in`) are allowed as-is — they are CSS-defined, not ad-hoc transitions.

**Principle:** "2-3 intentional motion types, remove if ornamental." The 4 transition tokens (`colors`, `transform`, `opacity`, `all`) are the approved set.

### 5. Icon Size Token Adoption

**Current state:** 6 tokens defined, **8% adoption** (~92 hardcoded icon sizes).

`iconSize` tokens exist but components use inline `w-X h-X` directly.

**No token changes needed** — the current scale is correct:
```ts
iconSize = { xs: 'w-3 h-3', sm: 'w-3.5 h-3.5', base: 'w-4 h-4', md: 'w-5 h-5', lg: 'w-6 h-6', hero: 'w-8 h-8' }
```

**Migration strategy:**

| Current Hardcoded | Token Replacement |
|-------------------|-------------------|
| `w-3 h-3` | `iconSize.xs` |
| `w-3.5 h-3.5` | `iconSize.sm` |
| `w-4 h-4` | `iconSize.base` |
| `w-5 h-5` | `iconSize.md` |
| `w-6 h-6` | `iconSize.lg` |
| `w-8 h-8` | `iconSize.hero` |

**Off-scale sizes** (`w-12`, `w-16`, etc.) are for illustrations/empty states, not icons — exempt from icon tokens but must use `w-X h-X` pairs consistently.

**Lint rule:** Add to GritQL + shell script — forbid `w-3 h-3`, `w-4 h-4`, `w-5 h-5` inline on Lucide icon elements; require `iconSize.*` token.

### 6. Component Variant Updates (variants.ts)

```ts
// Button: danger/warning → semantic CSS vars
buttonVariants.variant = {
  primary: 'bg-brand hover:bg-brand-hover text-content-inverse font-medium',     // no change
  secondary: 'bg-surface-muted hover:bg-active text-content font-medium',        // no change
  ghost: 'hover:bg-hover text-content-secondary',                                // no change
  danger: 'bg-semantic-error hover:bg-semantic-error-hover text-content-inverse font-medium',  // CHANGED
  warning: 'bg-semantic-warning hover:bg-semantic-warning-hover text-content-inverse font-medium', // CHANGED
}

// Card: highlight → brand-based, no gradient
cardVariants.variant = {
  default: 'bg-surface-elevated',                                      // no change
  elevated: 'bg-surface-muted',                                       // no change
  highlight: 'bg-brand-signal/10 border border-brand-signal/30',      // CHANGED (was gradient)
  interactive: 'bg-surface-elevated hover:bg-active cursor-pointer transition-colors', // no change
  danger: 'bg-surface-elevated border border-semantic-error/30',       // CHANGED (was border-red-500/30)
}

// Input: error → semantic
inputVariants.variant = {
  default: 'bg-surface-muted border-DEFAULT',   // no change
  error: 'bg-surface-muted border-semantic-error', // CHANGED (was border-red-500)
}

// Badge: purple → brand (accent removed)
badgeVariants.color = {
  default: 'bg-status-disconnected/20 text-content-secondary',  // no change
  success: 'bg-semantic-success/20 text-semantic-success',       // no change
  warning: 'bg-semantic-warning/20 text-semantic-warning',       // no change
  error: 'bg-semantic-error/20 text-semantic-error',             // no change
  info: 'bg-semantic-info/20 text-semantic-info',                // no change
  primary: 'bg-brand-signal/20 text-brand-text',                 // no change
  purple: 'bg-brand-signal/20 text-brand-text',                  // CHANGED (was accent-purple)
}

// Remove statColorMap entirely — replaced by brand single color
// All stat values use: 'bg-brand-signal/10 text-brand-text'
```

### 7. Component Migration Map

#### SuggestionBanner — Semantic Mapping
```
NeedFocusTime         → semantic.info    (blue — informational)
TakeBreak             → semantic.warning (amber — attention needed)
RestoreContext         → semantic.info    (blue — informational)
PatternDetected       → semantic.success (green — positive insight)
ExcessiveCommunication→ semantic.warning (amber — attention needed)
```

#### Component-by-Component Changes

| Component | Current Color Issue | Fix |
|-----------|-------------------|-----|
| **Dashboard metrics** | accent.teal/blue/purple per metric | `text-brand-text` for all values |
| **StatCard** | `statColorMap` 5 colors | `bg-brand-signal/10 text-brand-text` |
| **EventLog** | `teal-500`/`blue-100` hardcoded | `brand-signal` active, `semantic.info` badge |
| **ProcessList** | `teal-400`/`blue-400` | `text-brand-text` for all values |
| **ActivityHeatmap** | `green-100`→`green-500` 6-step | Brand opacity gradient (`brand-signal/10`→`brand-signal`) |
| **SuggestionBanner** | 5 colors per type | Semantic 4-color mapping (see above) |
| **FocusWidget** | `accent.blue`/`.purple`/`.amber` | `text-brand-text` for scores, semantic for alerts |
| **ErrorBoundary** | `text-red-600` | `text-semantic-error` |
| **UpdatePanel** | `text-amber-600` | `text-semantic-warning` |
| **Card highlight** | teal→blue gradient | `bg-brand-signal/10` |
| **OAuthPanel** | Various hardcoded colors | Brand + semantic tokens |
| **InsightCard** | `accent-green/blue` + `border-l-blue-500` | `brand-signal` + `border-brand-signal` |
| **StatisticsPanel** | `accent-green`/`accent-red` | `semantic.success`/`semantic.error` (trend up/down) |
| **TagInput** | `text-accent-teal` | `text-brand-text` |
| **GeneralTab** | `text-accent-teal` | `text-brand-text` |
| **SessionReplay** | `accent-emerald`/`accent-red` | `semantic.success`/`semantic.error` |
| **Automation** | `accent-emerald`/`accent-orange` | `semantic.success`/`semantic.warning` |
| **Focus** | `bg-accent-slate` | `bg-surface-muted` |

#### Typography Migration (High-Impact Files)

| File | Violations | Key Replacements |
|------|-----------|-----------------|
| **AiAutomationTab.tsx** | 100+ | `font-medium text-sm` → `typography.label`, `text-xs` → `typography.caption` |
| **Automation.tsx** | 80+ | Same + `text-[11px]` → `typography.micro`, `text-[10px]` → `typography.nano` |
| **SessionReplay.tsx** | 60+ | Same patterns |
| **Settings.tsx** | 50+ | `font-semibold text-sm` → `typography.label`, `text-xs` → `typography.caption` |
| **Privacy.tsx** | 50+ | Same patterns |
| **Reports.tsx** | 40+ | `font-bold text-4xl` → heading token, numeric → `typography.stat.*` |
| **Focus.tsx** | 40+ | Same patterns |
| **EventLog.tsx** | 30+ | `font-mono text-sm` → `typography.mono` |
| **FocusWidget.tsx** | 30+ | `font-bold text-lg` → `typography.h3` |
| All other components | 5-20 each | Systematic replacement |

### 8. Rules for Future Development

1. **One brand color** — Teal is the only accent. Additional colors require semantic justification (success/warning/error/info).
2. **No direct Tailwind color/typography classes in components** — Always use tokens.ts or variants.ts.
3. **Chart palette is the exception** — `palette.*` and `chartPalette` for data visualization SVG/canvas only.
4. **Typography hierarchy**: h1→h4 for headings, `label` for interactive text, `body` for paragraphs, `caption` for secondary info, `mono` for technical values, `overline` for category labels.
5. **Dark mode via CSS vars only** — No `dark:` prefixes in component code. All switching happens in index.css custom properties.
6. **Spacing scale only** — Use approved steps `{0,1,2,3,4,6,8,12}` for all `p-`, `m-`, `gap-`, `space-*` classes. No off-scale values (`p-5`, `gap-7`, etc.).
7. **Motion via tokens** — Use `motion.colors`/`.transform`/`.opacity`/`.all` for transitions. No bare `transition-colors` or `transition-all`.
8. **Icon sizes via tokens** — Use `iconSize.*` for all Lucide icon sizing. No inline `w-4 h-4` on icons.

### 9. Files Modified

**Core design system (4 files):**
- `src/styles/tokens.ts` — Remove accent, add typography/spacing/motion tokens
- `src/styles/variants.ts` — Semantic variants, remove statColorMap
- `src/index.css` — Remove accent CSS vars, add semantic hover vars
- `tailwind.config.js` — Remove accent from extend.colors

**Lint enforcement (4+ files):**
- `lint/no-hardcoded-colors.grit` — GritQL color lint
- `lint/no-hardcoded-typography.grit` — GritQL typography lint
- `lint/no-dark-prefix.grit` — GritQL dark mode lint
- `scripts/lint-design-tokens.sh` — CI gate shell script
- `biome.json` — Plugin registration

**Components (~50 files):**
- All files in `src/components/` and `src/pages/` with hardcoded colors, typography, spacing, motion, or icon sizes

### 10. Design System Lint Enforcement

Two-layer enforcement: **Biome GritQL plugin** (primary, IDE-integrated) + **shell script** (CI fallback).

#### 7.1 Biome GritQL Plugin (Primary)

Biome supports GritQL-based custom lint plugins that integrate with `biome check` and provide IDE real-time feedback.

**File:** `crates/oneshim-web/frontend/lint/no-hardcoded-colors.grit`

```gritql
language js;

// Forbid hardcoded Tailwind color classes in JSX className
`className=$value` where {
    $value <: or {
        contains r"text-(teal|blue|purple|green|amber|red|orange|pink|indigo|emerald|violet|slate)-\d",
        contains r"bg-(teal|blue|purple|green|amber|red|orange|pink|indigo|emerald|violet|slate)-\d",
        contains r"border-(teal|blue|purple|green|amber|red|orange|pink|indigo|emerald|violet)-\d",
        contains r"from-(teal|blue|purple|green|amber|red|orange)-\d",
        contains r"to-(teal|blue|purple|green|amber|red|orange)-\d"
    },
    register_diagnostic(
        span = $value,
        message = "Use design tokens (colors.primary, colors.semantic) instead of hardcoded Tailwind colors. See tokens.ts.",
        severity = "error"
    )
}
```

**File:** `crates/oneshim-web/frontend/lint/no-hardcoded-typography.grit`

```gritql
language js;

// Forbid hardcoded font weights in JSX className
`className=$value` where {
    $value <: or {
        contains "font-bold",
        contains "font-semibold",
        contains "font-medium",
        contains "font-mono"
    },
    register_diagnostic(
        span = $value,
        message = "Use typography tokens (typography.h1-h4, typography.label, typography.mono) instead of hardcoded font classes. See tokens.ts.",
        severity = "error"
    )
}
```

**File:** `crates/oneshim-web/frontend/lint/no-dark-prefix.grit`

```gritql
language js;

// Forbid dark: prefix in component className — CSS vars handle dark mode
`className=$value` where {
    $value <: or {
        contains r"dark:bg-",
        contains r"dark:text-",
        contains r"dark:border-"
    },
    register_diagnostic(
        span = $value,
        message = "Do not use dark: prefix. Dark mode is handled by CSS custom properties in index.css.",
        severity = "error"
    )
}
```

**biome.json addition:**
```json
{
  "plugins": [
    "./lint/no-hardcoded-colors.grit",
    "./lint/no-hardcoded-typography.grit",
    "./lint/no-dark-prefix.grit"
  ]
}
```

**File:** `crates/oneshim-web/frontend/lint/no-hardcoded-motion.grit`

```gritql
language js;

// Forbid bare transition classes — use motion.* tokens
`className=$value` where {
    $value <: or {
        contains "transition-colors",
        contains "transition-all",
        contains "transition-opacity",
        contains "transition-transform"
    },
    register_diagnostic(
        span = $value,
        message = "Use motion tokens (motion.colors, motion.all, motion.opacity, motion.transform) instead of bare transition classes. See tokens.ts.",
        severity = "error"
    )
}
```

**biome.json addition:**
```json
{
  "plugins": [
    "./lint/no-hardcoded-colors.grit",
    "./lint/no-hardcoded-typography.grit",
    "./lint/no-dark-prefix.grit",
    "./lint/no-hardcoded-motion.grit"
  ]
}
```

**Excluded files:** GritQL plugins only run on files included by `biome.json` `include`/`exclude` config. Exclude `src/styles/`, `*.test.tsx`, `*.stories.tsx` from these rules.

**Note on spacing/icon lint:** Off-scale spacing (`p-5`, `gap-7`) and inline icon sizes are checked by the shell script (Section 10.2) rather than GritQL, because the regex patterns for scale validation are more naturally expressed as grep patterns.

#### 7.2 Shell Script (CI Required Gate)

Location: `crates/oneshim-web/frontend/scripts/lint-design-tokens.sh`

**Note:** Biome GritQL plugins are diagnostic-only (no autofix) and still beta — some patterns may not match in edge cases. The shell script is the **authoritative CI gate**, not a fallback. Both layers run in CI; the GritQL plugin adds IDE real-time feedback.

Catches patterns in template literals and `cn()` calls that GritQL className matching may miss.

**Scan scope:** `src/components/` and `src/pages/`
**Excluded:** `src/styles/`, `tailwind.config.js`, `*.test.tsx`, `*.stories.tsx`

**Exit codes:** `0` = clean, `1` = violations found (prints file:line:match)

#### 7.3 npm Script Integration

```json
{
  "scripts": {
    "lint": "biome check src/ && bash scripts/lint-design-tokens.sh",
    "lint:tokens": "bash scripts/lint-design-tokens.sh"
  }
}
```

#### 7.4 Enforcement Notes

- `tokens.ts` and `variants.ts` are exempt — they are the source of truth
- `form.label` token (`'block text-sm font-medium text-content-strong mb-2'`) resolves to classes containing `font-medium`; this is fine because developers import the token, not the raw class. The lint checks className string literals in JSX, not resolved CSS.
- `typography.label` (`'text-sm font-medium'`) is a separate token from `form.label` — label is for inline text, form.label is for form field labels with block display and margin

### 11. Out of Scope

- Component directory restructuring (current ui/shell/features/pages is adequate)
- Storybook documentation (separate task)
- New component creation

### 12. Success Criteria

**Color:**
- Zero `accent-*` CSS vars or Tailwind classes in codebase
- Zero hardcoded color classes (`text-teal-*`, `bg-blue-*`, etc.) outside tokens/variants
- Dark mode works via CSS vars only — no `dark:` prefixes in component files

**Typography:**
- All typography uses token references (h1-h4, body, label, caption, small, micro, nano, mono, overline, stat.*)
- Zero hardcoded `font-bold`/`font-semibold`/`font-medium`/`font-mono` outside tokens/variants

**Spacing:**
- All spacing classes use approved scale steps `{0,1,2,3,4,6,8,12}` only
- Zero off-scale values (`p-5`, `gap-7`, `m-9`, `gap-[Xpx]`, etc.)

**Motion:**
- All transitions use `motion.*` tokens
- Zero bare `transition-colors`/`transition-all`/`transition-opacity` in components

**Icons:**
- All Lucide icon sizes use `iconSize.*` tokens
- Zero inline `w-4 h-4` / `w-5 h-5` on icon elements

**Enforcement:**
- `pnpm lint:tokens` passes with zero violations
- `pnpm lint` (biome + token lint) passes
- `cargo tauri dev` + visual inspection confirms no regressions
