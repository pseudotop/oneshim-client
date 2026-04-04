# Automation Desktop UI Integration Spec

**Date**: 2026-04-04
**Branch**: `feat/analysis-wiring`
**Scope**: `src-tauri` (IPC, shortcuts, tray), `oneshim-web/frontend` (UI)

## 1. Problem Statement

Automation (RPA) is fully implemented in the backend with 20+ REST endpoints and a React dashboard page. But desktop-native access is missing:

| Gap | Current | Needed |
|-----|---------|--------|
| **IPC commands** | None (REST only) | Core automation ops via Tauri IPC |
| **Keyboard shortcut** | None | Quick trigger (Cmd+Shift+A) |
| **Tray presets** | "Automation Settings" link only | Run presets from tray |
| **Confirmation policy** | Always via GUI session flow | Configurable: always/trusted/never |

## 2. Design

### 2.1 IPC Commands (8 commands)

| Command | Maps to REST | Purpose |
|---------|-------------|---------|
| `get_automation_status` | GET /automation/status | Check enabled, providers |
| `list_automation_presets` | GET /automation/presets | List available presets |
| `run_automation_preset` | POST /automation/presets/{id}/run | Execute a preset |
| `execute_intent_hint` | POST /automation/execute-hint | Natural language → action |
| `get_automation_stats` | GET /automation/stats | Execution statistics |
| `get_audit_logs` | GET /automation/audit | Recent execution logs |
| `analyze_current_scene` | GET /automation/scene | Analyze visible screen |
| `get_automation_config` | (new) | Get confirmation policy setting |

**Implementation pattern**: Same as sync IPC — create `AutomationRuntimeState` holding `Arc<dyn AutomationPort>`, create `commands/automation.rs`.

### 2.2 Keyboard Shortcut

**Shortcut**: `CmdOrCtrl+Shift+A`

**Action**: Opens automation quick-access panel on the overlay.
1. Analyze current scene (fast)
2. Show detected elements + suggested actions on overlay
3. User selects action → confirmation → execute

**Registration**: Add to `setup_shortcuts.rs` following existing pattern.

### 2.3 Tray Menu Enhancement

Current tray has "Automation Settings" → navigates to page. Add:

```
── Automation ──
  ▸ Run Preset: [Preset 1]    (dynamically populated)
  ▸ Run Preset: [Preset 2]
  ▸ Run Preset: [Preset 3]
  ─────────────
  Automation Settings...       (existing)
```

**Dynamic preset loading**: On tray build, query presets via the automation port. Show top 3 (or all if ≤5). Each triggers `run_automation_preset` with confirmation.

### 2.4 Confirmation Policy

New config field in `AutomationConfig`:

```rust
pub enum AutomationConfirmPolicy {
    AlwaysConfirm,     // Show dialog before every execution (safest, default)
    TrustedOnly,       // Auto-execute "trusted" presets, confirm others
    NeverConfirm,      // Execute immediately (dangerous, power users only)
}
```

**Confirmation flow**:
- `AlwaysConfirm`: Emit event to main window → React shows confirmation modal → user accepts → execute
- `TrustedOnly`: Check preset `trusted` flag. If trusted, execute directly. Otherwise confirm.
- `NeverConfirm`: Execute immediately. Log warning.

### 2.5 Overlay Integration

When shortcut pressed or preset run:
1. `MagicOverlayHandle::emit_automation_action()` — show action being executed
2. On completion: show result badge (success/failure) for 3 seconds
3. Use existing `update_focus_highlight()` for element targeting

## 3. Files Changed

| File | Change | Lines (~) |
|------|--------|-----------|
| `src-tauri/src/commands/automation.rs` | **NEW** — 8 IPC commands | +120 |
| `src-tauri/src/commands/mod.rs` | Export module | +1 |
| `src-tauri/src/runtime_state.rs` | `AutomationRuntimeState` | +15 |
| `src-tauri/src/main.rs` | Register commands | +10 |
| `src-tauri/src/setup_shortcuts.rs` | Add CmdOrCtrl+Shift+A | +20 |
| `src-tauri/src/tray.rs` | Dynamic preset menu items | +30 |
| `crates/oneshim-core/src/config/sections/automation.rs` | `AutomationConfirmPolicy` enum | +15 |

**Estimated total**: ~210 lines Rust

## 4. Key Decisions

- **IPC wraps REST**: IPC commands call the same `AutomationPort` trait that REST handlers use. No duplication.
- **Confirmation via event**: Not native OS dialog. Emit Tauri event → React modal → callback. Consistent with existing patterns.
- **Default policy**: `AlwaysConfirm` — safest for RPA operations.
- **Tray presets cached**: Loaded once at startup, refreshed on preset CRUD.
