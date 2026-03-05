# Design System Hardening Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 디자인 시스템을 "가이드" 수준에서 "코드 제약" 수준으로 격상. 빠진 토큰 채우기 + 전체 적용 + Biome로 위반 차단.

**Architecture:** tokens.ts에 motion/elevation/iconSize 토큰 추가 → UI 프리미티브 수정 → 전체 페이지/컴포넌트에 토큰 참조 적용 → Biome + GritQL plugin으로 hardcoded class 차단.

**Tech Stack:** React 18, Tailwind CSS 3, Biome v2 (GritQL plugins), Vitest, Storybook 10

---

### Task 1: Token Expansion — motion, elevation, iconSize, typography micro/nano

**Files:**
- Modify: `src/styles/tokens.ts`

**Step 1: Add motion tokens after `interaction` (line 114)**

```ts
export const motion = {
  fast: 'duration-150 ease-out',
  normal: 'duration-300 ease-out',
  slow: 'duration-500 ease-out',
} as const
```

**Step 2: Add elevation tokens after motion**

```ts
export const elevation = {
  dropdown: 'shadow-lg z-40',
  dialog: 'shadow-xl z-50',
  overlay: 'shadow-2xl z-50',
  tooltip: 'shadow-md z-[60]',
} as const
```

**Step 3: Add iconSize tokens after elevation**

```ts
export const iconSize = {
  xs: 'w-3 h-3',
  sm: 'w-3.5 h-3.5',
  base: 'w-4 h-4',
  md: 'w-5 h-5',
  lg: 'w-6 h-6',
  hero: 'w-8 h-8',
} as const
```

**Step 4: Add micro/nano to typography (after `small: 'text-xs'`, line 94)**

```ts
micro: 'text-[11px]',
nano: 'text-[10px]',
```

**Step 5: Update layout tokens to reference new tokens**

Replace in `layout.activityBar`:
- `iconSize: 'w-5 h-5'` → `iconSize: iconSize.md`
- `tooltip: '...shadow-lg'` → use `elevation.tooltip` reference concept (keep compound string but note)

Replace in `layout.commandPalette`:
- `shadow: 'shadow-2xl'` → `shadow: elevation.overlay` — but elevation includes z-index too, so keep separate: just document the mapping.

Actually, keep layout tokens as-is to avoid breaking changes. The new tokens are for page/component-level usage.

**Step 6: Verify build**

Run: `pnpm build`
Expected: Success (tokens are just exports, no consumers yet)

**Step 7: Commit**

```bash
git add src/styles/tokens.ts
git commit -m "feat(tokens): add motion, elevation, iconSize tokens + typography micro/nano"
```

---

### Task 2: Fix UI Primitives — Button, Select, Input, EmptyState

**Files:**
- Modify: `src/components/ui/Button.tsx` (line 21-28)
- Modify: `src/components/ui/Select.tsx` (line 19-26)
- Modify: `src/components/ui/Input.tsx` (line 9-13)
- Modify: `src/components/ui/EmptyState.tsx` (line 26-33)

**Step 1: Button — add focusRing**

In `Button.tsx`, add `interaction.focusRing` to cn() call (after line 25 `interaction.disabled`):

```tsx
className={cn(
  'inline-flex items-center justify-center',
  radius.md,
  interaction.interactive,
  interaction.focusRing,    // ADD THIS
  interaction.disabled,
  buttonVariants.variant[variant],
  buttonVariants.size[size],
  className
)}
```

**Step 2: Select — add interaction.interactive**

In `Select.tsx` line 19-26, add `interaction.interactive` (it has focusRing but missing transition):

```tsx
className={cn(
  'w-full border',
  radius.md,
  colors.text.primary,
  interaction.interactive,   // ADD THIS
  interaction.focusRing,
  selectVariants.variant[variant],
  selectVariants.size[selectSize],
  className
)}
```

**Step 3: Input — remove dead variant prop**

In `Input.tsx`, remove `variant` from the interface (line 10). Keep only `inputSize` and `error`:

```tsx
export interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  inputSize?: keyof typeof inputVariants.size
  error?: boolean
}
```

Remove `variant` from destructuring (line 16):

```tsx
({ className, inputSize = 'md', error, ...props }, ref) => {
```

**Step 4: EmptyState — use Button component**

Replace raw `<button>` (lines 26-33) with `<Button>`:

```tsx
import { Button } from './Button'

// ... in the component:
{action && (
  <Button
    variant="primary"
    size="md"
    onClick={action.onClick}
  >
    {action.label}
  </Button>
)}
```

**Step 5: Run tests**

Run: `pnpm test`
Expected: 61/61 pass

**Step 6: Run build**

Run: `pnpm build`
Expected: Success

**Step 7: Commit**

```bash
git add src/components/ui/Button.tsx src/components/ui/Select.tsx src/components/ui/Input.tsx src/components/ui/EmptyState.tsx
git commit -m "fix(ui): add focusRing to Button, interactive to Select, fix Input dead prop, EmptyState uses Button"
```

---

### Task 3: Apply typography.h1 to all pages

**Files:**
- Modify: `src/pages/Reports.tsx:93`
- Modify: `src/pages/Focus.tsx:179`
- Modify: `src/pages/Search.tsx:101`
- Modify: `src/pages/Timeline.tsx:165`
- Modify: `src/pages/Privacy.tsx:203`
- Modify: `src/pages/Updates.tsx:11`
- Modify: `src/pages/Automation.tsx:185-187`
- Modify: `src/pages/SessionReplay.tsx:348-350`

**Step 1: Add imports to each page that lacks them**

Each page needs:
```tsx
import { typography, colors } from '../styles/tokens'
import { cn } from '../utils/cn'
```
(Some already have these imports — check first, only add missing ones)

**Step 2: Replace hardcoded h1 classes**

Pattern: `className="text-2xl font-bold text-content"` → `className={cn(typography.h1, colors.text.primary)}`

For pages with extra classes (Focus, Automation, SessionReplay):
- Focus.tsx:179 `"text-2xl font-bold text-content flex items-center gap-2"` → `{cn(typography.h1, colors.text.primary, 'flex items-center gap-2')}`
- Automation.tsx:185-187 — same pattern
- SessionReplay.tsx:348-350 — same pattern

8 pages total. Each is a simple string replacement.

**Step 3: Run build**

Run: `pnpm build`
Expected: Success

**Step 4: Run tests**

Run: `pnpm test`
Expected: 61/61 pass

**Step 5: Commit**

```bash
git add src/pages/Reports.tsx src/pages/Focus.tsx src/pages/Search.tsx src/pages/Timeline.tsx src/pages/Privacy.tsx src/pages/Updates.tsx src/pages/Automation.tsx src/pages/SessionReplay.tsx
git commit -m "refactor(pages): use typography.h1 token for all page headings"
```

---

### Task 4: Apply motion tokens

**Files:**
- Modify: `src/components/FocusWidget.tsx:75`
- Modify: `src/pages/Focus.tsx:85`

**Step 1: FocusWidget.tsx — import and apply**

Add import:
```tsx
import { motion } from '../styles/tokens'
```

Line 75: Replace `className="transition-all duration-500"` with:
```tsx
className={`transition-all ${motion.slow}`}
```

**Step 2: Focus.tsx — import and apply**

Add to existing tokens import:
```tsx
import { ..., motion } from '../styles/tokens'
```

Line 85: Replace `className="transition-all duration-700"` with:
```tsx
className={`transition-all ${motion.slow}`}
```

Note: Both were SVG circle gauge animations. `motion.slow` (500ms) is the right fit — the 700ms was unnecessarily long.

**Step 3: Run build + tests**

Run: `pnpm build && pnpm test`
Expected: Build success, 61/61 pass

**Step 4: Commit**

```bash
git add src/components/FocusWidget.tsx src/pages/Focus.tsx
git commit -m "refactor: use motion.slow token for gauge animations"
```

---

### Task 5: Apply elevation tokens

**Files:**
- Modify: `src/components/shell/ShortcutsHelp.tsx:68`
- Modify: `src/pages/Privacy.tsx:39-40`
- Modify: `src/components/TagInput.tsx:143`
- Modify: `src/components/LanguageSelector.tsx:57`

**Step 1: Add elevation import to each file**

```tsx
import { elevation } from '../styles/tokens'  // or '../../styles/tokens' for ui/
```

**Step 2: ShortcutsHelp.tsx:68**

Replace `shadow-xl` in the className string. Current:
```tsx
className={cn(layout.commandPalette.bg, layout.commandPalette.border, 'rounded-lg shadow-xl max-w-md w-full mx-4')}
```
Replace with:
```tsx
className={cn(layout.commandPalette.bg, layout.commandPalette.border, elevation.dialog, 'rounded-lg max-w-md w-full mx-4')}
```
Note: `elevation.dialog` = `shadow-xl z-50`. The outer div already has `z-50`, so this is safe (duplicate z-50 is harmless in Tailwind).

**Step 3: Privacy.tsx:39-40**

Current: `<div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">`
and `<Card ... className="max-w-md w-full mx-4 shadow-xl">`

Replace Card className: `className={cn('max-w-md w-full mx-4', elevation.dialog)}`

**Step 4: TagInput.tsx:143**

Replace `shadow-lg` + `z-50` with elevation.dropdown:
```tsx
className={cn(
  'absolute mt-1 w-full rounded-lg',
  elevation.dropdown,
  'bg-surface-overlay border border-muted',
  'max-h-60 overflow-auto'
)}
```
Note: `elevation.dropdown` = `shadow-lg z-40`. This changes z-50→z-40 for dropdown, which is semantically correct (dropdowns below modals).

**Step 5: LanguageSelector.tsx:57**

Current: `className="absolute right-0 mt-1 py-1 w-36 bg-surface-overlay rounded-lg shadow-lg border border-muted z-50"`

Replace with:
```tsx
className={cn('absolute right-0 mt-1 py-1 w-36 bg-surface-overlay rounded-lg border border-muted', elevation.dropdown)}
```
Same z-50→z-40 semantic correction.

**Step 6: Run build + tests**

Run: `pnpm build && pnpm test`

**Step 7: Commit**

```bash
git add src/components/shell/ShortcutsHelp.tsx src/pages/Privacy.tsx src/components/TagInput.tsx src/components/LanguageSelector.tsx
git commit -m "refactor: use elevation tokens for shadow/z-index consistency"
```

---

### Task 6: Apply iconSize tokens + fix focusRing gaps

**Files:**
- Modify: `src/pages/Focus.tsx:180`
- Modify: `src/components/SuggestionBanner.tsx:140`
- Modify: `src/components/Lightbox.tsx:66,81,105`
- Modify: `src/components/DateRangePicker.tsx:118,125`
- Modify: `src/pages/Timeline.tsx:264-275,308-319`
- Modify: `src/pages/Automation.tsx:295-302,337-339`

**Step 1: Icon size fixes**

Focus.tsx:180 — `w-7 h-7` → `iconSize.lg` (`w-6 h-6`):
```tsx
import { iconSize } from '../styles/tokens'
// ...
<FocusIcon className={iconSize.lg} />
```

SuggestionBanner.tsx:140 — already `w-6 h-6`, just tokenize:
```tsx
import { iconSize } from '../styles/tokens'
// ...
<Icon className={iconSize.lg} />
```

Lightbox.tsx:66,81,105 — `w-8 h-8` → `iconSize.hero` for close/nav in fullscreen overlay (hero size is appropriate for overlay controls):
```tsx
import { iconSize } from '../styles/tokens'
// lines 66, 81, 105:
<svg className={iconSize.hero} ...>
```

**Step 2: DateRangePicker focusRing fix**

Lines 118, 125: Replace `focus:outline-none focus:border-teal-500` with `interaction.focusRing`:
```tsx
import { interaction } from '../styles/tokens'
// Both inputs:
className={cn('bg-surface-overlay border border-DEFAULT rounded-lg px-3 py-1.5 text-sm text-content', interaction.focusRing)}
```

**Step 3: Timeline.tsx focusRing**

Line 271-275 (grid button) — add `interaction.focusRing` to cn() call:
```tsx
className={cn(
  'aspect-video bg-hover rounded overflow-hidden border-2 transition-all hover:scale-105',
  interaction.focusRing,
  selectedFrame?.id === frame.id
    ? 'border-brand-signal ring-2 ring-brand-signal/50'
    : 'border-transparent hover:border-strong'
)}
```

Line 315-319 (list button) — same pattern, add `interaction.focusRing`.

**Step 4: Automation.tsx focusRing**

Line 298 (tab button) — add focusRing:
```tsx
className={cn(
  'px-3 py-1.5 rounded-md text-sm font-medium transition-colors',
  interaction.focusRing,
  presetTab === tab
    ? 'bg-accent-teal/10 text-accent-teal'
    : 'text-content-secondary hover:bg-hover'
)}
```

Line 339 (expand button) — add focusRing:
```tsx
className={cn('mt-2 flex items-center text-xs text-content-muted hover:text-content-strong transition-colors', interaction.focusRing)}
```

**Step 5: Run build + tests**

Run: `pnpm build && pnpm test`

**Step 6: Commit**

```bash
git add src/pages/Focus.tsx src/components/SuggestionBanner.tsx src/components/Lightbox.tsx src/components/DateRangePicker.tsx src/pages/Timeline.tsx src/pages/Automation.tsx
git commit -m "refactor: apply iconSize tokens + fix 6 missing focusRing instances"
```

---

### Task 7: Biome Setup + GritQL Plugin

**Files:**
- Create: `biome.json`
- Create: `plugins/banned-classes.grit`
- Modify: `package.json`

**Step 1: Install Biome**

Run: `pnpm add -D @biomejs/biome`

**Step 2: Create biome.json**

```json
{
  "$schema": "https://biomejs.dev/schemas/2.0.0/schema.json",
  "organizeImports": {
    "enabled": true
  },
  "linter": {
    "enabled": true,
    "rules": {
      "recommended": true,
      "nursery": {
        "useSortedClasses": {
          "level": "warn",
          "options": {
            "attributes": ["className"]
          }
        }
      }
    }
  },
  "formatter": {
    "enabled": true,
    "indentStyle": "space",
    "indentWidth": 2,
    "lineWidth": 120
  },
  "javascript": {
    "formatter": {
      "quoteStyle": "single",
      "trailingCommas": "all",
      "semicolons": "asNeeded"
    }
  },
  "files": {
    "ignore": [
      "dist/**",
      "node_modules/**",
      "storybook-static/**",
      "*.config.js",
      "*.config.ts"
    ]
  },
  "plugins": [
    "./plugins/banned-classes.grit"
  ]
}
```

**Step 3: Create GritQL plugin**

Create `plugins/banned-classes.grit`:

```grit
language js;

`<$_ className=$value $$$/>` where {
  $value <: contains r"(bg|text|border)-(slate|gray)-\d",
  register_diagnostic(
    span = $value,
    message = "Hardcoded color class detected. Use semantic tokens from styles/tokens.ts instead (e.g., bg-surface-muted, text-content-secondary).",
    severity = "error"
  )
}
```

**Step 4: Update package.json scripts**

Replace the dead `lint` script and add new ones:

```json
"lint": "biome check src/",
"lint:fix": "biome check --write src/",
"format": "biome format --write src/"
```

**Step 5: Run lint to verify**

Run: `pnpm lint`
Expected: Pass with 0 errors (all hardcoded slate/gray classes were already removed in previous sessions, except intentional ones in SuggestionBanner/ActivityHeatmap which use template literals, not static className strings)

If the GritQL plugin catches false positives in SuggestionBanner or ActivityHeatmap (template literal className), we'll need to add `biome-ignore` comments to those specific lines.

**Step 6: Run build + tests to confirm no regressions**

Run: `pnpm build && pnpm test`

**Step 7: Commit**

```bash
git add biome.json plugins/banned-classes.grit package.json
git commit -m "feat: add Biome linter with GritQL plugin for design token enforcement"
```

---

### Task 8: Update DesignTokens Storybook

**Files:**
- Modify: `src/stories/DesignTokens.stories.tsx`

**Step 1: Add imports for new tokens**

```tsx
import { colors, spacing, typography, radius, interaction, form, dataViz, palette, motion, elevation, iconSize } from '../styles/tokens'
```

**Step 2: Add Motion Tokens section** (after Data Visualization section)

```tsx
<TokenSection title="Motion Tokens">
  <div className="space-y-3">
    {Object.entries(motion).map(([key, value]) => (
      <div key={key} className="flex items-center gap-4">
        <div className={`w-16 h-4 bg-brand rounded ${value}`} style={{ transform: 'scaleX(0.3)', transformOrigin: 'left' }} />
        <code className="text-xs text-content-secondary font-mono">{key}</code>
        <code className="text-xs text-content-tertiary font-mono">{value}</code>
      </div>
    ))}
  </div>
</TokenSection>
```

**Step 3: Add Elevation section**

```tsx
<TokenSection title="Elevation">
  <div className="grid grid-cols-2 gap-4">
    {Object.entries(elevation).map(([key, value]) => (
      <div key={key} className={`p-4 rounded-lg bg-surface-elevated ${value}`}>
        <p className="text-sm font-medium text-content">{key}</p>
        <code className="text-xs text-content-tertiary font-mono">{value}</code>
      </div>
    ))}
  </div>
</TokenSection>
```

**Step 4: Add Icon Sizes section**

```tsx
<TokenSection title="Icon Sizes">
  <div className="flex flex-wrap items-end gap-6">
    {Object.entries(iconSize).map(([key, value]) => (
      <div key={key} className="flex flex-col items-center gap-2">
        <div className={`${value} bg-brand rounded`} />
        <code className="text-xs text-content-tertiary font-mono">{key}</code>
        <code className="text-[10px] text-content-tertiary font-mono">{value}</code>
      </div>
    ))}
  </div>
</TokenSection>
```

**Step 5: Verify Storybook renders**

Run: `pnpm build` (stories excluded from TS build, just verify no import errors)

**Step 6: Commit**

```bash
git add src/stories/DesignTokens.stories.tsx
git commit -m "feat(storybook): add motion, elevation, iconSize token visualizations"
```

---

### Task 9: Final Verification

**Step 1: Full build**

Run: `pnpm build`
Expected: Success

**Step 2: Full test suite**

Run: `pnpm test`
Expected: 61/61 pass

**Step 3: Lint check**

Run: `pnpm lint`
Expected: 0 errors

**Step 4: Grep for remaining hardcoded slate/gray (rg verification)**

Run: `rg '(bg|text|border)-(slate|gray)-\d' src/ --glob='*.{ts,tsx}' -l`
Expected: Only intentional files (SuggestionBanner.tsx, ActivityHeatmap.tsx, variants.ts)

**Step 5: Grep for remaining ad-hoc patterns**

Run: `rg 'duration-[0-9]' src/ --glob='*.{ts,tsx}'`
Expected: 0 matches (all replaced with motion tokens)

Run: `rg 'shadow-(sm|md|lg|xl|2xl)' src/ --glob='*.{ts,tsx}' -l`
Expected: Only tokens.ts (where elevation tokens are defined) and layout compound strings

**Step 6: Commit (if any cleanup needed)**

```bash
git add -A
git commit -m "chore: final verification cleanup"
```
