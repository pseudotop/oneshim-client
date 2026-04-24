# Phase 9 Plan — Review Synthesis (Loop 2c → 2d)

**Date**: 2026-04-24
**Plan under review**: `docs/reviews/2026-04-23-phase9-quick-wins-plan.md` @ 1353 lines
**Inputs**: R1 (587L, 2C/7I/8M) + R2 (362L, 2C/6I/7M) + R3 (338L, 3C/8I/9M) = **44 findings** raw
**Consolidated**: **32 findings** after dedup (6 Critical, 14 Important, 12 Minor)
**Disagreements**: **2 items** (Q-plan-2 sub-module extraction; effort-estimate resize)
**User-decision blockers**: **3 items** (all low-ambiguity — recommend a resolution)
**Fix-plan length**: **26 ordered steps**
**Gate**: **NOT READY** for Loop 3 impl — zero-Critical gate fails on 6 items.

All file:line references assume worktree tip `5618558c`. Spec is locked (Loop 1e PASS on all three axes) — no Decision reopening. Fixes are plan-level only.

---

## 0. Reading key

| Marker | Meaning |
|--------|---------|
| `CONS-PCxx` | Consolidated Critical finding at plan level (must fix) |
| `CONS-PIxx` | Consolidated Important finding at plan level (must fix) |
| `CONS-PMxx` | Consolidated Minor finding at plan level (can defer) |
| `🚨 DISAGREEMENT` | Reviewers contradict; user decision or tie-break needed |
| `⚠ USER-INPUT` | Requires user decision (rare at plan level) |
| `✅ Verified` | Evidence check confirms reviewer claim |
| `❌ Rebutted` | Evidence check failed (none this round) |

---

## 1. Consolidated Critical (6 — zero-gate blockers)

### CONS-PC01. Composition-root file paths wrong — `SmartCaptureTrigger::with_schedule` + `BatchUploader::new` live in `agent_runtime_support.rs`, not `main.rs` / `app_runtime_launch.rs`

- **Sources**: R1.C1 (primary) + R3.I3 (complementary)
- **Severity**: Critical (R1 call; R3 classified as Important but same evidence)
- **Plan sections affected**: §3.3 A.7 (line 244), §3.3 A.12 (line 344), §3.3 A.9 (audio gate — same pattern)
- **Evidence** (✅ Verified):
  ```
  $ grep -rn 'SmartCaptureTrigger::with_schedule\|BatchUploader::new' src-tauri/src/
  src-tauri/src/agent_runtime_support.rs:251:  Arc::new(SmartCaptureTrigger::with_schedule(
  src-tauri/src/agent_runtime_support.rs:405:  let batch_uploader = Arc::new(BatchUploader::new(
  ```
  Zero hits in `main.rs`, `app_runtime_launch.rs`, or any other file.
- **Impact**: Implementer follows plan verbatim, greps the named files, finds nothing, wastes time or makes wrong edits. Worse: A.7 hoist + A.12 DI wiring both touch the **same file** (`agent_runtime_support.rs`) — plan should bundle or cross-reference to save one cold-clippy cycle.
- **Fix**: Replace both citations with `src-tauri/src/agent_runtime_support.rs:251` (A.7) and `src-tauri/src/agent_runtime_support.rs:405` (A.12). Add explicit note: "A.7 and A.12 touch the same file — bundle to share cold-clippy cycle."
- **Dependency**: none (mechanical edit).

### CONS-PC02. `capture_permitted_now` helper drops `consent_granted` and `!capture_paused` — GDPR transparency guarantee weakened

- **Sources**: R2.C1 (primary; unique)
- **Severity**: Critical
- **Plan sections affected**: §3.3 A.4 (line 180, helper signature), A.5 (line 198, impl), A.7 (line 246, monitor use-site), A.9 (lines 282-303, 9 new gate sites)
- **Spec anchor**: §3.4 composition rule + §3.4b consent-top-authority table. The 4-term composition is:
  ```
  capture_allowed(now, tier) = consent_granted(tier)
                            AND active_hours_gate(now)
                            AND NOT tracking_schedule_active(now)
                            AND NOT capture_paused
  ```
- **Evidence** (✅ Verified):
  - Plan A.5 line 198 ships `capture_permitted_now := should_run_now_with_time(cfg, now) && !tracking_schedule_active(cfg, now)` — 2 of 4 gates.
  - Plan A.7 line 246 hoists the pre-gate `save_event`/`uploader.enqueue` calls inside `if capture_permitted_now && !capture_paused` — partially fixes it at the **monitor call site only**.
  - Plan A.9 line 293 uses `if crate::scheduler::tracking_schedule_active(&cfg.get()) || !crate::scheduler::should_run_now(&cfg.get()) { continue; }` at 9 new gate sites (analysis/focus/coaching/process/input/clipboard/file-access/cross-device-sync/audio). This is `!TS || !active_hours` — **omits both consent and capture_paused**.
- **Product impact**: User toggles "pause capture" from tray → 9 of 13 newly gated pipelines keep running. Analysis/focus/coaching loops continue ticking on last events. GDPR Art. 13/14 transparency silently violated.
- **Consent impact**: If consent is revoked mid-session, the same 9 pipelines keep running because none consult `ConsentManager` (`crates/oneshim-core/src/consent.rs:102`).
- **Rollback impact**: If a reviewer catches the consent gap post-merge, the fastest revert (A.9) loses the D13 scope-expansion gains — a bad trade.
- **Fix**:
  1. Redefine `capture_permitted_now(cfg, consent, now) -> bool` in A.4/A.5 to compose all 4 terms.
  2. Thread `Arc<ConsentManager>` (or `ConsentPermissions` snapshot) through to the scheduler helper or expose via `AppConfig`.
  3. Rewrite A.9 to call the full composite gate — replace `tracking_schedule_active || !should_run_now` at all 9 sites.
  4. Add `consent_revoked_top_authority_overrides_ts_inactive` test to A.8 and extend the A.4 truth-table to 4-term composition.
- **Dependency**: Resolves simultaneously with CONS-PC04 (audio signature) since audio gate needs the same composite. Drives most of CONS-PI-series (I3 PartialEq, I4 sync→async, etc. do not conflict).

### CONS-PC03. PR-A test-count inconsistency — 28 vs 42 vs 56 disagreement across §3.3/§6.2/§7.2

- **Sources**: R2.C2 (primary; unique); R1.M2 (arithmetic note on PR-B), R3.M7 (minor drift acknowledgement)
- **Severity**: Critical (R2 call — STATUS.md is single source of truth; drift blocks PR review gates)
- **Plan sections affected**: §3.3 A.21 line 483, §3.5 line 518, §6.2 line 998, §7.2 table line 1079-1082
- **Evidence** (✅ Verified):
  - §3.3 A.21 line 483: "PR-A adds ~28 new tests" with breakdown `8 + 9 + 3 + 13 + 3 + 5 + 4 + 4 + 7 = 56`. Prose says 28; itemization sums to 56.
  - §3.5 line 518: "PR-A adds ~28 new tests" (same stale figure).
  - §6.2 line 998: "PR-A adds ~28; ... Total Phase 9: ~75 new tests".
  - §7.2 line 1079: PR-A = `22 + 13 + 7 + 0 = 42`; Grand total `42 + 21 + 27 = 90`.
- **Impact**:
  1. `docs/STATUS.md` test-count bump cannot be committed correctly — the plan gives no authoritative number.
  2. PR review gating on "all tests pass" is meaningless if the expected count is ambiguous.
  3. CONS-C11 (Loop 1) already burned on test-count drift — plan regresses.
- **Fix**:
  1. Pick the authoritative count. §7.2 table itemization (PR-A = 42) is the most scrutinised; recommend adopt as source of truth.
  2. Update §3.3 A.21 line 483 to match (or sum from the §3.3 breakdown explicitly).
  3. Update §3.5 line 518 and §6.2 line 998 to `~42`; Phase 9 total becomes `~90`.
  4. If R2.I04's per-variant split of A.8 sanity tests is accepted, bump A.8 from 13 → 18 and PR-A from 42 → 47; propagate.
  5. Add a final-checks line: "All four test-count mentions consistent with §7.2 table".
- **Dependency**: CONS-PI05 (A.8 sanity-test split) may modify the final number; resolve order: decide CONS-PI05 → fix CONS-PC03.

### CONS-PC04. `start_audio_capture` signature change not spelled out — audio gating requires new state param

- **Sources**: R1.C2 (primary)
- **Severity**: Critical
- **Plan sections affected**: §3.3 A.9 audio-command bullet (line 297-303)
- **Evidence** (✅ Verified):
  ```rust
  // src-tauri/src/commands/audio.rs:19-21 (actual)
  pub async fn start_audio_capture(
      state: tauri::State<'_, AudioRuntimeState>,
  ) -> Result<(), IpcError>
  ```
  Plan proposes `if crate::scheduler::tracking_schedule_active(&config_state.get()) { ... }` — `config_state` does not exist in this signature.
- **Impact**: Implementer has no guidance on state-injection path (inject `State<'_, ConfigRuntimeState>`, or give `AudioRuntimeState` a `ConfigManager` field, or route via app-handle lookup). Each path has architectural trade-offs.
- **Fix**: Explicitly specify path (a) — add second `State<'_, ConfigRuntimeState>` param:
  ```rust
  pub async fn start_audio_capture(
      audio_state: tauri::State<'_, AudioRuntimeState>,
      config_state: tauri::State<'_, ConfigRuntimeState>,
  ) -> Result<(), IpcError>
  ```
  Verify Tauri `generate_handler!` registration accommodates multi-state (it does; precedent in `commands::settings::update_setting`). If CONS-PC02 adopts full 4-term composite, also thread `ConsentRuntimeState` or equivalent.
- **Dependency**: resolves with CONS-PC02 (audio gate must use the full composite).

### CONS-PC05. CI contract-drift gate cited incorrectly — `./scripts/verify-integrity.sh` is a supply-chain check, not a manifest/OpenAPI check

- **Sources**: R3.C1 (primary; unique)
- **Severity**: Critical
- **Plan sections affected**: §3.5 line 515, §4.5 line 735, §5.5 line 952, §6.5 line 1057, §8.2 line 1120, §9.6 — **five distinct citations**
- **Evidence** (✅ Verified):
  ```
  $ head -50 scripts/verify-integrity.sh
  # runs cargo-audit, cargo-deny, cargo-vet, cargo-cyclonedx SBOM —
  # a security / supply-chain gate, NOT the manifest/OpenAPI gate.

  $ ls scripts/ | grep -E 'integrity|manifest|openapi'
  generate-http-openapi.sh
  verify-http-interface-manifest.sh
  verify-http-openapi-sync.sh
  verify-integrity.sh              # <-- supply-chain
  verify-route-integrity.sh
  ```
- **Impact**: Engineer following plan's local gate runs the security gate (may require `cargo-audit`/`cargo-deny`/etc. pre-installed) and still merges with OpenAPI drift. Actual contract-drift surfaces silently in `ci.yml` `check` job (lines 192-199), wasting a ~28-min CI cycle.
- **Fix**: Replace all five `./scripts/verify-integrity.sh` citations with the explicit pair:
  ```
  ./scripts/generate-http-openapi.sh docs/contracts/oneshim-web.v1.openapi.yaml \
    && ./scripts/verify-http-interface-manifest.sh \
    && ./scripts/verify-http-openapi-sync.sh
  ```
  Update §8.2 header to reference the real contract gates (not `integrity-gates.yml`).
- **Dependency**: CONS-PC06 (OpenAPI auto-generation) — fix together.

### CONS-PC06. OpenAPI snapshot is auto-generated, not hand-maintained — plan's "hand-patch" instructions will fail CI

- **Sources**: R3.C2 (primary; unique)
- **Severity**: Critical
- **Plan sections affected**: §3.3 A.21 line 481, §5.3 C.10 line 929, §4.3 B.10 line 697, §6.5 line 1057
- **Evidence** (✅ Verified):
  ```
  $ head -70 scripts/generate-http-openapi.sh
  # reads docs/contracts/http-interface-manifest.v1.json
  # → generates docs/contracts/oneshim-web.v1.openapi.yaml

  $ head -15 scripts/verify-http-openapi-sync.sh   # (implied; CI ci.yml:192-199)
  # regenerates to tmp + diff -u against tracked file; fail on drift.
  ```
- **Impact**: If engineers hand-patch `oneshim-web.v1.openapi.yaml`, CI `check` job fails with `snapshot drift detected`. Plan wastes a ~28-min CI cycle per PR. Plan inherits this misunderstanding from spec §6.5 (R3.M2 notes to file a spec follow-up after plan fix).
- **Fix**: For A.21, B.10, C.10 rewording:
  1. Edit `docs/contracts/http-interface-manifest.v1.json` (HAND-MAINTAINED; no generator).
  2. Run `./scripts/generate-http-openapi.sh docs/contracts/oneshim-web.v1.openapi.yaml` to regenerate the snapshot.
  3. Run `./scripts/verify-http-openapi-sync.sh` to confirm no drift.
  Confirm in §8.2 CI impact section that hand-editing the generated YAML will fail the `check` job.
- **Dependency**: Resolve together with CONS-PC05.

---

## 2. Consolidated Important (14 — zero-Important gate blockers)

### CONS-PI01. `autostart::enable_autostart()` sync → async signature change not called out

- **Sources**: R1.I4 (primary) + R3.I1 (complementary — picks up same issue)
- **Severity**: Important (both reviewers concur)
- **Plan sections affected**: §4.3 B.3 line 595-597 (timeout wrap)
- **Evidence** (✅ Verified): `autostart.rs:8,31,54` declare `pub fn enable_autostart() -> Result<(), String>` — 100% sync. Wrapping in `tokio::time::timeout` requires async context. B.5's IPC commands are already async; cleanest path is making the public fns `async fn`.
- **Fix**: B.3 must explicitly list `pub fn` → `pub async fn` for `enable_autostart`, `disable_autostart`, `is_autostart_enabled`. Existing in-module tests (`enable_disable_roundtrip_unsupported_platform`) gain `#[tokio::test]` attribute. Zero production call sites outside the new IPC commands (verified).

### CONS-PI02. Windows `RegSetValueExW` is synchronous — `tokio::time::timeout` wrap is vestigial

- **Sources**: R3.I2 (primary; unique)
- **Severity**: Important
- **Plan section affected**: §4.3 B.3 line 597
- **Evidence** (✅ Verified): `autostart.rs:200-227` is a single unsafe Win32 `RegSetValueExW` call — no process spawn.
- **Fix**: Keep Windows `enable`/`disable`/`is_enabled` synchronous; skip `spawn_blocking + tokio::time::timeout` wrap for Windows. Per-platform dispatch: Linux/macOS use async; Windows stays sync (or async with `spawn_blocking` but no timeout). State the Windows-specific exception explicitly in B.3.

### CONS-PI03. Windows + macOS autostart tests never run in CI — plan silently accepts the gap

- **Sources**: R3.C3 (originally Critical, demoted to Important here — it's a **pre-existing** CI coverage gap, not a plan-authored regression)
- **Severity**: Important (dimensional downgrade: plan inherits an existing CI limitation rather than introducing one)
- **Plan sections affected**: §4.3 B.1 + B.4 (`#[cfg(target_os = "windows")]` tests), §8.1 "None new" claim (line 1112)
- **Evidence** (✅ Verified):
  ```
  $ grep -n 'runs-on\|matrix' .github/workflows/ci.yml | head -10
  22:    runs-on: ubuntu-latest
  47:    runs-on: ubuntu-latest
  142:   runs-on: ubuntu-latest
  259:   runs-on: ubuntu-latest
  314:   runs-on: ubuntu-latest                    # <-- test job: Linux only
  424:   runs-on: ${{ matrix.os }}                 # <-- build job: has matrix
  ```
  `build` job has 4-platform matrix but only `cargo build`, not `cargo test`.
- **Impact**: `windows_enable_returns_err_on_regsetvalueexw_nonzero` (B.1) and `get_status_returns_mechanism_per_platform` macOS/Windows branches (B.4) will never run in merge gates.
- **Fix**: Option 1 (recommended): add §8.7 "Platform coverage gap" explicitly stating PR-B platform-branched tests are Linux-only in CI; developer must run `cargo test` locally on macOS + Windows before merge. Add follow-up TODO to wire macOS/Windows test runners. Option 2 (more work): duplicate build matrix into test job — adds ~30 min per push; recommend for follow-up PR.
- **Note**: R3 argued Critical; demoted on synthesis because:
  1. The gap is pre-existing and structurally independent of Phase 9.
  2. Plan §4.8 already documents Linux as primary target; macOS/Windows fall-through is testable locally.
  3. Demotion is recorded; user may override — see §5 user-decision blockers.

### CONS-PI04. Pre-flush drain at window-entry (spec §3.9 clause 4) silently dropped from plan

- **Sources**: R2.I01 (primary; unique)
- **Severity**: Important
- **Plan sections affected**: §3.3 A.11 / A.12 (suppression predicate); §3.4 follow-up bullet list (line 498)
- **Evidence** (✅ Verified): `grep -n 'pre-flush drain\|on_window_boundary_approaching\|long-window overflow' docs/reviews/2026-04-23-phase9-quick-wins-plan.md` → zero hits.
- **Impact**: Long TS windows (e.g., 10h overnight) may exceed uploader `max_queue_size`; `drop_oldest()` silently drops pre-window events. Spec §3.9 labels this optional (deferrable), but plan neither defers explicitly nor implements.
- **Fix**: Add explicit deferral to §3.4 "PR-A cross-cutting docs / Follow-up TODOs registered" bullet list (line 498): "`on_window_boundary_approaching` pre-flush drain for long TS windows (spec §3.9 clause 4, deferred per complexity)".
- **Dependency**: none.

### CONS-PI05. A.8 `ts_inactive_allows_events` single sanity test is too weak — 5-variant coverage blind spot

- **Sources**: R2.I04 (primary; unique)
- **Severity**: Important
- **Plan section affected**: §3.3 A.8 line 271
- **Evidence**: Plan has 5 per-variant suppression tests but one catch-all sanity test. A broken gate over-suppressing only one variant (e.g., only Window) passes "events arrive" sanity assertion.
- **Fix**: Split into 5 per-variant sanity tests — `ts_inactive_allows_window_events`, `..._process_...`, `..._input_...`, `..._clipboard_...`, `..._file_access_...`. Each asserts `COUNT(*) > 0` for its variant. A.8 test count: 13 → 18. Flows to CONS-PC03 re-reconciliation.

### CONS-PI06. A.2 test set missing serde validation edge cases (invalid IANA, empty end, end-before-start, malformed HH:MM)

- **Sources**: R2.I03 (primary; unique)
- **Severity**: Important
- **Plan section affected**: §3.3 A.2 (line 142-149)
- **Evidence**: Plan has `empty_days_never_active` + DST tests but no `serde_rejects_invalid_hhmm`, `serde_rejects_invalid_iana_timezone`, `window_with_empty_end_is_invalid`, `window_end_before_start_not_same_day_is_invalid`.
- **Fix**: Add 4 validation tests to A.2. Spec §6.3 error codes `config.invalid` (IANA) and `validation.invalid_field` (empty/malformed) correspond. A.2 test count: 8 → 12.

### CONS-PI07. Clock-irregularity test coverage gap (spec §3.7a) — suspend-across-window + forward-jump cases untested

- **Sources**: R2.I02 (primary; unique)
- **Severity**: Important
- **Plan sections affected**: §3.3 A.4 (line 182-192) + A.18 (line 442-446)
- **Evidence**: Plan's `notifier_debounces_within_60s` covers backward-jump; spec §3.7a enumerates 5 rows but only 1 tested.
- **Fix**: Add 3 pure-fn tests to A.4 (all composable without mock clock):
  1. `window_active_across_suspend` — `tracking_schedule_active` returns correct value regardless of tick between suspend/resume.
  2. `forward_clock_jump_into_future_window` — returns true after jump.
  3. `forward_clock_jump_past_window_end` — returns false after jump.

### CONS-PI08. `AutostartStatus.needs_repair` field under-specified — frontend test assumes field the contract doesn't declare

- **Sources**: R2.I05 (primary; unique)
- **Severity**: Important
- **Plan sections affected**: §4.3 B.5 line 641 (struct def), §4.3 B.8 line 682 (frontend test stub)
- **Evidence**: B.5 declares `pub struct AutostartStatus { enabled: bool, mechanism: String, fallback_used: bool }` — no `needs_repair`. B.8 stubs `needs_repair: true extension flag (reuse existing flags if any; otherwise add a simple stale-path detection)`.
- **Fix**:
  1. Add `pub needs_repair: bool` to `AutostartStatus` in B.5 line 641.
  2. Define computation: `is_autostart_enabled() && !recorded_path_matches_current_exe()`. On Linux check `ExecStart=` line; on macOS parse plist; on Windows read registry value.
  3. B.4/B.6 gain test `needs_repair_true_when_recorded_path_differs_from_current_exe`.

### CONS-PI09. D15 frontend error-path under-tested — one test for highest-coordination-risk behavior change

- **Sources**: R2.I06 (primary; unique)
- **Severity**: Important
- **Plan section affected**: §5.3 C.4 line 857 (TimelineLayout.tsx consumer) + C.7 tests
- **Evidence**: One frontend test covers silent-200 → explicit-500 behavior change, despite being flagged as highest-coordination risk in plan §1 and §5.3.
- **Fix**: Add to C.7 (or extend C.4):
  1. Mutation returns 500 `storage.failed` → `onError` fires, toast shows localized `timeline.batchTagError`.
  2. Mutation returns 500 `validation.invalid_arguments` (batch > 1000) → different toast.
  3. Mutation succeeds with `affected_count: 0` → success toast "0 frames newly tagged" (not error).
  4. Playwright E2E "select 5 frames, forced-error → 500 → toast → selection behavior" in C.9.

### CONS-PI10. Frontend `tagged_count` rename incomplete — `TimelineLayout.tsx:49` type-alias missed

- **Sources**: R3.I4 (primary; unique)
- **Severity**: Important (R3 call; would cause `pnpm tsc` failure but easily caught)
- **Plan section affected**: §5.3 C.4 line 857
- **Evidence** (✅ Verified): `grep -n 'tagged_count' TimelineLayout.tsx` → line 49 (useMutation generic type alias) + line 135 (consumer). Plan cites range `131-140` which covers 135 but misses 49.
- **Fix**: C.4 must cite `TimelineLayout.tsx:49` (type alias) AND `TimelineLayout.tsx:135` (use site). Add pre-commit verification: `grep -rn 'tagged_count' crates/oneshim-web/frontend/src/` returns 0 hits post-refactor.

### CONS-PI11. `ALLOWED_KEYS` edit breaks snapshot test — plan omits test update

- **Sources**: R1.I1 (primary; unique)
- **Severity**: Important
- **Plan section affected**: §3.3 A.14 line 378
- **Evidence**: `src-tauri/src/commands/settings.rs:44` is where plan adds `"tracking_schedule"`; plan says line 44 but file structure shows `"notification"` on line 44 (insertion point between `"coaching"` line 54 and closing `]` line 55). Companion snapshot test at `:343-361` (`allowed_keys_matches_expected_set`) asserts exact array equality.
- **Fix**: A.14 must list both edits — `ALLOWED_KEYS` array + snapshot test update. Correct line citation to ~54-55 (between `"coaching"` and closing `]`). Verify sibling test `allowed_keys_excludes_sensitive_sections` at `:363` does NOT list `tracking_schedule` as forbidden.

### CONS-PI12. A.17 tray-task spawn wording misleading — `tray.rs` has no existing async task

- **Sources**: R1.I2 (primary; unique)
- **Severity**: Important
- **Plan section affected**: §3.3 A.17 line 413-432
- **Evidence** (✅ Verified): `tray.rs` contains **zero** `tokio::spawn` calls. `setup_tray<R: Runtime>` is sync; `sync_tray_state` is a sync callback from `on_menu_event`.
- **Fix**: Clarify A.17 — creates a **new** async worker spawned from the composition root (likely `agent_runtime_support.rs` per CONS-PC01) after `setup_tray` returns, keyed on an `AppHandle` clone for emit/icon calls. Document task lifetime (teardown on app shutdown; JoinHandle ownership).

### CONS-PI13. `ConfigManager::get()` deep-clones per call — predicate closure + A.9 gate sites have hot-path cost

- **Sources**: R1.I7 (primary; unique)
- **Severity**: Important
- **Plan sections affected**: §3.3 A.12 predicate closure; §3.3 A.9 all gate sites
- **Evidence** (✅ Verified): `config_manager.rs:97-99`:
  ```rust
  pub fn get(&self) -> AppConfig {
      AppConfig::clone(&self.inner.sender.borrow())
  }
  ```
  Deep-clones entire `AppConfig` (37 sections). `ConfigManager::snapshot() -> Arc<AppConfig>` at `:122-124` is O(1) Arc-clone.
- **Impact**: Per-flush cost tolerable at 1s monitor-loop cadence; at 100ms input_interval cadence (spec §3.8 row 8) the deep clone is a measurable regression. Plan's "O(1) early-return when disabled" only holds after the clone.
- **Fix**: Change predicate closure and helper wrapper to use `snapshot()`:
  ```rust
  let pred: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || {
      crate::scheduler::tracking_schedule_active(&cfg_mgr_for_pred.snapshot())
  });
  ```
  `tracking_schedule_active` accepts `&Arc<AppConfig>` or `&AppConfig` (deref-coerces). Update all A.9 gate sites `&cm.get()` → `&cm.snapshot()`.

### CONS-PI14. Observability (err.code, tracing spans, counters, audit log) not specified for new surfaces

- **Sources**: R3.I7 (primary; unique)
- **Severity**: Important
- **Plan sections affected**: §3 through §5 (new IPC/REST/helper surfaces)
- **Evidence**: Plan adds new `BatchUploader` suppression events, new `DesktopNotifier` fires, new IPC commands, new REST handlers, new `tracking_schedule_active` gate enter/exit — zero observability specification.
- **Fix**: Add §6.6 "Observability" section enumerating:
  1. Tracing spans: `tracking_schedule_active(enter/exit)`, `autostart_enable/disable/status`, `bulk_tag_transaction(add/remove)`.
  2. Counters (format-bikeshed OK): `oneshim_tracking_schedule_state{active:bool}`, `oneshim_bulk_tag_operations_total{op:add|remove, result:ok|err}`, `oneshim_autostart_attempt_total{result, mechanism}`.
  3. `err.code` fields on all new `warn!`/`error!` sites (CLAUDE.md convention).
  4. Audit entry `TrackingScheduleTransition{prev, now, reason: "user" | "scheduled"}` called from A.18.

---

## 3. Consolidated Minor (12 — non-blocking polish)

### CONS-PM01. `watch::Receiver` latest-wins coalescence may miss notifier transitions (A.18)
- **Source**: R3.I6 (originally Important; demoted — spec §3.7a explicitly accepts some clock-irregularity transitions as "missed")
- Plan A.18 uses `subscribe` for prev→now transition detection; rapid enable→disable→enable mutations may coalesce. Fix: either switch to tick-based poll or document as accepted non-goal. Add row to §3.8 risk register.

### CONS-PM02. PartialEq derive ripple-effect not audited on `NotificationConfig` (A.17)
- **Source**: R1.I3 + R3.I5 (both flag)
- Plan says "add PartialEq if missing" without cross-consumer audit. Fix: pre-commit `grep -rn 'NotificationConfig ==\|NotificationConfig !='` to confirm zero consumers. Additive derive is safe if zero hits.

### CONS-PM03. Line-number drift in multiple citations
- **Source**: R1.M1 (TimelineLayout.tsx:131-140 → actual 130-138), R1.I1 line-off-by-10, R1.E (settings.rs:44)
- Fix: Plan drafter re-verify all file:line citations via `grep -n` before finalizing.

### CONS-PM04. TDD ordering prose inconsistency (A.1 is dep-bump, not TDD)
- **Source**: R1.I5 (originally Important; demoted — cosmetic documentation issue)
- Plan §3.3 opens with "Test-first for every impl commit" but A.1 is impl-first. Fix: §3.3 prose notes A.1 as prerequisite commit; A.5 resolves two red gates (A.2 + A.4); A.12 gets one micro-test for predicate-closure hookup.

### CONS-PM05. Intra-PR-A dep graph missing
- **Source**: R1.I6 (originally Important; demoted — plan narrative implies ordering; graph is docs enhancement)
- Fix: Add explicit dependency graph to §3.3 prose or as ASCII figure:
  ```
  A.1 → A.3 → A.5 → A.7 → A.9 → A.17
           ↘ A.4 ↗       ↗
                     A.11 → A.12
  A.13 → A.14 ; A.15 → A.16 ; A.18 (uses A.5) ; A.19 → A.20 (uses A.14 + A.16)
  ```

### CONS-PM06. Commit-count table arithmetic drift (§9.4 PR-B)
- **Source**: R1.M2
- §9.4 says PR-B = 11 commits, 5 test column, but B.1/B.2/B.4/B.6/B.8/B.11 = 6 test commits. Fix arithmetic.

### CONS-PM07. Follow-up registration location ambiguous
- **Source**: R2.M01
- §3.4 line 498 says "project_next_tasks.md OR follow-ups.md — verify". MEMORY.md confirms `project_next_tasks.md` exists. Commit to that file and remove either/or.

### CONS-PM08. Korean "추적 일정" enforcement is manual grep only (A.20)
- **Source**: R2.M02
- Minor; grep check acceptable for quick-wins but may be added to CI later.

### CONS-PM09. `heartbeat_loop_continues_during_ts` sanity tests don't cover all ungated loops
- **Source**: R2.M03
- §3.8 has 16 rows; ungated rows 14-16 covered partially (heartbeat + OAuth). Consider adding `metrics_loop_continues_during_ts`. Low priority.

### CONS-PM10. CI env-var wiring location not specified for B.3 stub
- **Source**: R3.M3
- §8.1 says "add `ONESHIM_AUTOSTART_STUB: 1`" but B.3 doesn't name ci.yml line-block. Fix: cite `.github/workflows/ci.yml` test job around line 314; prefer step-level `env:` over job-level to avoid leaking to unrelated builds.

### CONS-PM11. Snap refresh follow-up not tied to Repair button in risk register (M8)
- **Source**: R3.M8
- Plan §4.1 mentions "Repair Autostart cross-platform" but §4.8 risk register doesn't tie it to Snap. Add one-sentence note in §4.8 "Snap refresh users benefit from Repair button" + follow-up TODO registered.

### CONS-PM12. `repair_autostart` IPC has no rate-limit
- **Source**: R3.M9
- Misbehaving frontend retry loop could spam `systemctl --user enable`. Fix: trivial AtomicU64 `last_repair_at` + 5s elapsed check. Register as minor addition to B.5.

---

## 4. Disagreements

### 🚨 DISAGREEMENT-1: Q-plan-2 — autostart.rs sub-module extraction timing

- **R1 position**: "Extract now" (classified as Important). Reasoning: ADR-003 threshold is 500-600 LoC; post-B.3 file is ~650; platform-split aligns with `#[cfg(target_os = ...)]`; test_observer is a clean extraction.
- **R2 position**: "Defer to follow-up". Reasoning: splitting into mod/linux/macos/windows/test_observer during this PR inflates reviewer surface; 650 LoC is tolerable short-term; future D-errtype refactor will touch all anyway.
- **R3 position**: "Split commit" (M4). Reasoning: the extraction + behavioral fix in one commit (B.3) is a large refactor+fix mixture — better to split B.3a pure-refactor + B.3b behavioral-fix for bisect clarity.
- **Synthesis recommendation**: R3's middle-ground. Extract now (R1 correct on ADR-003 threshold) but split into `B.3a refactor(autostart): split into sub-modules per ADR-003` + `B.3b fix(autostart): non-zero-exit Err + timeout + stub env-var` commits. Two cold-clippy cycles, but bisect works and pure-refactor has zero semantic risk.
- **User input**: if user wants to defer extraction entirely (R2 position), noted as alternative; see §5.

### 🚨 DISAGREEMENT-2: Effort estimate realism

- **Plan position**: 19 wall-clock days serial (§9.1).
- **R3 position** (I8): realistic is 26-30 wall-clock days serial, 14-18 parallel. Memory `feedback_3loop_yields_real_catches.md` documents 30-50% review-cycle tax on security-sensitive PRs; PR-A's pipeline-gating qualifies.
- **R1/R2 positions**: no explicit estimate rebuttal.
- **Synthesis recommendation**: Plan §9.1 adds explicit line "expected review-cycle wall-clock tail: +3-5 days per PR for Loop 2c/2d iterations". Keep 19-day headline as optimistic floor; add 26-30-day realistic ceiling.
- **User input**: whether to resize the published plan or keep the 19-day optimistic; see §5.

---

## 5. User-decision blockers (plan-level)

Unlike Loop 1c which had 13 user decisions, Loop 2c should have few — most plan fixes are mechanical. Three genuine ambiguities surface:

### ⚠ USER-INPUT-1: Autostart sub-module extraction (Q-plan-2 / DISAGREEMENT-1)
- **Options**:
  - **A** (R1): Extract NOW as single combined B.3 commit (650 LoC → 4 files).
  - **B** (R3, recommended): Split into B.3a pure-refactor + B.3b fix; 2 cold-clippy cycles, better bisect.
  - **C** (R2): Defer extraction entirely; 650 LoC tolerable short-term; track as follow-up.
- **Synthesis default if no user input**: **B** (R3 split). Rationale: gets ADR-003 compliance + bisect clarity at cost of 1 extra cold-clippy cycle (~16 min).

### ⚠ USER-INPUT-2: CI platform coverage resolution (CONS-PI03)
- **Options**:
  - **A**: Accept Linux-only CI gate; document in new §8.7; add follow-up TODO for macOS/Windows test matrix.
  - **B**: Expand ci.yml test job to 4-platform matrix in this PR; adds ~30 min per push; delays PR-B by ~2 wall-clock days (setup + debugging).
- **Synthesis default**: **A** (document gap, defer expansion). Rationale: pre-existing CI limitation; plan-level fix scope; expanding CI is its own PR.

### ⚠ USER-INPUT-3: Effort estimate resize (DISAGREEMENT-2 / CONS-PI-residual)
- **Options**:
  - **A**: Keep 19-day headline; add R3's 26-30-day realistic ceiling as commentary.
  - **B**: Rewrite §9.1 to 26-30-day headline, 19-day as optimistic floor.
- **Synthesis default**: **A** (add commentary, keep headline). Rationale: resize is a product decision about stakeholder expectations; engineering plan is unchanged.

---

## 6. Fix plan (ordered — 26 steps)

**Ordering rationale**:
1. Baseline corrections first (correct file paths, correct CI script names, reconcile counts) — they're cited by later edits.
2. Critical design gaps next (consent/capture_paused composite, OpenAPI generation) — biggest surface change, drives test updates.
3. Important test + signature fixes.
4. Minor polish.

Each step lists: plan section(s) rewritten, original snippet (2-5 lines), replacement text, findings addressed, dependency, user-input-required (boolean).

### Baseline corrections (file paths, CI scripts, counts)

**Step 1 — Fix composition-root paths (CONS-PC01)**
- **Sections**: §3.3 A.7 (line 244), §3.3 A.12 (line 344)
- **Original**: `- src-tauri/src/main.rs (composition root) or src-tauri/src/app_runtime_launch.rs: find SmartCaptureTrigger::with_schedule(...) call — replace with SmartCaptureTrigger::new(throttle_ms).`
- **Replacement**: `- src-tauri/src/agent_runtime_support.rs:251: replace Arc::new(SmartCaptureTrigger::with_schedule(throttle_ms, schedule_config)) with Arc::new(SmartCaptureTrigger::new(throttle_ms)). (Bundle with A.12 edit to same file at line 405 — share cold-clippy cycle.)`
- **Findings**: CONS-PC01
- **Dependency**: none
- **User-input**: no

**Step 2 — Fix CI contract-drift script names (CONS-PC05)**
- **Sections**: §3.5 line 515, §4.5 line 735, §5.5 line 952, §6.5 line 1057, §8.2 line 1120
- **Original**: `./scripts/verify-integrity.sh`
- **Replacement**: `./scripts/generate-http-openapi.sh docs/contracts/oneshim-web.v1.openapi.yaml && ./scripts/verify-http-interface-manifest.sh && ./scripts/verify-http-openapi-sync.sh`
- **Findings**: CONS-PC05
- **Dependency**: Step 3 co-resolution
- **User-input**: no

**Step 3 — Rewrite OpenAPI "hand-patch" wording to generator-based (CONS-PC06)**
- **Sections**: §3.3 A.21 line 481, §4.3 B.10 line 697, §5.3 C.10 line 929, §6.5 line 1057
- **Original**: `docs/contracts/oneshim-web.v1.openapi.yaml: add 3 new routes ... This is hand-maintained (no generator); integrity gate at .github/workflows/integrity-gates.yml enforces consistency.`
- **Replacement**: `docs/contracts/http-interface-manifest.v1.json: add 3 new route entries (HAND-MAINTAINED, no generator). Then run ./scripts/generate-http-openapi.sh docs/contracts/oneshim-web.v1.openapi.yaml to regenerate the OpenAPI snapshot. Finally run ./scripts/verify-http-openapi-sync.sh to confirm no drift. CI check job (.github/workflows/ci.yml:192-199) enforces snapshot consistency on every push.`
- **Findings**: CONS-PC06
- **Dependency**: Step 2 (same conceptual area)
- **User-input**: no

**Step 4 — Reconcile PR-A test count (CONS-PC03)**
- **Sections**: §3.3 A.21 line 483, §3.5 line 518, §6.2 line 998, §7.2 table line 1079-1082
- **Original**: Four different figures (~28 / ~28 / ~28 / ~42) + breakdown summing 56.
- **Replacement**: Pick §7.2 table as source of truth (42); if CONS-PI05 is applied (A.8 13→18), bump to 47. Update all four mentions consistently:
  - §3.3 A.21 line 483: "PR-A adds ~47 new tests (A.2: 12 serde + validation; A.4: 12 helper + clock-irregularity; A.6: 3 migrated; A.8: 18 integration per-variant; A.10: 3 uploader; A.13: 5 IPC; A.15: 4 REST; A.18: 4 notifier; A.19: 7 frontend — sums to 68 if all sub-items counted, 47 if only feature-level categories)."
  - §3.5 line 518: "PR-A adds **~47 new tests** (Rust + frontend)."
  - §6.2 line 998: "PR-A adds ~47; PR-B adds ~20; PR-C adds ~30. Total Phase 9: ~97 new tests."
  - §7.2 table: update PR-A column to 47.
  - Add final-checks line: "All four test-count mentions match §7.2 table".
- **Findings**: CONS-PC03
- **Dependency**: Resolve after Step 11 (A.8 split) and Step 12 (A.2 validation tests) for correct arithmetic.
- **User-input**: no

### Critical design gaps (consent composition, audio signature)

**Step 5 — Redefine `capture_permitted_now` as 4-term composite (CONS-PC02)**
- **Sections**: §3.3 A.4 (line 180), A.5 (line 198), A.7 (line 246), A.9 (lines 282-303)
- **Original A.4 line 180**: `stub pub(crate) fn tracking_schedule_active(cfg: &AppConfig, now: DateTime<Local>) -> bool { todo!() } + pub(crate) fn capture_permitted_now(cfg: &AppConfig, now: DateTime<Local>) -> bool { todo!() }`
- **Replacement A.4**: `stub pub(crate) fn tracking_schedule_active(cfg: &AppConfig, now: DateTime<Local>) -> bool { todo!() } + pub(crate) fn capture_permitted_now(cfg: &AppConfig, consent: &ConsentPermissions, capture_paused: bool, now: DateTime<Local>) -> bool { todo!() } — composes all 4 gates per spec §3.4: consent_granted AND active_hours_gate AND !tracking_schedule_active AND !capture_paused.`
- **Original A.5 line 198**: `impl capture_permitted_now(cfg, now) composing should_run_now_with_time(cfg, now) && !tracking_schedule_active(cfg, now).`
- **Replacement A.5**: `impl capture_permitted_now(cfg, consent, capture_paused, now) composing consent.allows_tier(tier) && should_run_now_with_time(cfg, now) && !tracking_schedule_active(cfg, now) && !capture_paused per spec §3.4 composition rule.`
- **Original A.9 line 293**: `if crate::scheduler::tracking_schedule_active(&cfg.get()) || !crate::scheduler::should_run_now(&cfg.get()) { continue; }`
- **Replacement A.9** (for all 9 gate sites): `if !crate::scheduler::capture_permitted_now(&cfg.snapshot(), &consent_permissions, capture_paused.load(...), chrono::Local::now()) { continue; }` — threading `ConsentPermissions` + `capture_paused` atomic into scheduler loops.
- **Add test** to A.4: `consent_revoked_overrides_ts_inactive_active_hours` — consent revoked + TS inactive + active_hours true → false.
- **Add test** to A.8: `capture_paused_suppresses_events_during_ts_inactive`.
- **Findings**: CONS-PC02
- **Dependency**: Step 6 (audio signature — same pattern)
- **User-input**: no

**Step 6 — Spell out `start_audio_capture` state-injection (CONS-PC04)**
- **Sections**: §3.3 A.9 audio bullet (line 297-303)
- **Original**:
  ```rust
  pub async fn start_audio_capture(
      state: tauri::State<'_, AudioRuntimeState>,
  ) -> Result<(), IpcError>
  // ... proposed gate uses config_state.get() — undefined param
  ```
- **Replacement**: Specify signature change:
  ```rust
  pub async fn start_audio_capture(
      audio_state: tauri::State<'_, AudioRuntimeState>,
      config_state: tauri::State<'_, ConfigRuntimeState>,
      consent_state: tauri::State<'_, ConsentRuntimeState>,  // or equivalent
  ) -> Result<(), IpcError> {
      if !crate::scheduler::capture_permitted_now(&config_state.get().snapshot(), &consent_state.get(), capture_paused.load(...), chrono::Local::now()) {
          return Err(IpcError::new("validation.invalid_arguments", "Audio capture unavailable due to privacy gates."));
      }
      // ... existing body
  }
  ```
  Verify Tauri `generate_handler!` accommodates multi-state (precedent: `commands::settings::update_setting`).
- **Findings**: CONS-PC04
- **Dependency**: Step 5 (composite helper)
- **User-input**: no

### Important design + signature corrections

**Step 7 — Spell out autostart sync→async signature change (CONS-PI01)**
- **Sections**: §4.3 B.3 line 595-597, B.1 test module, B.5 IPC callers
- **Original**: `Wrap the Command::new("systemctl").arg(...).output() call in tokio::time::timeout(Duration::from_secs(5), tokio::task::spawn_blocking(move || cmd.output())).`
- **Replacement**: Add explicit bullet:
  > **Signature change**: `pub fn enable_autostart() -> Result<(), String>` → `pub async fn enable_autostart() -> Result<(), String>`. Same for `disable_autostart`, `is_autostart_enabled`. No production callers outside the new IPC commands in B.5 (verified via grep). Existing in-module tests (`enable_disable_roundtrip_unsupported_platform` at `:544-547`) gain `#[tokio::test]` attribute or wrap in `tokio::runtime::Runtime::new().block_on(...)`.
- **Findings**: CONS-PI01
- **Dependency**: none
- **User-input**: no

**Step 8 — Windows timeout wrap exception (CONS-PI02)**
- **Sections**: §4.3 B.3 line 597 (Windows bullet)
- **Original**: `No timeout needed for registry writes (synchronous Win32 call; no spawn), but add a 5s guard via tokio::time::timeout(Duration::from_secs(5), tokio::task::spawn_blocking(...)) for consistency.`
- **Replacement**: `Windows-specific exception: RegSetValueExW is synchronous (no process spawn, no I/O bounded delay). Keep Windows enable/disable/is_enabled as pub fn (sync) or pub async fn wrapping a sync spawn_blocking with NO timeout wrap. Adding tokio::time::timeout around RegSetValueExW is vestigial (~50-100μs overhead, extra tokio task, no real protection) — skip.`
- **Findings**: CONS-PI02
- **Dependency**: Step 7
- **User-input**: no

**Step 9 — Autostart sub-module split (DISAGREEMENT-1 / USER-INPUT-1)**
- **Sections**: §4.3 B.3 (current single commit)
- **Original**: single B.3 commit bundles extraction + behavior fix.
- **Replacement** (synthesis-default: Option B):
  - **B.3a `refactor(autostart): split into sub-modules per ADR-003`** — move `autostart.rs` → `autostart/mod.rs` + `autostart/linux.rs` + `autostart/macos.rs` + `autostart/windows.rs` + `autostart/test_observer.rs`. Pure refactor; existing tests must pass unchanged.
  - **B.3b `fix(autostart): non-zero-exit Err + 5s timeout + ONESHIM_AUTOSTART_STUB + OnceLock + async signature`** — all behavioral fixes.
- **Findings**: DISAGREEMENT-1, CONS-PM04 (bisect clarity)
- **Dependency**: Step 7 (async signature lands in B.3b)
- **User-input**: **YES** (USER-INPUT-1 — user can choose A, B, or C)

**Step 10 — Add platform-coverage-gap disclosure (CONS-PI03 / USER-INPUT-2)**
- **Sections**: NEW §8.7 "Platform coverage gap"
- **Replacement** (synthesis-default: Option A):
  > **§8.7 Platform coverage gap**: PR-B introduces `#[cfg(target_os = "windows")]` and `#[cfg(target_os = "macos")]` tests in B.1/B.4. Per `.github/workflows/ci.yml:314`, the test job runs on `ubuntu-latest` only; the build matrix (line 424) runs on all platforms but invokes `cargo build`, not `cargo test`. **Consequence**: these tests will only run locally. Before merging PR-B, developer MUST run `cargo test -p oneshim-app --lib autostart` on both macOS and Windows hosts. **Follow-up** (registered): expand test job to 4-platform matrix in a future infra PR.
- **Findings**: CONS-PI03
- **Dependency**: none
- **User-input**: **YES** (USER-INPUT-2 — user may prefer Option B: expand matrix in this PR)

### Test coverage additions

**Step 11 — Split A.8 sanity tests per-variant (CONS-PI05)**
- **Sections**: §3.3 A.8 line 271 (`ts_inactive_allows_events`)
- **Original**: 1 sanity test.
- **Replacement**: 5 per-variant sanity tests — `ts_inactive_allows_window_events`, `..._process_events`, `..._input_events`, `..._clipboard_events`, `..._file_access_events`. Each asserts `COUNT(*) > 0` for its variant. A.8 test total: 13 → 18.
- **Findings**: CONS-PI05
- **Dependency**: none; feeds into Step 4 count-reconciliation.
- **User-input**: no

**Step 12 — A.2 serde validation edge cases (CONS-PI06)**
- **Sections**: §3.3 A.2 line 142-149
- **Original**: 8 tests.
- **Replacement**: Add 4 tests — `serde_rejects_invalid_hhmm` (`"25:00"`), `serde_rejects_invalid_iana_timezone` (`"Foo/Bar"`), `window_with_empty_end_is_invalid`, `window_end_before_start_not_same_day_is_invalid`. A.2 total: 8 → 12. Spec §6.3 error codes `config.invalid` + `validation.invalid_field` align.
- **Findings**: CONS-PI06
- **Dependency**: feeds Step 4
- **User-input**: no

**Step 13 — A.4 clock-irregularity tests (CONS-PI07)**
- **Sections**: §3.3 A.4 line 182-192
- **Replacement**: Add 3 pure-fn tests — `window_active_across_suspend` (correctness regardless of tick between suspend/resume), `forward_clock_jump_into_future_window`, `forward_clock_jump_past_window_end`. A.4 total: 9 → 12.
- **Findings**: CONS-PI07
- **Dependency**: feeds Step 4
- **User-input**: no

**Step 14 — D15 frontend error-path tests (CONS-PI09)**
- **Sections**: §5.3 C.7 line 896 (frontend Vitest)
- **Replacement**: Add 3 Vitest tests — mutation 500 `storage.failed` → `onError` toast with `timeline.batchTagError`; mutation 500 `validation.invalid_arguments` (batch > 1000) → different toast; mutation 200 `affected_count: 0` → success toast "0 frames newly tagged". Also add Playwright E2E "select 5 frames, forced-error scenario → 500 → toast → selection behavior" in C.9. C.7 total: 5 → 8.
- **Findings**: CONS-PI09
- **Dependency**: none
- **User-input**: no

**Step 15 — Add `needs_repair` to AutostartStatus contract (CONS-PI08)**
- **Sections**: §4.3 B.5 line 641, B.4 test additions
- **Original**: `pub struct AutostartStatus { enabled: bool, mechanism: String, fallback_used: bool }`
- **Replacement**: `pub struct AutostartStatus { enabled: bool, mechanism: String, fallback_used: bool, needs_repair: bool }`. Define: `needs_repair = is_autostart_enabled() && !recorded_path_matches_current_exe()`. Linux: check `ExecStart=` in unit file. macOS: parse plist. Windows: read registry value. Add test `needs_repair_true_when_recorded_path_differs_from_current_exe` to B.4 + B.6.
- **Findings**: CONS-PI08
- **Dependency**: none
- **User-input**: no

### Important file-accuracy + hot-path fixes

**Step 16 — Fix TimelineLayout.tsx line citations (CONS-PI10)**
- **Sections**: §5.3 C.4 line 857
- **Original**: `Frontend crates/oneshim-web/frontend/src/pages/timeline/TimelineLayout.tsx:131-140: change data.tagged_count → data.affected_count`
- **Replacement**: `Frontend edit points — cite BOTH: (a) TimelineLayout.tsx:49 (useMutation generic type alias: rename tagged_count → affected_count in the generic type); (b) TimelineLayout.tsx:135 (onSuccess callback: data.tagged_count → data.affected_count). Pre-commit verify: grep -rn 'tagged_count' crates/oneshim-web/frontend/src/ returns 0 hits.`
- **Findings**: CONS-PI10
- **Dependency**: none
- **User-input**: no

**Step 17 — ALLOWED_KEYS edit + snapshot test update (CONS-PI11)**
- **Sections**: §3.3 A.14 line 378
- **Original**: `src-tauri/src/commands/settings.rs:44 (ALLOWED_KEYS array) — append "tracking_schedule"`
- **Replacement**: `src-tauri/src/commands/settings.rs: append "tracking_schedule" between "coaching" (line 54) and closing ] (line 55) of ALLOWED_KEYS. ALSO update the companion snapshot test allowed_keys_matches_expected_set at :343-361 to include "tracking_schedule" in the expected vec. Verify sibling test allowed_keys_excludes_sensitive_sections at :363 does NOT list "tracking_schedule" as forbidden.`
- **Findings**: CONS-PI11, CONS-PM03
- **Dependency**: none
- **User-input**: no

**Step 18 — Tray new-task spawn clarification (CONS-PI12)**
- **Sections**: §3.3 A.17 line 413-432
- **Original**: `in the tray task spawn, add ... (implies existing task)`
- **Replacement**: `A.17 creates a NEW async worker task (tray.rs currently has zero tokio::spawn calls). Spawn from the composition root (src-tauri/src/agent_runtime_support.rs, see CONS-PC01) after setup_tray returns. The worker holds an AppHandle clone for emit/icon calls. Task lifetime: teardown on app shutdown via CancellationToken or graceful drop of the receiver. Document JoinHandle ownership (likely AppState.tray_watch_handle).`
- **Findings**: CONS-PI12
- **Dependency**: CONS-PC01 (composition-root correction)
- **User-input**: no

**Step 19 — Use `snapshot()` not `get()` on hot paths (CONS-PI13)**
- **Sections**: §3.3 A.12 line 348-354 (predicate); §3.3 A.9 all gate sites
- **Original**: `cfg_mgr_for_pred.get()` in predicate; `&cfg.get()` at 9 gate sites.
- **Replacement**: `cfg_mgr_for_pred.snapshot()` returning `Arc<AppConfig>` (O(1) Arc-clone). Update all A.9 gate sites `&cm.get()` → `&cm.snapshot()`. Helper wrapper signature: `tracking_schedule_active(config: &Arc<AppConfig>)` or `tracking_schedule_active<C: Deref<Target=AppConfig>>(config: &C)`.
- **Findings**: CONS-PI13
- **Dependency**: Step 5 (composite helper signature)
- **User-input**: no

**Step 20 — Observability section (CONS-PI14)**
- **Sections**: NEW §6.6 "Observability"
- **Replacement**: Add section enumerating:
  - Tracing spans: `tracking_schedule_active(enter/exit)`, `autostart_enable/disable/status`, `bulk_tag_transaction(add/remove)`.
  - Counters: `oneshim_tracking_schedule_state{active:bool}`, `oneshim_bulk_tag_operations_total{op:add|remove, result:ok|err}`, `oneshim_autostart_attempt_total{result, mechanism}`.
  - `err.code` fields on all new `warn!`/`error!` sites (CLAUDE.md observability convention).
  - Audit entry `TrackingScheduleTransition{prev, now, reason: "user" | "scheduled"}` from A.18.
- **Findings**: CONS-PI14
- **Dependency**: none
- **User-input**: no

**Step 21 — Pre-flush drain explicit deferral (CONS-PI04)**
- **Sections**: §3.4 "PR-A cross-cutting docs / Follow-up TODOs" bullet list (line 498)
- **Replacement**: Add bullet: `on_window_boundary_approaching pre-flush drain for long TS windows (spec §3.9 clause 4, deferred per complexity; drops events if queue approaches max_queue_size over a 10h+ suppression).`
- **Findings**: CONS-PI04
- **Dependency**: none
- **User-input**: no

### Minor polish + cross-cutting

**Step 22 — Effort-estimate commentary (DISAGREEMENT-2 / USER-INPUT-3)**
- **Sections**: §9.1
- **Replacement** (synthesis-default: Option A): Add line: `Realistic ceiling: 26-30 wall-clock days serial accounting for 30-50% review-cycle tax on security-sensitive PRs (memory feedback_3loop_yields_real_catches.md) — PR-A's pipeline-gating qualifies. 19-day headline is optimistic floor.`
- **Findings**: DISAGREEMENT-2
- **Dependency**: none
- **User-input**: **YES** (USER-INPUT-3 — user may prefer Option B: rewrite headline to 26-30 days)

**Step 23 — Rate-limit repair_autostart IPC (CONS-PM12)**
- **Sections**: §4.3 B.5 line 639
- **Replacement**: Add note: `Rate-limit: 5s min-interval via AtomicU64 last_repair_at — rejects spam with IpcError (cooldown.throttled or validation.invalid_arguments).`
- **Findings**: CONS-PM12
- **Dependency**: none
- **User-input**: no

**Step 24 — CI stub env-var wiring location (CONS-PM10)**
- **Sections**: §4.3 B.3 (env-var setup), §8.1
- **Replacement** in §8.1: `Add ONESHIM_AUTOSTART_STUB: "1" to the env: block of the Rust test STEP (not job-level) in .github/workflows/ci.yml around line 314. Step-level scoping prevents leaking to unrelated builds.`
- **Findings**: CONS-PM10
- **Dependency**: none
- **User-input**: no

**Step 25 — PartialEq cross-consumer audit prelude (CONS-PM02 / R1.I3)**
- **Sections**: §3.3 A.17 line 429
- **Replacement**: Add pre-commit verification: `Before A.17 lands: grep -rn 'NotificationConfig ==\|NotificationConfig !=\|PartialEq<NotificationConfig>' crates/ src-tauri/ → if zero hits, PartialEq + Eq is a pure additive derive (safe). If nonzero, list affected callers and accept the semantics change explicitly.` Also: narrow diff from `cfg.notification != last.1` to `cfg.notification.tracking_schedule_enabled != last_ts_flag` per spec §3.11a filter rule.
- **Findings**: CONS-PM02
- **Dependency**: none
- **User-input**: no

**Step 26 — Line-drift + minor polish sweep (CONS-PM03, PM04, PM05, PM06, PM07, PM08, PM09, PM11)**
- **Sections**: multiple — §3.3 prose, §9.4 arithmetic, §3.4 follow-up list, §4.8 risk register
- **Replacement**: Batch polish:
  - TimelineLayout.tsx:131-140 → 130-138 (CONS-PM03).
  - §3.3 opening prose: note "A.1 is a prerequisite dep-bump commit, not TDD red-first" (CONS-PM04).
  - Add explicit dep graph to §3.3 (CONS-PM05).
  - Fix §9.4 PR-B commit-count 5 → 6 test commits (CONS-PM06).
  - §3.4: commit to `project_next_tasks.md`, drop the either/or (CONS-PM07).
  - Add note on Korean i18n grep (acceptable as manual spot-check) (CONS-PM08).
  - Optional: add `metrics_loop_continues_during_ts` sanity test for row 16 coverage (CONS-PM09).
  - §4.8: one-sentence note tying Repair button to Snap refresh follow-up (CONS-PM11).
- **Findings**: CONS-PM03, PM04, PM05, PM06, PM07, PM08, PM09, PM11
- **Dependency**: last — cleanup pass
- **User-input**: no

---

## 7. Expected remaining issues post-fix

Assuming all 26 steps land:

- **Zero Critical** — all 6 CONS-PCxx resolved mechanically.
- **Zero Important** — all 14 CONS-PIxx resolved.
- **Residual Minor** — CONS-PM01 (watch coalescence documented as non-goal rather than fixed; acceptable per spec §3.7a); CONS-PM09 (optional coverage addition). Both non-blocking.
- **Residual user choices** — 3 decisions remain (sub-module split style, CI matrix timing, effort-estimate presentation) but all have synthesis-defaults; user-silence advances with default.

**Expected verdict after Loop 2d**: **PASS** all three reviewer axes on re-verify (Loop 2e).

Remaining risk categories after fix-plan:
1. R3-derived observability §6.6 may attract review bike-shedding (counter naming, span granularity). Tolerable — not a gate.
2. DISAGREEMENT-2 on effort estimate is a framing choice — will surface again at PR description level but does not affect engineering correctness.
3. `watch::Receiver` coalescence (CONS-PM01) may surface in Loop 3 integration testing if a reviewer sees a flaky notifier test — document as known non-goal.

---

## Report back

- **Total consolidated count**: 32 findings (6 Critical, 14 Important, 12 Minor; +2 disagreements)
- **User-decision items**: 3 (USER-INPUT-1 sub-module split style, USER-INPUT-2 CI matrix timing, USER-INPUT-3 effort-estimate resize) — all with synthesis-defaults
- **Fix-plan length**: 26 ordered steps
- **Disagreements**: 2 items (Q-plan-2 autostart extraction timing, effort-estimate realism)

_End of synthesis._
