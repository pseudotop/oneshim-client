# Suggestions Panel Overlay — Technical Specification

> **Version**: 1.1 (reviewed — all Critical/Important resolved)
> **Date**: 2026-03-24
> **Scope**: Overlay React component + Rust event emission + IPC integration
> **Depends on**: A3 (Suggestion IPC commands — already merged in PR #152)

---

## 1. Overview

Add a **Suggestions Panel** to the MagicOverlay that displays pending AI suggestions and allows users to accept, reject, or defer them. The panel integrates with the existing overlay architecture (CoachingPopup, GoalProgressBar, FocusHighlight, HeatmapGhost) and reuses the A3 IPC commands (`get_pending_suggestions`, `get_suggestion_history`, `submit_suggestion_feedback`).

### Design Approach: Pull-Based with Event Notifications

Unlike coaching (push-based: Rust emits `overlay:show-coaching`), the suggestions panel uses a **hybrid pull approach**:

1. **Panel toggle**: User opens/closes the panel via a trigger (keyboard shortcut or coaching action)
2. **Initial fetch**: On open, panel calls `get_pending_suggestions` via Tauri IPC (pull)
3. **Refresh signal**: Rust emits `overlay:suggestions-changed` when queue changes (new arrival, feedback processed)
4. **Panel re-fetches**: On receiving the signal, panel calls `get_pending_suggestions` again

**Why not pure push?** Suggestions are fetched on-demand (panel open) rather than streamed every tick. This avoids emitting 50-item payloads every 3 seconds when the panel is closed.

---

## 2. User Interaction Flow

```
User presses Cmd+Shift+S (or clicks suggestion count badge)
  → Overlay becomes interactive (set_ignore_cursor_events = false)
  → SuggestionsPanel slides in from right
  → Panel calls get_pending_suggestions via IPC
  → Items rendered with priority colors and action buttons

User clicks [Accept] on a suggestion
  → Panel calls submit_suggestion_feedback(id, "accept") via IPC
  → Item fades out, panel re-fetches

User clicks [Reject] on a suggestion
  → Panel calls submit_suggestion_feedback(id, "reject") via IPC
  → Item fades out, panel re-fetches

User clicks [Defer] on a suggestion
  → Panel calls submit_suggestion_feedback(id, "defer") via IPC
  → Item remains but moves to lower priority

User presses Escape or clicks outside
  → Panel slides out
  → Overlay returns to click-through mode
```

---

## 3. Component Design

### 3.1 SuggestionsPanel

**Position**: Fixed right side, below coaching popup area.

```
Screen layout:
┌──────────────────────────────────────────────────────┐
│                                    [CoachingPopup]   │ z-50
│                                    ┌──────────────┐  │
│                                    │ SUGGESTIONS  │  │ z-45
│                                    │ (3 pending)  │  │
│                                    │              │  │
│                                    │ ✉ Email Draft│  │
│                                    │ HIGH · 95%   │  │
│                                    │ [✓] [✗] [→]  │  │
│                                    │              │  │
│                                    │ ⚡ Prod Tip   │  │
│                                    │ MED · 87%    │  │
│                                    │ [✓] [✗] [→]  │  │
│                                    │              │  │
│                                    └──────────────┘  │
│ [FocusMode]                                          │
│                     [GoalProgressBar]                 │ z-40
└──────────────────────────────────────────────────────┘
```

### 3.2 Component Props

```typescript
// SuggestionsPanel.tsx — reads from reducer state, not local state
interface SuggestionsPanelProps {
  open: boolean
  suggestions: SuggestionViewDto[]
  onClose: () => void
  onRefresh: () => void
}

// SuggestionItem (internal)
interface SuggestionItemProps {
  item: SuggestionViewDto
  onAction: (id: string, action: 'accept' | 'reject' | 'defer') => void
}
```

### 3.3 SuggestionViewDto (from A3 IPC)

Already defined in `src-tauri/src/commands/suggestions.rs`:

```typescript
interface SuggestionViewDto {
  id: string              // suggestion_id
  title: string           // e.g., "Work Guidance" (from type_to_title)
  body: string            // suggestion content
  priority: string        // "critical" | "high" | "medium" | "low"
  category: string | null // Option<String> in Rust, always null for now
  source: string          // "server" | "local"
  created_at: string      // ISO 8601
  is_read: boolean
}
```

**IPC parameter naming**: Tauri v2 auto-converts camelCase JS args to snake_case Rust params. The existing CoachingPopup uses `{ messageId }` for a Rust `message_id` param. Follow the same pattern:
```typescript
await invoke('submit_suggestion_feedback', { suggestionId: id, action })  // CORRECT
// Tauri maps JS camelCase → Rust snake_case automatically
```

### 3.4 Styling

Following existing overlay component patterns (CoachingPopup reference):

```
Panel container:
  fixed right-4 top-20 z-[45] w-80
  rounded-xl border border-content-inverse/10
  bg-surface-sunken/90 shadow-2xl backdrop-blur-md
  (Note: click-through is managed by Tauri set_ignore_cursor_events,
   NOT CSS pointer-events. No pointer-events class needed.)

Panel header:
  px-4 py-3 border-b border-content-inverse/5
  flex justify-between items-center

Header text:
  text-xs font-semibold uppercase tracking-wider text-content-secondary

Close button:
  text-content-tertiary hover:text-content text-sm

Suggestion item:
  px-4 py-3 border-b border-content-inverse/5

Item title:
  text-sm font-medium text-content

Item body:
  text-xs text-content-secondary mt-1 line-clamp-2

Priority badge:
  inline-flex items-center px-1.5 py-0.5 text-[10px] font-semibold rounded
  Colors by priority:
    critical: bg-semantic-error/20 text-semantic-error
    high:     bg-semantic-warning/20 text-semantic-warning
    medium:   bg-brand/20 text-brand
    low:      bg-content-secondary/20 text-content-secondary

Source badge:
  text-[10px] text-content-tertiary

Action buttons:
  flex gap-1.5 mt-2
  Each: rounded-md px-2 py-1 text-xs
  Accept: bg-semantic-success/15 text-semantic-success hover:bg-semantic-success/25
  Reject: bg-semantic-error/15 text-semantic-error hover:bg-semantic-error/25
  Defer:  bg-content-inverse/10 text-content-secondary hover:bg-content-inverse/15

Empty state:
  px-4 py-8 text-center text-xs text-content-tertiary
```

### 3.5 Animations

- **Panel enter**: `translate-x-full → translate-x-0` (slide from right, 300ms ease-out)
- **Panel exit**: `translate-x-0 → translate-x-full` (slide to right, 200ms ease-in)
- **Item removal**: opacity 1→0, height collapse (200ms)
- Use `transition-transform` and `transition-opacity` from design tokens

---

## 4. Overlay State Integration

### 4.1 New State Fields

Add to `OverlayState` in `types.ts`:

```typescript
interface OverlayState {
  // ... existing fields
  suggestionsPanelOpen: boolean
  suggestions: SuggestionViewDto[]
}
```

### 4.2 New Actions

Add to `OverlayAction` in `useOverlayEvents.ts`:

```typescript
type OverlayAction =
  | ... // existing
  | { type: 'toggle-suggestions-panel'; payload?: boolean }
  | { type: 'set-suggestions'; payload: SuggestionViewDto[] }
  | { type: 'remove-suggestion'; payload: string } // suggestion_id
```

### 4.3 New Tauri Events

**From Rust → React:**

| Event | Payload | When |
|-------|---------|------|
| `overlay:suggestions-changed` | `{ count: number }` | New suggestion arrives via SSE, or feedback processed |
| `overlay:toggle-suggestions` | `()` (no payload) | Keyboard shortcut Cmd+Shift+S pressed — frontend toggles own state |

**From React → Rust (IPC invoke):**

| Command | Params | Returns |
|---------|--------|---------|
| `get_pending_suggestions` | (none) | `SuggestionViewDto[]` |
| `submit_suggestion_feedback` | `{ suggestion_id, action }` | `()` |
| `toggle_overlay_interactive` | `{ interactive: bool }` | `()` |

All 3 IPC commands already exist from A3. No new Rust commands needed.

---

## 5. Rust-Side Changes

### 5.1 MagicOverlayHandle — New Methods

Add to `src-tauri/src/magic_overlay.rs`:

```rust
/// Notify overlay that suggestion queue changed (new arrival or feedback).
pub fn emit_suggestions_changed(&self, count: usize) {
    let _ = self.app_handle.emit(
        "overlay:suggestions-changed",
        serde_json::json!({ "count": count }),
    );
}

/// Toggle suggestions panel from keyboard shortcut (no arg — frontend inverts state).
pub fn emit_toggle_suggestions(&self) {
    let _ = self.app_handle.emit("overlay:toggle-suggestions", ());
}
```

### 5.2 Keyboard Shortcut

In `src-tauri/src/setup.rs`, add alongside the existing `Cmd+Shift+O` shortcut. Must match the exact 3-argument callback pattern with `ShortcutState::Pressed` guard, and use `async_runtime::spawn` because `set_interactive` is async:

```rust
// Cmd+Shift+S (macOS) / Ctrl+Shift+S (Windows/Linux): Toggle suggestions panel
app.global_shortcut()
    .on_shortcut("CmdOrCtrl+Shift+S", |app_handle, _shortcut, event| {
        if event.state == ShortcutState::Pressed {
            let handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                let state: tauri::State<'_, AppState> = handle.state();
                if let Some(ref overlay) = state.magic_overlay {
                    // Emit toggle — frontend inverts its own panel state
                    overlay.emit_toggle_suggestions();
                    overlay.set_interactive(true).await;
                }
            });
        }
    });
```

**Note on toggle**: `emit_toggle_suggestions()` takes no argument. The React frontend toggles its own `suggestionsPanelOpen` state. When the panel closes, the frontend calls `toggle_overlay_interactive(false)` via IPC to return to click-through. This ensures the shortcut works as a true toggle (open→close, close→open).

### 5.3 Suggestion Queue Change Notification

In `src-tauri/src/commands/suggestions.rs`, after `submit_suggestion_feedback` processes feedback:

```rust
// After accept/reject/defer feedback is processed:
if let Some(ref overlay) = state.magic_overlay {
    let queue = mgr.queue().lock().await;
    overlay.emit_suggestions_changed(queue.len());
}
```

Also in the network/SSE path: when `SuggestionReceiver` pushes a new suggestion to the queue, emit the event. This requires passing `MagicOverlayHandle` to the receiver or using a channel.

**Simpler alternative**: Instead of modifying `SuggestionReceiver`, emit the event periodically from the scheduler's network loop (every 10s sync interval). This avoids coupling the suggestion crate to overlay concerns.

---

## 6. Frontend Implementation

### 6.1 New Files

| File | Purpose |
|------|---------|
| `crates/oneshim-web/frontend/src/overlay/components/SuggestionsPanel.tsx` | Panel + item list |
| `crates/oneshim-web/frontend/src/overlay/components/SuggestionItem.tsx` | Individual suggestion card |

### 6.2 SuggestionsPanel.tsx

**Architecture**: The panel reads from the `useOverlayEvents` reducer state (`state.suggestions`, `state.suggestionsPanelOpen`), NOT local `useState`. All event listeners are registered in `useOverlayEvents.ts` (consistent with coaching, goals, etc.). The panel only handles IPC calls and rendering.

```typescript
import { invoke } from '@tauri-apps/api/core'

interface SuggestionsPanelProps {
  open: boolean
  suggestions: SuggestionViewDto[]
  onClose: () => void
  onRefresh: () => void  // triggers re-fetch via dispatch
}

export function SuggestionsPanel({ open, suggestions, onClose, onRefresh }: SuggestionsPanelProps) {
  // Fetch on open
  useEffect(() => {
    if (open) onRefresh()
  }, [open])

  async function handleAction(id: string, action: string) {
    // NOTE: snake_case parameter names to match Rust command signature
    await invoke('submit_suggestion_feedback', { suggestion_id: id, action })
    onRefresh()  // re-fetch after action
  }

  // Escape key handler
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape' && open) onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [open, onClose])

  return (
    <div className={`fixed right-4 top-20 z-[45] w-80 ... transform transition-transform
      ${open ? 'translate-x-0' : 'translate-x-[calc(100%+1rem)]'}`}>
      {/* Header with count + close button */}
      {/* Item list */}
      {/* Empty state when suggestions.length === 0 */}
    </div>
  )
}
```

**Event listeners in useOverlayEvents.ts** (NOT in the component):

```typescript
// In useOverlayEvents.ts — alongside existing u1-u9 listeners:

// u10: Suggestions toggle (from Cmd+Shift+S shortcut)
const u10 = await listen('overlay:toggle-suggestions', () => {
  dispatch({ type: 'toggle-suggestions-panel' })
})

// u11: Suggestions changed (queue updated) — triggers re-fetch
const u11 = await listen<{ count: number }>('overlay:suggestions-changed', async () => {
  // Re-fetch and update state
  const { invoke } = await import('@tauri-apps/api/core')
  const suggestions = await invoke<SuggestionViewDto[]>('get_pending_suggestions')
  dispatch({ type: 'set-suggestions', payload: suggestions })
})

// Add u10, u11 to cleanup array
return () => { u1(); u2(); ... u10(); u11() }
```

This avoids the listener leak (C2 fix) and keeps all state in the reducer (I1 fix).

### 6.3 SuggestionItem.tsx

```typescript
export function SuggestionItem({ item, onAction }: SuggestionItemProps) {
  return (
    <div className="px-4 py-3 border-b border-content-inverse/5">
      <div className="flex items-start justify-between">
        <span className="text-sm font-medium text-content">{item.title}</span>
        <PriorityBadge priority={item.priority} />
      </div>
      <p className="text-xs text-content-secondary mt-1 line-clamp-2">{item.body}</p>
      <div className="flex items-center gap-1.5 mt-2">
        <button onClick={() => onAction(item.id, 'accept')} className="accept-btn">Accept</button>
        <button onClick={() => onAction(item.id, 'reject')} className="reject-btn">Reject</button>
        <button onClick={() => onAction(item.id, 'defer')} className="defer-btn">Later</button>
        <span className="ml-auto text-[10px] text-content-tertiary">{item.source}</span>
      </div>
    </div>
  )
}
```

---

## 7. App.tsx Integration

```typescript
// In OverlayApp component:
// NOTE: App.tsx currently only destructures `state` — must add `dispatch`:
const { state, dispatch } = useOverlayEvents()
const isRich = state.mode === 'rich' || state.mode === 'adaptive'

async function handleClosePanel() {
  dispatch({ type: 'toggle-suggestions-panel', payload: false })
  // Return overlay to click-through — import invoke directly (not tauriInvoke helper)
  const { invoke } = await import('@tauri-apps/api/core')
  await invoke('toggle_overlay_interactive', { interactive: false })
}

async function handleRefreshSuggestions() {
  const { invoke } = await import('@tauri-apps/api/core')
  const suggestions = await invoke<SuggestionViewDto[]>('get_pending_suggestions')
  dispatch({ type: 'set-suggestions', payload: suggestions })
}

return (
  <div className="relative h-screen w-screen overflow-hidden">
    <TrackingBorder />
    <FocusModeIndicator active={state.focusMode} />
    {state.focusHighlight && <FocusHighlight highlight={state.focusHighlight} />}
    {state.coaching && <CoachingPopup message={state.coaching} />}

    {/* NEW: Suggestions Panel — reads from reducer state, not local state */}
    <SuggestionsPanel
      open={state.suggestionsPanelOpen}
      suggestions={state.suggestions}
      onClose={handleClosePanel}
      onRefresh={handleRefreshSuggestions}
    />

    {isRich && state.goals.length > 0 && <GoalProgressBar goals={state.goals} />}
    {isRich && <HeatmapGhost />}
  </div>
)
```

---

## 8. Edge Cases

- **Panel open + coaching popup appears**: Both render simultaneously (different z-index: coaching z-50, panel z-45). Coaching takes visual priority.
- **Panel open + focus mode activates**: Panel stays open (user explicitly opened it). New suggestions won't arrive (coaching suppressed), but panel shows existing queue.
- **Empty queue**: Show "No suggestions yet" empty state with subtle icon.
- **Suggestion expires while panel open**: On re-fetch after `suggestions-changed` event, expired items are naturally excluded by the queue's `remove_expired()`.
- **Overlay not interactive**: Panel is invisible/non-interactive by default. Only visible after toggle (shortcut or action).
- **Panel close**: Returns overlay to click-through via `toggle_overlay_interactive(false)`.

---

## 9. New Files Summary

| File | Purpose | Layer |
|------|---------|-------|
| `crates/oneshim-web/frontend/src/overlay/components/SuggestionsPanel.tsx` | Panel container + fetch logic | Frontend |
| `crates/oneshim-web/frontend/src/overlay/components/SuggestionItem.tsx` | Individual suggestion card | Frontend |

### Modified Files

| File | Changes |
|------|---------|
| `crates/oneshim-web/frontend/src/overlay/types.ts` | Add `SuggestionViewDto`, `suggestionsPanelOpen` state |
| `crates/oneshim-web/frontend/src/overlay/hooks/useOverlayEvents.ts` | Add actions, event listeners, reducer cases |
| `crates/oneshim-web/frontend/src/overlay/App.tsx` | Render SuggestionsPanel, handle close |
| `src-tauri/src/magic_overlay.rs` | Add `emit_suggestions_changed()`, `emit_toggle_suggestions()` |
| `src-tauri/src/commands/suggestions.rs` | Emit suggestions-changed after feedback |
| `src-tauri/src/setup.rs` | Add Cmd+Shift+S keyboard shortcut |

---

## 10. Out of Scope

- Suggestion creation/generation (server-side)
- Suggestion history view in overlay (use web dashboard)
- Drag-and-drop suggestion reordering
- Inline suggestion editing
- Notification sound on new suggestion
