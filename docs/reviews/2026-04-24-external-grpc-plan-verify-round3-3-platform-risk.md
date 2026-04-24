# External gRPC Audit + Live-Config — Plan Verify Round 3 / Platform & Risk Lens

**Reviewer role:** platform / runtime / dependencies / security
**Plan under review:** `docs/reviews/2026-04-24-external-grpc-audit-liveconfig-plan.md` rev-3 (commit `da8b2a26`, 3773 lines)
**Round-1 review:** `docs/reviews/2026-04-24-external-grpc-plan-review-3-platform-risk.md`
**Round-2 review:** `docs/reviews/2026-04-24-external-grpc-plan-verify-round2-3-platform-risk.md`
**Round:** Loop 2 Round 3
**Verdict:** **CONDITIONAL-PASS** — R2 blockers N-C1/N-C2 cleanly resolved. Two new Important issues found (Task 9.0 `cfg_mgr.current()` API that doesn't exist; I4 RAII guard still not shown). Neither is compile-blocking in the hot path (G3 test).

---

## R2 blocker resolution status

### N-C1 — `ConfigManager::new_in_memory(Arc<AppConfig>)` nonexistent → RESOLVED ✅

Rev-3 L3615-3619 now uses real API:

```rust
let tmp = tempfile::NamedTempFile::new().expect("tempfile create");
let cfg_mgr = Arc::new(
    ConfigManager::with_path(tmp.path().to_path_buf())
        .expect("ConfigManager::with_path")
);
```

Cross-verified `config_manager.rs` L45 `pub fn with_path(config_path: PathBuf) -> Result<Self, CoreError>` — signature matches exactly. `.expect()` handles the `Result`. `PathBuf` construction via `tmp.path().to_path_buf()` is correct. **Clean.**

Strategy check: `NamedTempFile::new()` returns a `Result` and the resulting path is a valid file in OS temp dir. `with_path` at L46-58 creates parent dir if missing — `/tmp` (or equivalent) already exists on all CI platforms, so the `fs::create_dir_all` is a no-op. At L60 it checks `config_path.exists()` — `NamedTempFile::new()` creates an empty file, so `load_from_file` will be attempted on a zero-byte file. That will fail JSON parsing, falling through to the "corrupt → defaults" branch (L64-75) which **overwrites** the empty tempfile with default JSON. This side-effect is benign and self-contained. **Acceptable for CI.**

### N-C2 — `update_with` async + `Arc::make_mut` wrong → RESOLVED ✅

Rev-3 L3621-3626 (seed) and L3640-3643 (toggle):

```rust
cfg_mgr.update_with(|c| {
    c.external_grpc.streaming_enabled = Some(false);
    Ok(())
}).expect("update_with apply");
```

All four requirements satisfied:
1. **No `.await`** — sync call matches `fn update_with<F>(&self, F) -> Result<AppConfig, CoreError>` at L139.
2. **Closure takes `&mut AppConfig`** directly — `c.external_grpc...` pattern works, not `Arc::make_mut(c)`.
3. **Returns `Ok(())`** matching `F: FnOnce(&mut AppConfig) -> Result<(), String>` at L141.
4. **Outer wrap `.expect(...)`** consumes the `Result<AppConfig, CoreError>` return. ✅

Zero `Arc::make_mut` calls in the G3 test body. **Clean.**

---

## R2 Important re-verify

### I4 — DeferredGuard RAII pattern → STILL PARTIAL ⚠️

Checked rev-3 Task 3.1 Step 3 (L2661): still says "Increment/decrement `metrics.deferred_audit_in_flight` around spawn body". Cross-reference to spec §5.5 L559-688 remains the plan's delegation. No explicit `struct DeferredGuard(Arc<Metrics>); impl Drop { fn drop(&mut self) { self.0.deferred_audit_in_flight.fetch_sub(1, Relaxed); } }` snippet was added.

**Risk:** If the spec's §5.5 pseudocode places `fetch_add` outside the spawn scope and `fetch_sub` at the scope end (non-RAII), a panic inside the deferred task leaks the gauge, slowly climbing `deferred_audit_in_flight` over the process lifetime. This would surface as a false "audit backlog growing" alarm in ops dashboards.

**Non-blocking.** Retain in Loop 3 impl-review checklist: "grep for `deferred_audit_in_flight.fetch_add` and confirm the matching `fetch_sub` is in a `Drop` impl, not a trailing statement".

---

## New issues found in rev-3

### N-I1 (Important) — Task 9.0 Step 3 uses `cfg_mgr.current()` which does NOT exist

Plan L3526:
```rust
let cfg = cfg_mgr.current();  // Arc<AppConfig>
```

Verified in `crates/oneshim-core/src/config_manager.rs` — public methods are `new`, `with_path`, `get`, `subscribe`, `snapshot`, `update`, `update_with`, `config_path`, `reload`, `config_dir`, `data_dir`. **There is no `current()` method.**

The intended API is **`snapshot() -> Arc<AppConfig>`** (L122). The plan author used the wrong name — likely confusing with `std::sync::RwLock::current()` conventions. The contextual comment `// Arc<AppConfig>` matches `snapshot`'s return type exactly.

**Impact:** Task 9.0 Step 3 pseudocode won't compile as written. Since Task 9.0 Step 3 is presented as a 10-15 LoC scaffold ("implementer inlines 10-15 LoC from existing spawn_server"), the impl-writer can correct this at paste time. But the error is the same API-drift class as R2 N-C1/N-C2 and deserves a pre-impl flag.

**Required fix:** Change L3526 to `let cfg = cfg_mgr.snapshot();` (or `let cfg = cfg_mgr.get();` if `AppConfig` by-value is wanted — but `Arc<AppConfig>` is usually cheaper downstream, so prefer `snapshot`).

**Severity:** Important, not Critical — it's in a scaffold with explicit "implementer inlines" language, not in an inlined test body like the G3 case.

### N-I2 (Important) — Task 9.0 Step 1 `log_event` has `_details` bound to `details` name without use

Plan L3441 `async fn log_event(&self, action_type: &str, session_id: &str, details: &str)` binds `details` but never reads it — only `action_type` is pushed. Under `-D warnings` (clippy line in Task 10.3), this produces an `unused_variables` warning.

**Impact:** `cargo clippy --workspace --all-targets --features ... -- -D warnings` at Task 10.3 will FAIL CI.

**Required fix:** Rename `details` → `_details` at L3441 signature (or actually capture it in `CapturedEntry`). One-character fix. Symmetric with `_level`/`_session_id` already using underscore prefixes.

### N-I3 (Minor) — `tempfile` dev-dep verified

Rev-3 L3679 claims "tempfile is already a dev-dep in `oneshim-web/Cargo.toml` (gated behind `test-support`)". Verified in `crates/oneshim-web/Cargo.toml`:
- L83: `tempfile = { workspace = true, optional = true }` (under `[dependencies]`)
- L126: `test-support = ["dep:tempfile"]` (feature gate)
- L133: `tempfile = { workspace = true }` (under `[dev-dependencies]`)

Double-registered: optional dep + dev-dep. This is fine — the G3 test uses `tempfile` unconditionally (dev-dep path at L133 is what integration tests see). `test-support` feature is for re-exporting into production-path test scaffolding. **No action.**

### N-I4 (Minor — false alarm deflected) — Seed-before-spawn race

Assessed whether the seed `update_with` at L3621 fires a watch change that the reload task misses because the task isn't spawned yet. Sequence:
1. Seed `update_with` → fires `send_replace` to **existing** subscribers (there are none — spawn hasn't happened).
2. `spawn_server_with_config_manager` → inside, `subscribe()` returns receiver with "current value marked unseen" per tokio `watch` semantics.
3. The reload task's first `changed().await` will see the seeded value and apply it as the initial config.

No race. `watch::Sender::send_replace` is NOT latched across late subscribers — but `subscribe()` always returns a receiver initialized to the CURRENT value with `has_changed == true` until first `borrow_and_update` / `changed`. So the reload task's initial iteration applies the seeded config correctly. **No issue.**

### N-I5 (Minor — acknowledged as "implementer inlines") — Task 9.0 Step 3 body absent

Plan L3529 says "... (implementer inlines 10-15 LoC from existing spawn_server)". This is acknowledged scaffolding, not a defect. However, combined with N-I1 (`cfg_mgr.current()`) the implementer must decide two things: (a) which public API to call, (b) how to thread `Arc<ConfigManager>` through `build_external_spawn_config`. Neither is spec-level risk, but Loop 3 subagent review should check both. **Non-blocking.**

---

## CapturingAudit `command_id: String::new()` in `log_event`

Plan L3441-3451 uses `command_id: String::new()` for `log_event` entries because the port trait for `log_event` has no `command_id` parameter (only `action_type, session_id, details`). Downstream filters in §9 assertions query by `command_id` and would skip these empty-command entries.

This is **intended behavior** — `log_event` is for generic/session-level notifications (per the comment at L3442-3443), not per-request audits. Per-request audits use `log_start_if` + `log_complete_with_time` which DO carry `command_id`. **Correct by design; non-issue.**

---

## Verdict

**CONDITIONAL-PASS** — R2 critical findings N-C1 and N-C2 are **cleanly resolved** in rev-3. The G3 convergence test body now compiles against real `ConfigManager` API (`with_path`/`update_with` sync signature). `tempfile` strategy is sound for CI.

Remaining issues (all Important, not Critical):

1. **N-I1**: Task 9.0 Step 3 `cfg_mgr.current()` → rename to `cfg_mgr.snapshot()` (1 char). In a scaffold section with "implementer inlines" disclaimer, but still worth flagging.
2. **N-I2**: Task 9.0 Step 1 `log_event` `details` → `_details` to avoid `-D warnings` CI break. Trivial fix.
3. **I4 (carried over)**: `DeferredGuard` RAII not explicit in Task 3.1 — keep on Loop 3 impl review checklist.

If the plan author lands a trivial rev-3.1 patch changing `cfg_mgr.current()` → `cfg_mgr.snapshot()` and `details` → `_details` at L3441, or commits to doing both at impl-time (with spec acknowledgement), platform/risk PASSES this plan for Loop 3.

**Recommendation:** PASS under the condition that the Loop 3 subagent review rubric explicitly audits:
- Every `ConfigManager` API call site against the real public-method list (`get`/`snapshot`/`subscribe`/`update`/`update_with`/`reload`/`config_path`).
- Every `async fn log_event` signature for unused-variable-warning avoidance under `-D warnings`.
- Every `fetch_add`/`fetch_sub` pair for RAII `Drop` symmetry.

---

**Word count:** ~1200.
**R2 blocking findings resolved:** 2/2 (N-C1 ✅, N-C2 ✅).
**New Important findings:** 2 (N-I1 `cfg_mgr.current()`, N-I2 `log_event` warning).
**Recommended gate:** **CONDITIONAL-PASS** — trivial rev-3.1 fixes OR impl-time corrections with checklist acknowledgment, then proceed to Loop 3.
