# Phase 1 — Iteration 2 Spec Review Findings

**Date**: 2026-04-25
**Spec under review**: spec v2 (commit `5f1add95`)
**Reviewers**: 1 fresh code-reviewer subagent + Q7 research subagent + direct verification
**Outcome**: 1 Critical (N-C1), 3 Important (N-I1 through N-I4), 3 Nice-to-have (N-N1 through N-N3) + Q7/Q8/Q9 resolved

---

## NEW Critical Issue

### N-C1 — `IpcError::from_string` does not exist

**Spec assumed** (§5.1 lines 224, 229, 235): `IpcError::from_string(format!(...))`
**Reality** (`src-tauri/src/ipc_error.rs:70`): `IpcError::new(code: impl Into<String>, message: impl Into<String>)`

**Impact**: 4 of 5 IPC commands in §5.1 would fail to compile.

**Fix applied in v3**: Changed all 4 `IpcError::from_string(...)` calls to `IpcError::new("autostart.<verb>_failed", format!(...))` with proper wire codes.

**Cascading consequence**: Wire codes `autostart.enable_failed`, `autostart.disable_failed`, `autostart.query_failed` must be added to:
- `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` (snapshot registry)
- `crates/oneshim-web/frontend/src/i18n/wire-errors.en.json`
- `crates/oneshim-web/frontend/src/i18n/wire-errors.ko.json`

Otherwise existing `scripts/check-wire-error-i18n-coverage.sh` CI gate fails. Documented in v3 §5.1 Wiring section + §10.1 commit dependency note.

---

## NEW Important Issues

### N-I1 — Wrong import path for `IpcError`

**Spec assumed** (line 215): `use crate::commands::IpcError;`
**Reality**: `use crate::ipc_error::IpcError;` — verified via `commands/settings.rs:3`, `commands/onboarding.rs:4`.

**Fix applied in v3**: Changed import path.

### N-I2 — ShowPromptCoordinator HMR pitfall

**Issue**: `let hasShownThisSession = false` at module level resets on Vite HMR reload during `pnpm dev`. May cause prompt re-fire after code edits in dev.

**Fix applied in v3**: Added explicit "Dev-only quirk" note in §5.5 documenting the limitation. Production builds unaffected. Optional `import.meta.env.DEV` reset documented but not required for PR-B1.

### N-I3 — Commit 7 (D-Bus presence check) effect unclear

**Issue**: Original commit 7 was a standalone commit just to log a warn at startup. No code path consumes it.

**Fix applied in v3**: Bundled D-Bus presence check into commit 6 (single-instance plugin registration) — saves a redundant commit. Total PR-B1 commits: 16 → 15. Estimate: 23h → 22h.

### N-I4 — Wayland kept-hidden window — no concrete fallback

**Issue**: I1 from iter-1 asked for Wayland kept-hidden window handling. v2 §9.5 mentions a smoke test but no concrete fallback code or acceptance criteria.

**Fix applied in v3**: Updated §13 Risk Register row "Wayland kept-hidden window unmappable on focus" to explicitly state:
- **Accepted as known limitation** in PR-B1
- Manual smoke test in §9.5 surfaces the case
- If broken: documented in PR-B2 `docs/guides/autostart.ko.md` user runbook
- Concrete fallback (e.g., `window.create()` if `is_visible()` returns false post-`set_focus()`) is a follow-up PR if needed

This avoids scope creep in PR-B1 while keeping the risk visible.

---

## NEW Nice-to-have

### N-N1 — Document commit dependency graph

**Fix applied in v3**: Added explicit dependency notes under §10.1: `2 → 3`, `2 → 4`, `4 → 5`, `2 → 10`, `4 → 12`, `10 → 12`. Helps the implementer understand commit ordering.

### N-N2 — Q8 i18n parity lint

**Resolution**: Verified existing `scripts/check-wire-error-i18n-coverage.sh` only covers wire-error keys (not general). For general autostart keys (`settings.general.autostart.*`, `onboarding.autostart.*`): manual review only. Adding a general parity script is OUT of PR-B1 scope. If parity drift becomes an issue post-launch, address as separate PR.

### N-N3 — §17.2 PR-A wording clarity

**Fix applied in v3**: Added clarifying sentence to §17 noting that PR-A `feature/phase9-tracking-schedule` (#487) is **already merged** to main; only `fix/phase9-pr-a-followup-cleanup` remains as a separate cleanup branch. Avoids confusing future readers who might think PR-A is still pending.

---

## Q7-Q9 Resolutions

### Q7 — `tauri-plugin-single-instance` D-Bus name source ✅

**Verified via plugin source**:
- `tauri-apps/plugins-workspace v2/plugins/single-instance/src/platform_impl/linux.rs:31-49`
- D-Bus well-known name = `<identifier>.SingleInstance` = **`com.oneshim.client.SingleInstance`**
- Object path = `/com/oneshim/client/SingleInstance` (dots → slash, dashes → underscore)
- Interface = `org.SingleInstance.DBus` (hardcoded)
- No identifier transformation; suffix only
- Optional `semver` feature appends `_<semver-compat-version>` to name (we do NOT enable it)

Documented in §12.2.

### Q8 — i18n key parity CI lint ✅

**Verified via codebase scan**:
- `scripts/check-wire-error-i18n-coverage.sh` — checks ONLY wire-error keys against snapshot registry
- `scripts/check-language.sh` (delegates to `oneshim-lint::language-check`) — checks for non-English text in source code, NOT key parity
- NO general i18n key parity lint exists

Recommendation in §12.2: keep manual review for our keys. Adding general parity script is out of PR-B1 scope.

### Q9 — Reconciler XDG fallback path ✅

**Verified via `autostart.rs:449-457`**:
```rust
pub fn is_enabled() -> Result<bool, String> {
    let svc_path = service_path()?;
    if svc_path.exists() {
        return Ok(true);
    }
    let desk_path = desktop_path()?;
    Ok(desk_path.exists())
}
```

The `is_enabled()` already returns the union of (systemd service file OR XDG desktop file) for Linux. Reconciler in §11.4 calls `autostart::is_autostart_enabled()` which routes to this. No spec change needed.

Documented in §12.2.

---

## Verification of v1 Critical Fixes (carried into v2 → v3)

All v1 Critical issues remain correctly addressed in v3:

- **C1** (ConfigManager API): ✅ `update_with` signature matches actual code
- **C2** (two-identifier design): ✅ §1.2 explanation clean and consistent
- **C3** (two-phase commit removed): ✅ no two-phase paths remain
- **C4** (migration policy): ✅ hash-check + defer-reload prevents both failure modes
- **C5** (Rust-side counter): ✅ frontend round-trip removed, idempotency via session_id

---

## Phase 1 Iter-3 Status

**v3 changes applied** (this iteration):
- §5.1 IPC commands: `IpcError::new` + correct import path + wire code registration note
- §5.5 ShowPromptCoordinator: HMR dev-only quirk documented
- §10.1 commit list: bundle commit 7 into 6, total commits 16 → 15, estimate 23h → 22h, dependency graph documented
- §12.2: Q7-Q9 fully resolved
- §13: Wayland known-limitation explicitly accepted with mitigation path
- §17: PR-A wording clarified
- Header: version bumped to v3, total estimate 38h → 37h, review history updated

**Phase 1 Exit Criteria Check**:
- ✅ All Critical issues fixed (5 from iter-1 + 1 from iter-2 = 6 total, all addressed)
- ✅ All Important issues fixed (8 from iter-1 + 3 from iter-2 = 11 total, all addressed)
- ✅ Q1-Q9 all resolved
- ✅ Spec v3 committed
- ✅ Cross-consumer audit captured

**Recommendation**: Phase 1 complete. Advance to Phase 2 (writing-plans) in next iteration.

If iter-3 verification subagent finds NO new Critical issues, Phase 1 is closed.
