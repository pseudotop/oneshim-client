# ADR-003 Large File Split — Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split 3 files exceeding 2000 lines into directory modules per ADR-003, improving maintainability without changing any public API or behavior.

**Architecture:** Pure structural refactoring — move code blocks into sub-files, add `pub(super)` visibility, re-export from `mod.rs`. Zero logic changes. Each task produces an independently committable, fully tested state.

**Tech Stack:** Rust module system, `pub use` re-exports, `pub(super)` visibility, `#[cfg(test)]`

**Spec:** `docs/superpowers/specs/2026-03-21-adr-003-large-file-split-design.md`
**ADR:** `docs/architecture/ADR-003-directory-module-pattern.md` (updated with test-split exception)

---

## File Map

### Task 1 — `gui_interaction/mod.rs` test split (2,446 → ~400 + 6 test files)

| File | Change |
|------|--------|
| `crates/oneshim-automation/src/gui_interaction/mod.rs` | Remove 2050 lines of tests, keep re-exports + constants (~400 lines) |
| `crates/oneshim-automation/src/gui_interaction/tests/mod.rs` | New: shared mocks, fixtures, helpers, constants |
| `crates/oneshim-automation/src/gui_interaction/tests/session.rs` | New: session creation/get tests |
| `crates/oneshim-automation/src/gui_interaction/tests/highlight.rs` | New: highlight tests |
| `crates/oneshim-automation/src/gui_interaction/tests/confirm.rs` | New: confirm + ticket tests |
| `crates/oneshim-automation/src/gui_interaction/tests/execute.rs` | New: execute + complete + lifecycle tests |
| `crates/oneshim-automation/src/gui_interaction/tests/m5.rs` | New: M5 failure scenario tests |

### Task 2 — `scheduler/loops.rs` split (1,983 → directory module)

| File | Change |
|------|--------|
| `src-tauri/src/scheduler/loops.rs` | Remove, replace with `loops/mod.rs` |
| `src-tauri/src/scheduler/loops/mod.rs` | New: re-exports, constants |
| `src-tauri/src/scheduler/loops/helpers.rs` | New: build_segment_stats, handle_event_analysis, handle_frame_capture, record_to_segment_summary |
| `src-tauri/src/scheduler/loops/monitor.rs` | New: spawn_monitor_loop |
| `src-tauri/src/scheduler/loops/system.rs` | New: spawn_metrics_loop, spawn_process_loop, spawn_aggregation_loop |
| `src-tauri/src/scheduler/loops/network.rs` | New: spawn_sync_loop, spawn_heartbeat_loop |
| `src-tauri/src/scheduler/loops/intelligence.rs` | New: spawn_analysis_loop, spawn_focus_loop, spawn_coaching_loop |
| `src-tauri/src/scheduler/loops/events.rs` | New: spawn_event_snapshot_loop, spawn_notification_loop |
| `src-tauri/src/scheduler/loops/sync.rs` | New: spawn_cross_device_sync_loop, spawn_oauth_refresh_loop |

### Task 3 — `provider_adapters.rs` split (1,950 → directory module)

| File | Change |
|------|--------|
| `src-tauri/src/provider_adapters.rs` | Remove, replace with `provider_adapters/mod.rs` |
| `src-tauri/src/provider_adapters/mod.rs` | New: re-exports, resolve_ai_provider_adapters |
| `src-tauri/src/provider_adapters/types.rs` | New: ProviderSource, AiProviderAdapters, ExternalOcrPrivacyGuard |
| `src-tauri/src/provider_adapters/guarded_ocr.rs` | New: GuardedOcrProvider |
| `src-tauri/src/provider_adapters/surface.rs` | New: surface transport resolution |
| `src-tauri/src/provider_adapters/ocr_resolver.rs` | New: OCR provider resolution |
| `src-tauri/src/provider_adapters/llm_resolver.rs` | New: LLM provider resolution |
| `src-tauri/src/provider_adapters/helpers.rs` | New: endpoint config, fallback formatting |
| `src-tauri/src/provider_adapters/tests.rs` | New: all tests |

---

## Task 1: Split `gui_interaction/mod.rs` tests

**Why:** 2,446 lines, of which ~2,050 are tests. Production code is already split into `service.rs`, `crypto.rs`, `helpers.rs`, `types.rs`. Per updated ADR-003 §4, test blocks exceeding 1,000 lines should be extracted into a `tests/` sub-directory.

**Files:**
- Modify: `crates/oneshim-automation/src/gui_interaction/mod.rs`
- Create: `crates/oneshim-automation/src/gui_interaction/tests/mod.rs`
- Create: `crates/oneshim-automation/src/gui_interaction/tests/session.rs`
- Create: `crates/oneshim-automation/src/gui_interaction/tests/highlight.rs`
- Create: `crates/oneshim-automation/src/gui_interaction/tests/confirm.rs`
- Create: `crates/oneshim-automation/src/gui_interaction/tests/execute.rs`
- Create: `crates/oneshim-automation/src/gui_interaction/tests/m5.rs`

- [ ] **Step 1.1: Record baseline test count**

```
cargo test -p oneshim-automation -- gui_interaction 2>&1 | grep "test result"
```
Record the exact number (expected: ~83 passed).

- [ ] **Step 1.2: Create `tests/mod.rs` with shared infrastructure**

Create `tests/mod.rs` containing:
- All `use` imports from the current `#[cfg(test)] mod tests` block
- `TEST_HMAC_SECRET` constant
- `MockElementFinder`, `MockFocusProbe`, `MockOverlayDriver`, `PermissionDeniedElementFinder` structs + impls
- `make_focus()`, `make_scene()`, `make_service()`, `make_service_full()`, `make_service_with_finder()`, `make_drifted_focus()` fixture builders
- `default_create_request()`, `create_test_session()`, `create_and_highlight()`, `create_highlight_and_confirm()` helpers
- `encode_hex`, `decode_hex` imports
- Sub-module declarations: `mod session; mod highlight; mod confirm; mod execute; mod m5;`

- [ ] **Step 1.3: Create `tests/session.rs`**

Move tests from `// ── Session creation tests ──` and `// ── Get session tests ──` sections.
Each test function uses `use super::*;` at the top.

- [ ] **Step 1.4: Create `tests/highlight.rs`**

Move tests from `// ── Highlight tests ──` section.

- [ ] **Step 1.5: Create `tests/confirm.rs`**

Move tests from `// ── Confirm tests ──` section, including ticket signing/verification tests.

- [ ] **Step 1.6: Create `tests/execute.rs`**

Move tests from `// ── Execute tests ──`, `// ── Complete tests ──`, `// ── Lifecycle tests ──`, and `// ── Utility tests ──` sections.

- [ ] **Step 1.7: Create `tests/m5.rs`**

Move all `m5_` prefixed tests from the M5 failure scenario section.

- [ ] **Step 1.8: Update `mod.rs`**

Replace the entire `#[cfg(test)] mod tests { ... }` block with:
```rust
#[cfg(test)]
mod tests;
```

- [ ] **Step 1.9: Verify**

```
cargo test -p oneshim-automation -- gui_interaction 2>&1 | grep "test result"
cargo clippy -p oneshim-automation -- -D warnings
```
Test count must match Step 1.1 exactly.

- [ ] **Step 1.10: Commit**

```
git add crates/oneshim-automation/src/gui_interaction/
git commit -m "refactor(automation): split gui_interaction tests into directory module (ADR-003)"
```

---

## Task 2: Split `scheduler/loops.rs`

**Why:** 1,983 lines with 12 `spawn_*` loop functions + helpers. Each loop is functionally independent. The `loops.rs` file was originally split from `scheduler.rs` per ADR-003, but has grown beyond the threshold again.

**Files:**
- Remove: `src-tauri/src/scheduler/loops.rs`
- Create: `src-tauri/src/scheduler/loops/mod.rs`
- Create: `src-tauri/src/scheduler/loops/helpers.rs`
- Create: `src-tauri/src/scheduler/loops/monitor.rs`
- Create: `src-tauri/src/scheduler/loops/system.rs`
- Create: `src-tauri/src/scheduler/loops/network.rs`
- Create: `src-tauri/src/scheduler/loops/intelligence.rs`
- Create: `src-tauri/src/scheduler/loops/events.rs`
- Create: `src-tauri/src/scheduler/loops/sync.rs`
- Modify: `src-tauri/src/scheduler/mod.rs` (update `mod loops;`)

- [ ] **Step 2.1: Record baseline**

```
cargo test -p oneshim-app --bin oneshim 2>&1 | grep "test result"
cargo test -p oneshim-app --test gui_smoke_e2e 2>&1 | grep "test result"
```

- [ ] **Step 2.2: Create `loops/` directory and `mod.rs`**

Move imports and the `pub use` re-export (`record_to_segment_summary`) to `loops/mod.rs`.
Add sub-module declarations.

- [ ] **Step 2.3: Create `loops/helpers.rs`**

Move: `build_segment_stats_snapshot()`, `handle_event_analysis()`, `handle_frame_capture()`, `build_personalization_prompt()`, coaching constants, `record_to_segment_summary()`.

Use `pub(super)` for functions called from other sub-modules. Use `pub(crate)` for `record_to_segment_summary` (used by web handlers).

- [ ] **Step 2.4: Create `loops/monitor.rs`**

Move `spawn_monitor_loop()` — the largest function (576 lines). Uses `use super::helpers::*` for shared functions.

- [ ] **Step 2.5: Create `loops/system.rs`**

Move `spawn_metrics_loop()`, `spawn_process_loop()`, `spawn_aggregation_loop()`.

- [ ] **Step 2.6: Create `loops/network.rs`**

Move `spawn_sync_loop()`, `spawn_heartbeat_loop()`.

- [ ] **Step 2.7: Create `loops/intelligence.rs`**

Move `spawn_analysis_loop()`, `spawn_focus_loop()`, `spawn_coaching_loop()`.

- [ ] **Step 2.8: Create `loops/events.rs`**

Move `spawn_event_snapshot_loop()`, `spawn_notification_loop()`.

- [ ] **Step 2.9: Create `loops/sync.rs`**

Move `spawn_cross_device_sync_loop()`, `spawn_oauth_refresh_loop()`.

- [ ] **Step 2.10: Update `scheduler/mod.rs`**

The `mod loops;` declaration already exists. Since `loops.rs` becomes `loops/mod.rs`, no change needed in `scheduler/mod.rs` — Rust resolves `mod loops;` to either `loops.rs` or `loops/mod.rs` automatically.

- [ ] **Step 2.11: Delete old `loops.rs`**

Only after verifying compilation. The `loops/mod.rs` replaces it.

- [ ] **Step 2.12: Verify**

```
cargo check -p oneshim-app
cargo test -p oneshim-app --bin oneshim 2>&1 | grep "test result"
cargo test -p oneshim-app --test gui_smoke_e2e 2>&1 | grep "test result"
cargo clippy -p oneshim-app -- -D warnings
```

- [ ] **Step 2.13: Commit**

```
git add src-tauri/src/scheduler/
git commit -m "refactor(scheduler): split loops.rs into directory module (ADR-003)"
```

---

## Task 3: Split `provider_adapters.rs`

**Why:** 1,950 lines mixing types, privacy wrappers, resolver logic, and tests. Clear responsibility boundaries exist between OCR resolution, LLM resolution, and surface transport configuration.

**Files:**
- Remove: `src-tauri/src/provider_adapters.rs`
- Create: `src-tauri/src/provider_adapters/mod.rs`
- Create: `src-tauri/src/provider_adapters/types.rs`
- Create: `src-tauri/src/provider_adapters/guarded_ocr.rs`
- Create: `src-tauri/src/provider_adapters/surface.rs`
- Create: `src-tauri/src/provider_adapters/ocr_resolver.rs`
- Create: `src-tauri/src/provider_adapters/llm_resolver.rs`
- Create: `src-tauri/src/provider_adapters/helpers.rs`
- Create: `src-tauri/src/provider_adapters/tests.rs`
- Modify: `src-tauri/src/main.rs` (no change needed — `mod provider_adapters;` resolves automatically)

- [ ] **Step 3.1: Record baseline**

```
cargo test -p oneshim-app --bin oneshim -- provider_adapters 2>&1 | grep "test result"
```

- [ ] **Step 3.2: Create `provider_adapters/mod.rs`**

Move: `resolve_ai_provider_adapters()` main entry point.
Add `pub use` re-exports for `AiProviderAdapters`, `ExternalOcrPrivacyGuard`, `ProviderSource`.
Add sub-module declarations.

- [ ] **Step 3.3: Create `provider_adapters/types.rs`**

Move: `ProviderSource` enum + `as_str()`, `AiProviderAdapters` struct, `ExternalOcrPrivacyGuard` struct + impl.

- [ ] **Step 3.4: Create `provider_adapters/guarded_ocr.rs`**

Move: `GuardedOcrProvider` struct + `OcrProvider` impl.

- [ ] **Step 3.5: Create `provider_adapters/surface.rs`**

Move: `resolve_direct_surface_adapters()`, `configured_ocr_surface_transport()`, `configured_llm_surface_transport()`, `llm_uses_managed_oauth()`, `unsupported_ocr_surface_runtime()`.

- [ ] **Step 3.6: Create `provider_adapters/ocr_resolver.rs`**

Move: `resolve_ocr_provider()`, `resolve_cli_subscription_ocr_provider()`, `resolve_ocr_provider_oauth()`.

- [ ] **Step 3.7: Create `provider_adapters/llm_resolver.rs`**

Move: `resolve_llm_provider()`, `resolve_cli_subscription_llm_provider_with_detected()`, `resolve_llm_provider_oauth()`, `cli_subscription_unavailable_reason()`.

- [ ] **Step 3.8: Create `provider_adapters/helpers.rs`**

Move: `oauth_llm_endpoint()`, `require_endpoint_config()`, `resolve_remote_with_optional_fallback()`, `format_fallback_reason()`.

- [ ] **Step 3.9: Create `provider_adapters/tests.rs`**

Move all `#[cfg(test)] mod tests` content.

- [ ] **Step 3.10: Update `mod.rs` with `#[cfg(test)] mod tests;`**

- [ ] **Step 3.11: Delete old `provider_adapters.rs`**

- [ ] **Step 3.12: Verify**

```
cargo check -p oneshim-app
cargo test -p oneshim-app --bin oneshim -- provider_adapters 2>&1 | grep "test result"
cargo clippy -p oneshim-app -- -D warnings
```

- [ ] **Step 3.13: Commit**

```
git add src-tauri/src/provider_adapters/ src-tauri/src/main.rs
git commit -m "refactor(providers): split provider_adapters.rs into directory module (ADR-003)"
```

---

## Task 4: Update ADR-003 Applied Splits table

- [ ] **Step 4.1: Add new splits to ADR-003**

In `docs/architecture/ADR-003-directory-module-pattern.md`, add to the Applied Splits table:

```markdown
| `gui_interaction/mod.rs` | `gui_interaction/tests/{mod, session, highlight, confirm, execute, m5}.rs` | oneshim-automation |
| `scheduler/loops.rs` | `scheduler/loops/{mod, helpers, monitor, system, network, intelligence, events, sync}.rs` | src-tauri |
| `provider_adapters.rs` | `provider_adapters/{mod, types, guarded_ocr, surface, ocr_resolver, llm_resolver, helpers, tests}.rs` | src-tauri |
```

- [ ] **Step 4.2: Commit**

```
git add docs/architecture/ADR-003-directory-module-pattern.md
git commit -m "docs: update ADR-003 Applied Splits table with new directory modules"
```

---

## Verification

After all tasks:

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

Total test count must equal pre-split count (2328). Zero new tests, zero removed tests.
