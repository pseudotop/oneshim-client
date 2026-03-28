# UI Primitive Migration Spec

> **Status**: v3 (deep-reviewed)
> **Scope**: `crates/oneshim-web/frontend/`
> **Depends on**: Storybook & Design System commit `c67877c4`

## 1. Migration Targets (Verified)

Each target was read in context and verified as a genuine migration candidate.

### Divider (6 targets, 4 files)

| File | Line | Current Code | Migration |
|------|------|-------------|-----------|
| `shell/ActivityBar.tsx` | 148 | `<hr className="my-1 w-6 border-muted border-t" />` | `<Divider className="my-1 w-6 border-muted" />` |
| `shell/ActivityBar.tsx` | 151 | (same as above) | (same) |
| `shell/ActivityBar.tsx` | 157 | (same as above) | (same) |
| `SegmentContextMenu.tsx` | 111 | `<hr className="my-1 border-DEFAULT border-t" />` | `<Divider className="my-1" />` |
| `DevToolbar.tsx` | 88 | `<hr className="border-gray-700" />` | `<Divider className="border-gray-700" />` |
| `TagInput.tsx` | 164 | `<div className="border-muted border-t" />` | `<Divider className="border-muted" />` |

### Alert (5 targets, 3 files)

| File | Line | Current Pattern | Migration |
|------|------|----------------|-----------|
| `Privacy.tsx` | 287 | `rounded-lg border border-status-connected bg-semantic-success/20 p-4` — delete result success | `<Alert variant="success">` |
| `Privacy.tsx` | 437 | `rounded-lg border border-status-error bg-semantic-error/20 p-4` — restore error | `<Alert variant="error">` |
| `setting-tabs/OAuthConnectionPanel.tsx` | 243 | `rounded-lg border border-muted bg-surface-muted p-3` — CLI preference info | `<Alert variant="default">` |
| `setting-tabs/OAuthConnectionPanel.tsx` | 262 | `rounded-lg border border-muted bg-surface-muted p-3` — setup instructions | `<Alert variant="default">` |
| `setting-tabs/GeneralTab.tsx` | 141 | `rounded-lg border border-muted bg-surface-inset p-4` — update status info | `<Alert variant="info">` |

### Dialog (1 target)

| File | What | Shared Logic to Replace | Lines Saved |
|------|------|------------------------|-------------|
| `shell/ShortcutsHelp.tsx` | Keyboard shortcuts modal | Focus trap (L33-54), backdrop (L59-62), prev-focus save/restore (L18-30) | ~30 lines |

### Excluded After Deep Review

| Pattern | File | Reason |
|---------|------|--------|
| **Lightbox.tsx** | Dialog candidate | ESC handler conflicts with ArrowLeft/Right navigation; different backdrop color (`bg-surface-overlay/90` vs Dialog's `bg-black/50`); only 6 lines saved; high risk |
| **Privacy.tsx ConfirmModal** | Dialog candidate | Uses `role="alertdialog"` (not `dialog`); intentionally NO backdrop close (dangerous action); Dialog bakes in `onClick={onClose}` on backdrop; would need API change to Dialog |
| `border-b/border-t` | Chat.tsx (8 places) | Structural layout borders, not standalone separators |
| `rounded-lg border bg-surface-*` | AiAutomationTab.tsx (10 places) | Settings containers with form controls inside |
| `<input type="checkbox">` | NotificationSettings, GeneralTab, MonitoringTab | Integrated form grid layouts; restructuring too risky |
| ToggleRow.tsx | ToggleRow.tsx | Reverse layout (label-left, checkbox-right) |
| CommandPalette.tsx | CommandPalette.tsx | `role="combobox"` semantics, not `role="dialog"` |

---

## 2. Goals & Non-Goals

### Goals

1. Replace 6 ad-hoc `<hr>` / `<div border-t>` with `<Divider>` (4 files)
2. Replace 5 ad-hoc alert/info box patterns with `<Alert>` (3 files)
3. Refactor ShortcutsHelp to use `<Dialog>` + `<DialogContent>` (1 file, ~30 lines saved)
4. All existing tests and Storybook build continue to pass

### Non-Goals

- Migrating Lightbox (ESC/arrow key conflict)
- Migrating Privacy ConfirmModal (alertdialog, no-backdrop-close)
- Migrating structural borders, settings containers, checkboxes, ToggleRow, CommandPalette
- Changing any component's external API or behavior
- Adding new features or new props to existing primitives

---

## 3. Scope Summary

**Total: 12 replacements across 8 files**
- Divider: 6 replacements (4 files)
- Alert: 5 replacements (3 files)
- Dialog: 1 refactoring (1 file)

**Estimated diff**: ~+40 / -90 lines (net reduction of ~50 lines)

---

## 4. Validation Criteria

- [ ] `pnpm build` passes
- [ ] `pnpm lint` passes (0 violations)
- [ ] `pnpm test` passes (same 119/120 as before)
- [ ] `pnpm build-storybook` passes
- [ ] No component's external API changes
- [ ] ShortcutsHelp dialog focus trap + ESC close works correctly
