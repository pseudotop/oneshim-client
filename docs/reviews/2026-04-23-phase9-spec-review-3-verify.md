# Phase 9 Spec Review 3 — Verification Pass (Loop 1d → Loop 2 gate)

**Reviewer**: R3' (re-review of R3 round-1)
**Date**: 2026-04-23 (verify pass)
**Spec under review**: `docs/reviews/2026-04-23-phase9-quick-wins-spec.md` @ 1599 lines (was 1247 lines @ round 1)
**Round-1 review**: `docs/reviews/2026-04-23-phase9-spec-review-3-platform-test-risk.md` (274 lines; 2C / 7I / 9M)
**Synthesis**: `docs/reviews/2026-04-23-phase9-spec-review-synthesis.md`
**Worktree tip**: `5618558c`

## 0. Method

Each R3 finding was re-verified by grepping or reading the revised spec for the required fix. Facts independently re-verified against the worktree (test counts, file line counts, CI YAML).

Baseline commands (re-run):
- `wc -l src-tauri/src/scheduler/loops/monitor.rs` → 498 (matches spec CONS-I06 citation).
- `grep -c "#\[test\]" src-tauri/src/autostart.rs` → 9 (matches spec CONS-C11 fix).
- `grep -c "#\[test\]" crates/oneshim-vision/src/trigger.rs` → 13 (matches spec CONS-C11 fix).
- `grep -c "^[a-z]" crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` → 42 (matches spec §6.3).
- `ls crates/oneshim-web/frontend/e2e/ | grep timeline-actions` → `timeline-actions.spec.ts` exists (matches spec §6.1 Feature 3).
- `grep -n "with_health_flag" crates/oneshim-network/src/batch_uploader.rs` → line 74 (matches spec §3.9 corrected precedent).
- `grep -c app-sandbox src-tauri/assets/oneshim.entitlements` → 0 (macOS not sandboxed; autostart plist writes are unaffected by TCC — observation confirmed).
- `ls .github/workflows/` → `ci.yml`, `integrity-gates.yml` present; test job runs on `ubuntu-latest` (confirms R3.C2 CI constraint still applies; env-var stub escape hatch is the correct mitigation).

## 1. Part 1 — R3 round-1 finding verification

### Critical

| ID | Round-1 ask | Revision section(s) | Status | Notes |
|----|-------------|---------------------|--------|-------|
| **R3.C1** Mock-clock missing | §6.1 must specify 2-arg `tracking_schedule_active(cfg, now)` per U3=(b) | §3.8 helper extraction (lines 368-374); §6.1 Feature 1 (lines 1225-1233); Decisions D14/D17 | **PASS** | Spec explicitly states 2-arg pure-fn shape: `tracking_schedule_active(cfg: &AppConfig, now: DateTime<Local>) -> bool`. §6.1 Feature 1 test list references "pure-fn 2-arg shape per U3 = Option B" and enumerates multiple unit tests against it. No mock-clock primitive required. |
| **R3.C2** Linux systemctl CI fails silently | §6.1 Feature 2 env-var stub; `autostart.rs:389-401` behavior fix | §6.1 "CI env-var stub escape hatch" (lines 1245-1250); §4.10 "Behavioral fix required (CONS-C10)" (lines 833-843) | **PASS** | `ONESHIM_AUTOSTART_STUB=1` documented with module-level read, command-shape assertion via `TestObserver`, CI wiring. All three platforms (macOS/Linux/Windows) now required to return `Err` on non-zero exit and wrap `Command::output` in `tokio::time::timeout(Duration::from_secs(5), …)`. |

### Important

| ID | Round-1 ask | Revision section(s) | Status | Notes |
|----|-------------|---------------------|--------|-------|
| **R3.I1** `is_enabled()` cross-platform lie | §4.3 + §4.7 annotate as pre-existing, applies to all 3 platforms | §4.3 "Cross-platform `is_enabled()` caveat (per CONS-M09)" (lines 652-658); §4.7 "Repair Autostart button ... on all three platforms" (line 769) | **PASS** | Explicitly enumerated for macOS `plist_path().exists()`, Linux file-existence, Windows `RegQueryValueExW`. Repair-Autostart button scoped to all 3 platforms. |
| **R3.I2** `has_systemctl()` caching | §4.3 add `OnceLock<bool>` memoization | §4.3 "`has_systemctl` memoization (Q5 resolved in-place)" (line 650) | **PASS** | `static HAS_SYSTEMCTL: OnceLock<bool> = OnceLock::new();` one-time init per process. Q5 closed deterministically. |
| **R3.I3** Suspend/resume + clock-skew | §3.7a new subsection | §3.7a "Clock irregularities" (lines 329-341); §3.11 60s debounce via `last_notification_at: Instant` (line 521) | **PASS** | 5-row table covering suspend-across-boundary, backward jump (debounce), forward skipping, forward-into-future-window, user-self-inflicted clock setbacks. Mitigations all in scheduler-side helper. |
| **R3.I4** Only 4/16 loops enumerated | §3.8 must enumerate ≥13 pipelines | §3.8 table (lines 347-364) | **PASS** | 16 rows. 13 gated (capture, monitor, window-switch, analysis, focus, coaching, process events, input events, clipboard, file-access, upload flush, cross-device sync, audio-capture command), 3 ungated with rationale (heartbeat, OAuth, metrics/aggregation). Overlap with R2.C1 resolved. |
| **R3.I5** `with_capture_paused` false precedent | §3.9 rewrite to reference `with_health_flag` | §3.9 final paragraph (lines 491-493) | **PASS** | Corrected citation: "The closest existing precedent on `BatchUploader` is `with_health_flag(mut self, flag: Arc<AtomicBool>) -> Self` at `crates/oneshim-network/src/batch_uploader.rs:74` (circuit-breaker gating flag)." Explicitly disavows prior `with_capture_paused` false-precedent claim. |
| **R3.I6** Autostart non-zero exit swallowed on 3 platforms | §4.10 map non-zero exit + return Err across 3 platforms | §4.10 table row 3 (line 827); "Behavioral fix required (CONS-C10)" (lines 833-843) | **PASS** | Explicit for all three platforms: Linux `:389-401`, macOS `:137-141`, Windows `RegSetValueExW` return. 5-second `tokio::time::timeout` wrap mandatory on all three. Test covers `enable() returns Err on non-zero exit` and `enable() times out after 5s`. |
| **R3.I7** Batch-tag test gaps (FK, concurrent, empty, cache) | §6.1 Feature 3 list 5+ tests | §6.1 Feature 3 (lines 1254-1265) | **PASS** | All 5 enumerated: `add_tag_to_frames_rolls_back_on_fk_violation`, `remove_tag_from_frames_handles_missing_pairs_transactionally`, `batch_ops_compete_with_concurrent_writer`, `empty_input_is_lock_free`, `statement_cache_reuse_across_rolled_back_transactions`. Plus `MAX_BATCH_SIZE` cap test. |

### Minor

| ID | Round-1 ask | Revision section(s) | Status | Notes |
|----|-------------|---------------------|--------|-------|
| **R3.M1** chrono-tz size | D16 decision + scope | §3.7 Decision D16 (lines 316-327) | **PASS** | +2.1MB accepted, placement in `oneshim-core`, alternative port-adapter rejected with 3-point rationale. |
| **R3.M2** Playwright dir `frontend/tests` doesn't exist | §6.1 Feature 3 use `frontend/e2e/…` | §6.1 Feature 3 line 1265; §6.5 line 1405 | **PASS** | Correct path `crates/oneshim-web/frontend/e2e/timeline-actions.spec.ts` referenced; explicit "not `frontend/tests/…`" disavowal. Verified file exists in worktree. |
| **R3.M3** Repair-Autostart UI orphaned | §4.7 resolve in-place | §4.7 final paragraph (line 769) | **PASS** | One-sentence behavior spec in §4.7; button scoped to all 3 platforms; stale-path detection heuristic documented. |
| **R3.M4** Tray icon Q7 | §3.11 resolve or defer | §3.11 (line 518) | **PASS** | Reuse Paused icon + change tooltip label — Q7 resolved in-place; no new art assets required. |
| **R3.M5** Alpine / musl / OpenRC / Snap coverage | §4.8 tighten; §4.7 add Snap | §4.8 "Best-effort caveat" (line 782); §4.7 binary-path table line 761 | **PASS** | Snap row added: "Changed each refresh — YES — broken — Snap users must re-enable after each snap refresh". DE-less distros scoped as best-effort. |
| **R3.M6** `serial_test` for autostart integration | §6.5 update | §6.5 line 1413 | **PASS** | "`serial_test` required for new autostart integration tests that touch real FS state (`~/.config/systemd/user/*.service`, `~/Library/LaunchAgents/*.plist`, HKCU registry)". Existing unit tests exempted. Memory reference `reference_serial_test_pattern.md` honored. |
| **R3.M7** `err.code` observability convention | §6.2 enumerate all autostart sites | §6.2 (lines 1277-1298) | **PASS** | 9 enumerated structured-log sites across macOS/Linux/Windows paths plus bulk-tag. All use `%mech` (Display) not `?mech` (Debug). All error sites carry `err.code = "<wire_code>"` structured field. |
| **R3.M8** OpenAPI/manifest integrity-gate | §6.5 clarify | §6.5 lines 1406-1410 | **PASS** | Explicitly called out as integrity gate via `.github/workflows/integrity-gates.yml` (verified present). Hand-maintenance of `http-interface-manifest.v1.json` acknowledged; plan-step-ownership named. |
| **R3.M9** STATUS.md / PHASE-HISTORY.md / companion docs | §6.5 scope | §6.5 "Docs updates required in-PR" (lines 1416-1420) | **PASS** | Three bullets: STATUS test-count bump, PHASE-HISTORY new entry, explicit "no user-facing guide added" per D-guide. Workspace CLAUDE.md "41 wire codes" noted as stale → `reference_doc_drift` follow-up. |

**Part 1 total**: 18 of 18 R3 findings fixed (2C + 7I + 9M).

## 2. Part 2 — Regression scan

### 2.1 Test strategy — do the new tests type-check conceptually?

Feature 1 tests (§6.1 lines 1225-1233):
- `tracking_schedule_helper::tracking_schedule_active(cfg: &AppConfig, now: DateTime<Local>) -> bool` — pure fn; directly testable. **Conceptually sound**.
- `capture_permitted_now(cfg, now)` — composition of 2 pure-fn returns. **Sound**.
- `TrackingWindow::window_is_active(now)` — 12+ cases including DST spring-forward (correctly asserts "fires zero times" per CONS-C04 rewrite), fall-back (correctly asserts "fires twice"). **Sound**.
- Integration "upstream-gated event pipeline" test — asserts zero new rows in `events` table across 5 event variants during TS window. Requires actual event-loop instrumentation but no mock-clock. **Conceptually sound**.
- BatchUploader flush short-circuit via closure — verifies CONS-C03 FIFO-exit semantics. **Sound**.

Feature 2 tests (§6.1 lines 1237-1252):
- 9 existing autostart tests stay. **Sound** per worktree `grep -c '#\[test\]'` verification.
- 4 new tests: Wayland env, XDG session type, non-zero exit Err, timeout. Rely on `ONESHIM_AUTOSTART_STUB=1` + `TestObserver`. **Conceptually sound** but thread-local observer implementation must be manually mocked (no mockall per ADR-001 §5).
- IPC contract tests gated per `#[cfg(target_os = ...)]`. **Sound**.
- REST contract tests — GET→PUT→GET cycle with stub enabled. **Sound**.

Feature 3 tests (§6.1 lines 1256-1265):
- 7 unit + 1 integration + 1 E2E. All plausible against the storage/handler layers as specced in §5.4-5.6. **Sound**.

### 2.2 Platform coverage

- **Linux systemd** — covered in §4.1, §4.6 (Wayland env detection), §4.8 (XDG fallback), §4.10 (error mapping), §6.1 (env-var stub). ✅
- **Linux XDG** — covered in §4.8 as fallback when `has_systemctl() == false`, with best-effort caveat for DE-less distros. ✅
- **Linux Wayland** — covered in §4.6 via `WAYLAND_DISPLAY` + `XDG_SESSION_TYPE` env forwarding in unit file. ✅
- **macOS LaunchAgent** — covered in §4.10 (launchctl non-zero exit), §4.7 (binary-path stability), §4.3 (`is_enabled()` caveat). Sandbox observation confirmed: macOS is not app-sandboxed (verified via `oneshim.entitlements` grep), so plist write to `~/Library/LaunchAgents/` is unrestricted. ✅
- **Windows registry** — covered in §4.10 (`RegSetValueExW` non-zero return), §4.3 (HKCU Run key existence ≠ valid executable path). ✅

### 2.3 Performance claims

- `MAX_BATCH_SIZE = 1000` → "1000 ids → 200 < 50ms" (§5.9 line 1196; §6.1 line 1264). **Concern**: the 50ms number extrapolates from the 50-frame < 10ms measurement in §5.10 (actually cites "~1-2ms typical" from `events.rs:126` precedent) via linear scaling. Linear scaling of SQLite batched-insert is a reasonable but unproven assumption at 20x the measured size. A real benchmark is not cited. **Minor** — at 50ms the budget is >>100ms perceptual threshold and the test would catch regressions. Recommend the impl plan runs the benchmark once during implementation.
- Tracking-schedule gate O(n) in windows vec (§3.10 line 508). Spec states "negligible at typical window counts (≤ 20 per realistic user)". Reasonable.

### 2.4 CI implications

- `ONESHIM_AUTOSTART_STUB=1` read location: "the autostart module" (§6.1 line 1246). Module-level read. **Clear enough** for plan phase. The impl plan should specify exactly which `fn` reads the env-var (most cleanly: wrap `Command::output` inside a test-gated branch at each of the 3 platform-module call sites, or at a single helper `spawn_with_stub()`).
- CI job location: `.github/workflows/ci.yml` line 370-398 (verified) — `test` job runs on `ubuntu-latest` with `RUN_HEAVY_TESTS` gate. Spec's claim of "sets `ONESHIM_AUTOSTART_STUB=1` in the test env" is plan-level; the actual `env:` stanza addition will come in impl.
- No new CI matrix job needed (U4 = Option B chosen — env-var stub preferred over new Linux matrix with `systemd-run --user`).

### 2.5 Lefthook / pre-push

No new hook changes specified in spec. Lefthook pre-commit clippy cost (~16min cold) per memory `feedback_lefthook_clippy_cost.md` — acceptable since Phase 9 features are additive. No hook modifications required.

### 2.6 Observability — `err.code` convention

Verified across §6.2 (lines 1269-1298):
- All 9 autostart log sites include `err.code = "<wire_code>"` structured field. ✅
- Bulk-tag log sites (`err.code = "storage.failed"`, `err.code = "validation.invalid_arguments"`) follow convention. ✅
- Tracking-schedule info!() lines use `label = %label`, `ends_at = %ends_at` — stable Display rendering. ✅
- Debug-based `?mech` replaced with Display-based `%mech` throughout. ✅

Per CLAUDE.md Logging convention and memory note `reference_observability_err_code_pattern`. All pass.

### 2.7 Rollout risk / feature flag

No feature flag used. All features default to opt-out (tracking_schedule.enabled=false), user-driven (autostart requires toggle interaction), or purely additive with zero-impact deser (§3.13 "Zero-impact upgrade"). **Acceptable** — no hidden state change.

Risk watch-items:
- D15 (200→500 behavior change on batch-tag): **intentional** per CONS-C09. Frontend `TimelineLayout.tsx:131-140` updated in-PR. Acceptable.
- D17 (trigger refactor in-PR): **intentional** per U7. 3 schedule tests migrate; 10 stay. Acceptable.

### 2.8 Sandbox profile (ADR-002 M3)

Autostart paths (`~/Library/LaunchAgents/*.plist`, `~/.config/systemd/user/*.service`, HKCU registry) are written by the main process, not the `oneshim-sandbox-worker`. Verified above that macOS `oneshim.entitlements` has no `app-sandbox` key (hardened-runtime only). No new sandbox entries needed. Spec's silent omission of a sandbox section is acceptable because the round-1 R3 observation (#1) already concluded "sandbox profile is clean" — no action required.

### 2.9 `serial_test` for integration

§6.5 (line 1413) requires `serial_test` for new autostart integration tests that touch real FS state. Aligned with memory note `reference_serial_test_pattern.md`. ✅

### 2.10 Docs deliverables

§6.5 (lines 1416-1420) enumerates in-PR scope:
- `docs/STATUS.md` — test-count bump (required).
- `docs/PHASE-HISTORY.md` — new Phase 9 entry (required).
- `docs/DOCUMENTATION_POLICY.md` companion compliance: explicit "no user-facing guide added" per D-guide (U13). Satisfies policy via the explicit statement.
- Workspace-level `CLAUDE.md` "41 wire codes" → 42 stale-doc bump (noted as follow-up, not in-PR strictly required).

## 3. Part 3 — Binary verdict

**PASS** — zero R3-lens Critical and zero R3-lens Important remain.

All 2 R3 Critical findings resolved with concrete plan-ready specifications:
- R3.C1 → 2-arg pure-fn `tracking_schedule_active(cfg, now)` shape (U3 Option B locked).
- R3.C2 → `ONESHIM_AUTOSTART_STUB=1` env-var stub + non-zero-exit Err return + 5s timeout across all three platforms.

All 7 R3 Important findings resolved:
- R3.I1 → cross-platform `is_enabled()` caveat documented.
- R3.I2 → `OnceLock<bool>` memoization for `has_systemctl()`.
- R3.I3 → §3.7a clock-irregularities table with 5 scenarios + 60s debounce.
- R3.I4 → 16-row scheduler enumeration table (13 gated + 3 ungated).
- R3.I5 → `with_health_flag` precedent citation corrected.
- R3.I6 → non-zero-exit Err + 5s timeout mandated on all 3 platforms.
- R3.I7 → 5 new test cases listed for bulk-tag with MAX_BATCH_SIZE bound.

All 9 R3 Minor findings resolved.

### Residual watch-items (non-blocking, plan-phase)

1. `MAX_BATCH_SIZE=1000` perf claim extrapolates linearly from 50-frame `events.rs:126` measurement; impl plan should run a 1000-row benchmark once during coding. Minor; test asserting `< 50ms` will catch regression.
2. `TestObserver` mock harness for env-var stub — implementation shape not specified (thread-local vs per-test-instance). Impl plan must decide.
3. D15 (200→500 batch-tag behavior change) — frontend `TimelineLayout.tsx:131-140` consumer patch scope is 3 exact lines; impl plan verifies no other consumer post-merge.
4. `CLAUDE.md` "41 wire codes" stale — tracked as `reference_doc_drift` follow-up; non-blocking.

### Confidence

High. The revised spec addresses every round-1 R3 finding with concrete, verifiable, plan-ready specifications. CI workflow reality (ubuntu-latest only, integrity-gates hand-maintained manifest) verified against actual workflow files. No hidden regressions introduced by the revisions. Ready for Loop 2 implementation-plan draft.

_End of verification._
