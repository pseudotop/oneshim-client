# External gRPC Audit + Live-Config — Plan Review 3 / Platform & Risk Lens

**Reviewer role:** platform / runtime / dependencies / security
**Plan under review:** `docs/reviews/2026-04-24-external-grpc-audit-liveconfig-plan.md` (commit `6bd654ff`, ~3200 lines)
**Spec reference:** `docs/reviews/2026-04-24-external-grpc-audit-liveconfig-spec.md` rev-4 (commit `659bcebd`)
**Round:** Loop 2 Round 1
**Verdict:** **REWORK REQUIRED** — 2 Critical issues (SQL schema mismatch + Mutex API mismatch) that will cause compile + runtime failures; 3 Important issues needing plan amendment.

---

## Critical

### C1 — Task 0.4 SQL targets a non-existent table (`audit_entries` vs. actual `audit_log`)

**Impact:** The SQL in Task 0.4 (plan L502-636) will compile successfully (SQL strings are untyped in rusqlite) but will **fail at runtime** on every call. `entries_by_command_id` will always return `Vec::new()` with a `warn!` log — an infallible contract means silent data loss for the audit-export endpoint (Task 7.2 / spec NV1).

Evidence from the current worktree:
- Actual table name: **`audit_log`**, created in `crates/oneshim-storage/src/migration/v25.rs` (L6: `CREATE TABLE IF NOT EXISTS audit_log`).
- Actual primary-key column: **`entry_id`**, not `id` (v25.rs L73; mod.rs `save_audit_entry` L265-278 confirms).
- Existing indexes: `idx_audit_log_session_id`, `idx_audit_log_timestamp`, `idx_audit_log_action_type`. None on `command_id`.

Plan L511 creates `idx_audit_entries_command_id ON audit_entries (...)` — table does not exist. `CREATE INDEX IF NOT EXISTS ... ON audit_entries` on SQLite will hard-error with `no such table: audit_entries` (the `IF NOT EXISTS` only applies to the index name). Migration V32 will fail at boot, and `CURRENT_VERSION = 32` bump means every existing install crashes on startup.

Plan L607-613 `SELECT id, command_id, session_id, action_type, ...` — column `id` does not exist; runtime `warn!` fires every call.

**Required fix (blocking):** Replace `audit_entries` → `audit_log` and `id` → `entry_id` across plan Task 0.4 (steps 2, 5, and commit body). The SELECT projection must match the exact column list from v25.rs: `entry_id, timestamp, session_id, command_id, action_type, status, details, execution_time_ms`.

---

### C2 — Task 0.4 uses parking_lot `Mutex` API; actual `SqliteStorage.conn` is `std::sync::Mutex`

**Impact:** Won't compile.

Plan L606 `let conn = conn.lock();` — this only compiles against `parking_lot::Mutex`. Verified in `crates/oneshim-storage/src/sqlite/mod.rs` L39: `use std::sync::{Arc, Mutex};` and L82: `pub(super) conn: Arc<Mutex<Connection>>` — std `Mutex::lock()` returns `LockResult<MutexGuard<_>>` which must be unwrapped via `?`, `.expect(...)`, or the existing idiom at L256 `let Ok(conn) = self.conn.lock() else { ... return; }`.

**Required fix:** Use the existing pattern from `save_audit_entry`:

```rust
let Ok(conn) = conn.lock() else {
    tracing::warn!("audit: entries_by_command_id failed to acquire SQLite lock");
    return Vec::new();
};
```

Also confirm `tokio::task::spawn_blocking` is the right idiom here — the existing audit persistence path (`save_audit_entry`) is **synchronous** (no `spawn_blocking`). Adding it only for the read path creates two thread-hop disciplines in one module. Either drop `spawn_blocking` (simpler, matches codebase) or document why reads differ from writes.

---

## Important

### I1 — `Frame::trailers_ref()` only observes **trailer** frames, but the body may emit a data frame containing trailers — verify with a streaming fixture

Confirmed via `http-body-1.0.1/src/frame.rs` L104: `trailers_ref(&self) -> Option<&HeaderMap>`. Only returns `Some` when the frame was constructed via `Frame::trailers(map)` (L28). Data frames return `None`.

The Task 1.3 `poll_frame` check at L1572-1583 is correct for the *final* trailer frame. However, the tests (`captures_permission_denied`, `captures_deadline_exceeded`) use `FixtureBody` with `data: None, trailers: Some(...)` — this is a trailers-only fixture, which in production is actually handled by the **header-first** fast path in `AuditLayer::call` (spec L632-637), not by `poll_frame`. The test fixture does exercise `poll_frame`, but it does so in a trivial configuration — the production-code path that actually polls a trailer frame is the "normal trailers" case (data followed by trailer), which **is** covered by `captures_ok_trailer_fires_some_ok` (plan L1660-1671).

**Action:** No code change needed — the test triplet is adequate. But add a comment to `captures_permission_denied` explaining "this exercises `poll_frame`'s trailer branch; in production the same i32 would arrive via header-first fast path in `AuditLayer::call`." Prevents a future reviewer from deleting the test as "redundant with header-first."

---

### I2 — Migration version bump collides with Phase 9 if Phase 9 also bumps schema

Plan L520 bumps `CURRENT_VERSION: u32 = 32`. Current main is at 31 (verified: `crates/oneshim-storage/src/migration/mod.rs` L36 `CURRENT_VERSION: u32 = 31;`). Plan L74-81 runs `git merge-tree` only against *source files* for conflict detection; the migration file is **new** so `merge-tree` won't flag it — but both branches writing `CURRENT_VERSION = 32` in mod.rs **is** a textual conflict that merge-tree catches. However, if Phase 9's WIP branch bumps V32 then merges to main before this branch, this branch rebases with no file conflict (different migration files) but **same version integer** — silently applying two migrations at V32 is a data-corruption bug.

Per memory `project_next_tasks.md`: Phase 9 Loop 1-2 is committed on `feature/phase9-quick-wins`; Loop 3 impl is pending (3 PRs, ~122h). Phase 9 may land schema changes before this branch merges.

**Action:** Add a bullet to plan §"Phase 9 coexistence guard" (plan L74) — before Task 0.4 commit, run:

```bash
git fetch origin
grep -n "CURRENT_VERSION" $(git ls-tree -r --name-only origin/feature/phase9-quick-wins | grep migration/mod.rs) 2>/dev/null
```

If Phase 9 targets V32+, bump this plan's migration to V33 and reserve V32 via an empty stub. If Phase 9 migration number unknown at plan-consume time, land Task 0.4 early as a reservation commit on a branch merged to main before Phase 9 can grab V32.

---

### I3 — `tokio::task::spawn_blocking(move || { self.conn.lock() })` needs `self.conn.clone()` before the closure, which the plan does — but does not propagate `Arc::clone` explicitly; cosmetic but easy to break on impl

Plan L603-605 `let command_id = command_id.to_string(); let conn = self.conn.clone();` is correct — `Arc::clone` via the `.clone()` method on `Arc<Mutex<Connection>>`. The bound check passes. Minor clarity risk: new readers might misread `self.conn.clone()` as cloning the `Connection` itself (which would fail). Add `Arc::clone(&self.conn)` turbofish form or a comment.

---

### I4 — Per-request `tokio::spawn` for deferred audit is unbounded; `deferred_audit_in_flight` gauge observes but does not enforce a cap

Plan L893-894 shows unit-test increment/decrement in metrics.rs; spec L1324 instruments the gauge; plan Task 3.1 step 3 L2342 says "Increment/decrement `metrics.deferred_audit_in_flight` around spawn body." Good — observability is wired.

Risk: under tonic's `max_concurrent_streams × max_connections` ceiling (default 256 per connection, up to N connections), peak in-flight deferred audit tasks ≈ connections × streams. Each task holds `Arc<AuditBridge>`, `Arc<ExternalMetrics>`, `AuthContext` clone (small), 2× `String` clones (`remote`, `operation`, `request_id`), `Arc<AtomicU64>` (msg_counter), `oneshot::Receiver` — perhaps 500-1000 bytes × N. Not a leak, but unbounded spawn under sustained load is a DoS amplification surface — a single misbehaving client could cause O(max_concurrent_streams × max_connections) pending tasks that never drop until their oneshot fires.

**Action:** Verify that plan Task 3.1 step 3 explicitly places the gauge increment **outside** the `tokio::spawn(async move { ... })` body (in the `call` fn synchronously) and the decrement **inside** the task using an RAII guard — a drop-on-panic-or-cancel Drop guard. The spec §5.5 pseudocode at L645-666 doesn't show this explicitly; the plan defers to it. If the gauge is `fetch_add` inside the spawn body and `fetch_sub` at end, the gauge misses the interval between `tokio::spawn` scheduling and the async block entering. Spec's comment in L1324 says "increment on `tokio::spawn`" — plan should codify this ordering. Recommend:

```rust
metrics.deferred_audit_in_flight.fetch_add(1, Relaxed);
tokio::spawn(async move {
    let _guard = DeferredGuard(metrics.clone());  // Drop decrements
    // ... rest ...
});
```

This survives runtime cancellation and panic. Add this to Task 3.1 step 3 explicitly.

---

## Minor

### M1 — `watch::Receiver::borrow_and_update()` drop-at-semicolon is safe per plan L2019 comment, but worth an invariant comment

Plan L2018: `apply_config(&live, &config_rx.borrow_and_update());` — `Ref<'_, Arc<AppConfig>>` lives for the call; dropped at `;`. No `.await` between borrow and drop: ✅ safe. The explicit comment at L2019 "Ref dropped at end of statement; no await held across borrow." is exactly right.

One latent risk: if `apply_config` is ever made `async`, the `Ref` would need to be awaited across, which `watch::Ref` doesn't support (it's `!Send` on some paths). Add `#[deny(clippy::await_holding_refcell_ref)]` at module or crate level if the crate doesn't already — prevents future refactor hazard.

### M2 — `HeaderValue::from_str(&request_id)` validation overlap with `is_valid` (plan L1286-1288)

`is_valid` rejects bytes outside `0x21..=0x7E`. `HeaderValue::from_str` also rejects controls and `\r\n`. Double validation is fine (defense in depth), but clarify why: `is_valid` is our stricter policy (ASCII-graphic-only, length cap), `HeaderValue::from_str` is the http crate's minimum. Plan Task 1.2 should note this in a code comment.

### M3 — Workspace deps check in plan L1503-1507 references "if not at workspace root" — should verify order-of-operations in Task 0

Verified `http-body 1.0.1`, `http-body-util 0.1.3`, `pin-project-lite 0.2.17` are all present in `Cargo.lock` via transitive tonic-0.14 dep. Plan's fallback-to-add-to-workspace-root is a safety net. Direct imports of `http_body::{Body, Frame}` and `pin_project_lite::pin_project` in a downstream crate should work without a direct dep **only if** the transitive path is visible at name-resolution time. In practice, tonic re-exports `http_body` via `tonic::body::Body: http_body::Body`, but `http_body::Frame` is **not** re-exported. The plan will need to add `http-body` as a direct dep of `oneshim-web` even if `Cargo.lock` shows it — `cargo check` treats transitive deps as private.

**Action:** Make plan L1492-1501 stronger — don't say "verify via cargo tree," say "add `http-body = "1"` and `pin-project-lite = "0.2"` directly to `oneshim-web/Cargo.toml` regardless; `http-body-util` stays as dev-dep only."

### M4 — Test fixture `FixtureBody` redefines `http::HeaderValue::from(i32)` unchecked (plan L1654)

`HeaderValue::from(i32)` does exist (via `impl From<i16/i32/i64/...>`) and yields the ASCII-decimal representation — safe. No action needed but worth a mental note that this works only for integer status codes. Non-integer scenarios need `from_static` or `from_str`.

### M5 — `limit.min(1000)` DoS cap (Task 7.2) acceptable for loopback-only endpoint

Per spec, `/api/audit/export` is served by the web dashboard which is loopback-only. 2KB × 1000 = 2MB max response. Acceptable. No change.

---

## Strengths

- **Header-first grpc-status observation (D28)** — the plan correctly handles tonic 0.14.5's trailers-only response contract (verified against `tonic-0.14.5/src/status.rs` L605-613 reference in spec). The decision to use a conditional wrapper (`new_already_fired` for trailers-only vs. `new` for streaming/normal) avoids double-fire hazards.
- **PinnedDrop via pin-project-lite** — pin-project-lite 0.2.17's `PinnedDrop` macro produces the exact `impl<B> PinnedDrop for TrailerCapturingBody<B> { fn drop(this: Pin<&mut Self>) { ... } }` signature expected. `this.project()` on drop is the documented pattern; no subtle pinning issue.
- **Single snapshot per Debug print** (spec §5.6 L716-719, plan Task 4.1 L2425) — prevents cross-field torn reads for Debug output. Good defensive engineering.
- **`oneshot::channel::<Option<tonic::Code>>`** — the triple-valued semantic (fired+Some, fired+None, never-fired) is well-handled: Drop always fires (best-effort); header-first pre-fires; poll_frame fires-and-takes. The `Option` inside `Option` (i.e., `Ok(Some(code)) / Ok(None) / Err(_)`) maps cleanly to `map_code_to_audit_status`.
- **Deferred task captures are all `Clone`-able, all owned pre-.await** — spec L578 "Crucial: the deferred task holds captured clones; it does not borrow from the parent scope" — verified in pseudocode L585-597: `bridge.clone()`, `metrics.clone()`, `auth_ctx.cloned()`, `peer.cloned()`, `operation.to_string()`, `request_id.clone()`. No sync borrow held across `.await`.
- **Parameterized SQL** — Task 0.4 `WHERE command_id = ?1` via `rusqlite::params![&command_id, limit as i64]` — safe from injection.
- **Workspace dep sanity** — `arc-swap = "1"`, `uuid = "1"`, `tonic = "0.14"`, `tower = "0.5"`, `tonic-prost = "0.14"` all at workspace root; reproducible.
- **Tonic 0.14.5 `Server::layer`** (verified `tonic-0.14.5/src/transport/server/mod.rs` L624) is generic over any `NewLayer` with validation deferred to `add_service`. The plan's `impl<S: Clone> Layer<S>` bound + `impl<S, B, RespBody> Service<...> for RequestIdService<S> where S: Service<...Error = Infallible> + Clone + Send + 'static` composes correctly.

---

## Questions

### Q1 — Does `TrailerCapturingBody` satisfy tonic 0.14's body bound on the `Response<T>` that `add_service` emits?

Plan L1623 attempts a compile-time assertion but the `assert_body::<TrailerCapturingBody<...>>` call is commented out ("We can't use tonic::body::Body directly in tests"). The spec at L685-691 shows the stricter `const _: fn() = ...` assertion:

```rust
const _: fn() = || {
    fn assert_body<T: http_body::Body + Send + 'static>() {}
    assert_body::<TrailerCapturingBody<tonic::body::Body>>();
};
```

**Ask:** Plan Task 1.3 step 2 code (L1622-1628) should include the concrete assertion `assert_body::<TrailerCapturingBody<tonic::body::Body>>()` — not the vacuous generic one. Without it, the type-compatibility check only runs when integration tests exercise the full stack. Land a small compile-time proof in the unit test module.

### Q2 — `tokio::spawn` failure on runtime-shutdown vs. the body's `Drop`

If the tokio runtime is shutting down (app exit), the spawned deferred task may never run — `rx.await` completes (Drop on the body fires tx), but the task itself is cancelled before reaching `bridge.record_completion`. Spec L1291 acknowledges: "if the tokio runtime cancels the spawned audit task … the body's Drop still attempts `tx.send(captured)`, which returns Err silently. No audit row written — but this matches existing behavior." Confirmed acceptable.

**Ask:** Plan Task 3.1 should add a unit test for the "runtime shutdown drops deferred audit row" scenario — a `tokio::runtime::Runtime::drop` before the task runs. Currently the plan does not test this path; the user-visible failure mode is "missing completed row for an auth-OK request during graceful shutdown."

### Q3 — `self.conn.clone()` Mutex contention with the write path

`save_audit_entry` holds the Mutex synchronously during INSERT. `entries_by_command_id` holds it during SELECT. Under load (many deferred audit writes + a polling dashboard read), the single-Mutex-per-DB policy causes serialization. SQLite WAL mode makes this mostly benign (writer doesn't block readers at the SQL layer), but the `std::sync::Mutex<Connection>` wraps a single connection — all traffic funnels through one lock. Not a blocker for this plan (it's pre-existing) but worth noting in Task 0.4 as "read-path inherits existing lock-contention profile."

---

**Word count:** ~1950 words.
**Blocking findings:** C1 (SQL schema mismatch), C2 (Mutex API mismatch) — both will fail at `cargo test` time.
**Recommended gate:** REWORK, then re-review before Loop 3 plan sign-off.
