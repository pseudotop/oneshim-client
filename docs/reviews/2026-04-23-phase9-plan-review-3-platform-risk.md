# Phase 9 Plan Review 3 — Platform + Effort + CI + Risk

**Reviewer**: 3 of 3
**Lens**: Cross-platform, effort realism, CI, failure modes, observability, risk
**Date**: 2026-04-24
**Plan under review**: `docs/reviews/2026-04-23-phase9-quick-wins-plan.md` (1353 lines, first-draft)
**Worktree tip**: `5618558c`

---

## Summary

- **Critical**: 3
- **Important**: 8
- **Minor**: 9

**Gate**: FAIL — 3 Critical + 8 Important finds block zero-zero gate. Plan needs a pass-2 revision before Loop 3 kickoff.

Each Critical or Important is actionable — most require only one sentence updates, a few require targeted replan of 1-3 commits.

---

## Critical findings

### C1. Wrong integrity script cited across all three PR acceptance gates

**Location**: plan §3.5 line 515, §4.5 line 735, §5.5 line 952, §6.5 line 1057, §8.2 line 1120, §9.6 contingency. Five distinct `./scripts/verify-integrity.sh` citations.

**Evidence**:
- Actual `scripts/verify-integrity.sh` runs `cargo-audit`, `cargo-deny`, `cargo-vet`, `cargo-cyclonedx` SBOM generation, + integrity_guard/signature tests. This is a **security/supply-chain** gate.
- The **contract gates** the plan intends to cite are:
  - `./scripts/verify-http-interface-manifest.sh` (manifest structure)
  - `./scripts/verify-http-openapi-sync.sh` (OpenAPI regenerated from manifest)
  - `./scripts/generate-http-openapi.sh` (generator)
- These three run in `ci.yml` `check` job (lines 192-199), not in `integrity-gates.yml`.

**Impact**: Any engineer following the plan's local `./scripts/verify-integrity.sh` command will run the security gate (which may require `cargo-audit`/`cargo-deny`/`cargo-vet`/`cargo-cyclonedx` binaries pre-installed — see `scripts/cargo-cache.sh install` in `integrity-gates.yml`) and still merge with an OpenAPI drift. The actual OpenAPI drift detection is silent locally until CI `check` job runs.

**Fix**: Replace every `./scripts/verify-integrity.sh` citation with the explicit pair:
```
./scripts/verify-http-interface-manifest.sh && ./scripts/verify-http-openapi-sync.sh
```
Add a note that `./scripts/generate-http-openapi.sh docs/contracts/oneshim-web.v1.openapi.yaml` must be run to regenerate the snapshot **before** running the sync check. Flag to the user that this was propagated from the spec (spec §6.5 carries the same misunderstanding — see M2 below).

### C2. OpenAPI snapshot is auto-generated, not hand-maintained

**Location**: plan §3.3 Commit A.21 line 481 ("OpenAPI ... This is **hand-maintained** (no generator)"), §6.5 line 1057, §10 Q-plan-4 implicitly, §5.3 Commit C.10 (same).

**Evidence**:
- `scripts/generate-http-openapi.sh` (exists, 75 LoC) reads `docs/contracts/http-interface-manifest.v1.json` → generates `docs/contracts/oneshim-web.v1.openapi.yaml`.
- `scripts/verify-http-openapi-sync.sh` regenerates to tmp and `diff -u` against the tracked file. Non-zero on drift.
- CI `check` job runs both on every push/PR (ci.yml:192-199).

**Impact**: If engineers hand-patch `oneshim-web.v1.openapi.yaml` per plan A.21/B.10/C.10, the CI check job will fail with `snapshot drift detected`. They'll have to unwind their hand-edits and re-run the generator. Wasted iteration — a full CI cycle (~28 min per R3' memory).

**Fix**:
1. For each of A.21, B.10, C.10: reword "hand-patch" to "add manifest entries, then run `./scripts/generate-http-openapi.sh docs/contracts/oneshim-web.v1.openapi.yaml` to regenerate snapshot, then run `./scripts/verify-http-openapi-sync.sh` to confirm".
2. Confirm the manifest itself IS hand-maintained (no generator for it). The plan's "no generator" statement is true for the manifest but not for the OpenAPI.
3. Add a note in §8.2 CI impact section that hand-editing `oneshim-web.v1.openapi.yaml` will fail the `check` job.

### C3. Windows + macOS test jobs don't exist in CI; Windows/macOS autostart tests can only run locally

**Location**: plan §8.1 line 1112 ("None new. All Phase 9 tests run in existing jobs"). plan §4.3 commits B.1 + B.4 list `#[cfg(target_os = "windows")]` test gates.

**Evidence**:
- `.github/workflows/ci.yml` test job: `runs-on: ubuntu-latest` (single runner; line 314).
- `build` job matrix includes macos-latest, macos-14, windows-latest but only builds (`cargo build`); does not run `cargo test`.
- `macos-windowserver-gui-smoke.yml` is `workflow_dispatch` only — not a merge gate.
- Per memory `feedback_cross_platform_cargo_check.md`: macOS-local dev machine does not exercise `#[cfg(target_os = "linux|windows")]` paths.

**Impact**: PR-B tests in B.1 (`windows_enable_returns_err_on_regsetvalueexw_nonzero`) and B.4 (`get_status_returns_mechanism_per_platform` under Windows + macOS branches) **will never run in CI**. A Windows bug in `autostart.rs::windows::enable` (including the new 5s timeout wrap from B.3) escapes merge gates entirely. Similarly for macOS `launchctl` non-zero handling.

**Fix**: Two options:
1. **Accept the gap, document explicitly**: add a §8.7 "Platform coverage gap" section stating that PR-B platform-branched tests are Linux-only in CI; developers must run `cargo test` locally on macOS + Windows before merge. Add a follow-up TODO to wire macOS/Windows test runners in a future CI expansion.
2. **Add a matrix to the test job**: duplicate the build matrix into the test job — run `cargo test --workspace` on all four platforms. This adds ~30 min wall-clock per push and needs macOS/Windows Linux-dependency setup. Recommend option 1 for Phase 9 + track option 2 as follow-up.

Either way, the plan must explicitly acknowledge that `#[cfg(target_os = "macos")]` + `#[cfg(target_os = "windows")]` tests introduced in PR-B cannot be asserted green via CI merge gates. Currently the plan implies they do.

---

## Important findings

### I1. Async signature change in `autostart::enable_autostart()` not addressed in B.3

**Location**: plan §4.3 Commit B.3 line 595-597.

**Evidence**: `src-tauri/src/autostart.rs:8` declares `pub fn enable_autostart() -> Result<(), String>` — **sync**. Plan B.3 says to wrap `Command::new("systemctl")...output()` in `tokio::time::timeout(Duration::from_secs(5), tokio::task::spawn_blocking(...))`. `tokio::time::timeout` returns a `Future` — can only be `.await`-ed from an async context. Wrapping inside `fn enable_autostart()` forces:
- Either blocking on the future via `tokio::runtime::Handle::current().block_on(...)` (panics if called from inside a tokio runtime).
- Or changing the signature to `async fn`.

Plan doesn't pick one. Downstream callers at `src-tauri/src/agent_runtime_support.rs:251` (SmartCaptureTrigger with_schedule) are not the autostart caller; actual callers are the new IPC commands (introduced in B.5) which are already async. So `async fn enable_autostart()` is the cleaner option — but the plan must SAY so.

**Fix**: Plan B.3 should explicitly state:
- Change `pub fn enable_autostart()` → `pub async fn enable_autostart()` (+ disable + is_enabled).
- IPC commands in B.5 already `async`, no additional awaits needed beyond forwarding.
- Enumerate existing call sites: `agent_runtime_support.rs` search confirms zero direct callers of `enable_autostart`/`disable_autostart` outside the new IPC commands. Verify before B.3 lands.

### I2. Windows `RegSetValueExW` is synchronous — `tokio::time::timeout` wrap is vestigial but plan claims otherwise

**Location**: plan §4.3 Commit B.3 line 597 ("No timeout needed for registry writes (synchronous Win32 call; no spawn), but add a 5s guard via `tokio::time::timeout(Duration::from_secs(5), tokio::task::spawn_blocking(...))` for consistency").

**Evidence**: `autostart.rs:200-227` is a single unsafe Win32 `RegSetValueExW` call — no process spawn, no I/O bounded delay. The 5s timeout can only protect against a hang inside `RegOpenKeyExW` / `RegSetValueExW` themselves, which in practice block on registry hive lock contention (rare, bounded).

**Impact**: Adding `spawn_blocking` + `tokio::time::timeout` inside Windows adds ~50-100μs overhead + allocates a tokio task — for a guaranteed-synchronous call. Worse: it pulls Windows autostart into the async runtime, which may surface subtle ordering bugs during app shutdown.

**Fix**: Keep Windows `enable`/`disable`/`is_enabled` synchronous; skip the timeout wrap for Windows. Adjust `enable_autostart()` public API: if it's kept sync, Windows dispatches unchanged; Linux/macOS use `block_in_place + spawn_blocking`. If kept async (preferred per I1), Windows paths use `spawn_blocking` (no timeout). Either way, plan needs to state the Windows-specific exception.

### I3. Plan cites wrong SmartCaptureTrigger caller file

**Location**: plan §3.3 Commit A.7 line 244 ("`src-tauri/src/main.rs` (composition root) or `src-tauri/src/app_runtime_launch.rs`").

**Evidence**: `grep -rn "SmartCaptureTrigger::" src-tauri/src/` returns exactly one hit:
```
src-tauri/src/agent_runtime_support.rs:251:            Arc::new(SmartCaptureTrigger::with_schedule(
```
Not `main.rs`, not `app_runtime_launch.rs`.

**Impact**: Engineer following the plan opens `main.rs`, doesn't find the call, gets confused. Bigger risk: per memory `feedback_cross_worktree_line_drift.md`, spec/plan file:line references are brittle across worktrees — a wrong file name signals the plan drafter didn't verify.

**Fix**: Replace "`src-tauri/src/main.rs` (composition root) or `src-tauri/src/app_runtime_launch.rs`" with explicit "`src-tauri/src/agent_runtime_support.rs:251`". Plan drafter should re-verify all file:line citations with `grep -rn` before finalizing.

### I4. Frontend type-alias rename incomplete (TimelineLayout.tsx:49 missed)

**Location**: plan §5.3 Commit C.4 line 857 ("Frontend `crates/oneshim-web/frontend/src/pages/timeline/TimelineLayout.tsx:131-140`: change `data.tagged_count` → `data.affected_count`").

**Evidence**: `grep -n "tagged_count"`:
```
TimelineLayout.tsx:49:    typeof useMutation<{ tagged_count: number }, Error, { frameIds: number[]; tagId: number }>
TimelineLayout.tsx:135:      addToast('success', t('timeline.batchTagged', { count: data.tagged_count }))
client.ts:579:export async function batchAddTag(...): Promise<{ tagged_count: number }>
api-contracts/src/tags.rs:31:    pub tagged_count: u32,
handlers/tags.rs:88, 91, 97: tagged_count (Rust-side)
```
Line 135 is in the 131-140 range that plan calls out. Line 49 is NOT in that range — it's the `useMutation` generic type declaration, which compiles on the Rust+TS type identity.

**Impact**: If engineer changes only the call site (135) + api-contracts + handlers + client.ts, but misses 49, TypeScript will complain that `data: { tagged_count: number }` (per type-alias) doesn't have `affected_count`. Easy to catch in `pnpm tsc`, but avoidable.

**Fix**: Plan C.4 should cite **TimelineLayout.tsx:49** explicitly as another edit point. Also confirm by `grep -rn "tagged_count" crates/oneshim-web/frontend/src/` produces exactly 3 hits (client.ts:579, TimelineLayout.tsx:49, TimelineLayout.tsx:135) — plan should list all three.

### I5. `NotificationConfig` missing `PartialEq` derive — plan A.17 adds it without ripple-effect analysis

**Location**: plan §3.3 Commit A.17 line 429 ("`PartialEq` derive required on `TrackingScheduleConfig`, `NotificationConfig` — add if missing.").

**Evidence**: `grep -nE "PartialEq" crates/oneshim-core/src/config/sections/storage.rs` returns zero. The current `NotificationConfig` struct at line 110 does NOT derive `PartialEq`. Plan's A.17 adds it in one go without auditing:
- Any callers comparing `NotificationConfig` instances today? (if zero: safe.)
- `Arc<AppConfig>` pointer equality vs content equality semantics change?
- Test fixtures constructing partial `NotificationConfig` (can they all use `Default`)?

**Impact**: Low if zero callers today, but plan skips the cross-consumer audit step per memory `feedback_cross_consumer_audit.md`. Unreliable.

**Fix**: Before A.17 lands, run `grep -rn "NotificationConfig ==\|NotificationConfig !=\|PartialEq<NotificationConfig>" crates/ src-tauri/` — if zero hits, `PartialEq` is a pure additive derive. If nonzero, plan must explicitly accept/address them. Add this verification step to A.17 in the plan.

### I6. `watch::Receiver` latest-wins coalescence conflicts with notifier transition detection in A.18

**Location**: plan §3.3 Commit A.18 line 442-446 (notifier debounce tests).

**Evidence**: `config_manager.rs:106-112` doc comment warns:
> "`watch` has latest-wins semantics: rapid mutations may be coalesced and a subscriber that wakes late will see only the final value, not every intermediate transition. Consumers whose correctness depends on observing every transition (audit-log callers, counters) must either keep a tick-based poll structure OR run every `update` through their own side-effect channel."

Plan A.18 uses `subscribe` to detect `prev_active=false → now_active=true` transitions for DesktopNotifier firing. Rapid enable→disable→enable mutations can be coalesced; the transition trigger can be missed.

**Impact**: Edge case: user rapidly toggles via REST/IPC (e.g., test scripts). Notifier silent where spec guarantees fire. 60s debounce (plan's `notifier_debounces_within_60s` test) actually exercises the opposite direction — it tests that intended duplicates are suppressed, not that missed transitions are caught.

**Fix**: Plan A.18 should use a tick-based poll (evaluate `tracking_schedule_active` every ~5s from the monitor loop) as an ADR-016-sanctioned "keep a tick-based poll" pattern — NOT the `watch::Receiver` path. Alternatively, A.18 should acknowledge that UI-initiated toggles in rapid succession (< tick interval) may miss a notifier fire and document as a non-goal. Plan's §3.8 risk register doesn't mention this; add a row.

### I7. Observability (err.code, tracing spans, counters, audit log) not specified for new surfaces

**Location**: Plan §3 through §5. Review dimension E.

**Evidence**: Existing scheduler loops follow convention `warn!(err.code = %e.code(), "…")` (CLAUDE.md, `monitor.rs:183,296`). Plan adds:
- New IPC commands (A.14, B.5) can surface errors.
- New REST handlers (A.16, B.7, C.6) can surface errors.
- New `DesktopNotifier` fires from A.18.
- New `tracking_schedule_active` gate enters/exits.
- New `BatchUploader` suppression events.
- New autostart enable/disable call outcomes.

Plan does not specify:
- Which call sites emit `tracing::info/warn/error` with `err.code` field.
- Which new counters/spans (e.g., `oneshim_tracking_schedule_active{bool}`, `oneshim_bulk_tag_operations_total{result}`, `oneshim_autostart_attempt_total{result, mechanism}`).
- Audit-log entries for tracking-schedule transitions (consent-parallel — when tracking is paused, it's a privacy-sensitive event; user may want to verify via audit log that capture was indeed silent).

**Impact**: Plan may be "done" from a test-passing angle but leave production debugging harder. Per memory `feedback_industry_convention_check.md` (OTel/Prometheus conventions), counter naming is a reviewable choice.

**Fix**: Add new §6.6 "Observability" section enumerating:
- Tracing spans: `tracking_schedule_active(enter/exit)`, `autostart_enable/disable/status`, `bulk_tag_transaction(add/remove)`.
- Counters proposed (format-bikeshed in Loop 2c review): `oneshim_tracking_schedule_state{active:bool}`, `oneshim_bulk_tag_operations_total{op:add|remove, result:ok|err}`.
- `err.code` fields on all new `warn!`/`error!` sites.
- Audit entry `TrackingScheduleTransition{prev, now, reason: "user" | "scheduled"}` — call from A.18 helper.

### I8. Plan §9 effort model missing review-cycle + CI-queue time

**Location**: plan §9.1 line 1163 ("Assuming single engineer at 6-7 effective hours/day (accounting for review iterations per memory `feedback_holistic_pre_merge_review.md`)").

**Evidence**: Plan claims 64h PR-A + 30h PR-B + 22h PR-C = 116h engineer-clock, 19 wall-clock days serial. But review cycles carry wall-clock latency:
- Per memory `feedback_3loop_yields_real_catches.md`, 3-loop review ×2-iter per phase overhead is ~30-50% for security-sensitive PRs. PR-A touches privacy-sensitive pipeline gates → qualifies.
- CI wall-clock from recent PR #486 (D13 task 13) was ~28 min per push.
- Lefthook cold-clippy ~16 min per memory `feedback_lefthook_clippy_cost.md` — plan cites this but only models "bundle commits to amortize."

Plan §9.6 contingency adds "reviewer finds post-first-draft issue requiring spec revision: +2 days per issue" but does not size total expected issue count (from spec synthesis: 46 consolidated findings surfaced in Loop 1; plan review will surface 10-20 more per industry norm).

**Impact**: Stated 19 days may be under-budgeted by 30-50%. More realistic: 26-30 wall-clock days serial, 14-18 parallel.

**Fix**: Plan §9.1 should add a line: "expected review-cycle wall-clock tail per PR: +3-5 days each for Loop 2c/2d/2e iterations" (per `feedback_3loop_yields_real_catches.md`). Also add CI queue estimate: each push = 28 min; 10 pushes per PR = 5h CI wait time (usually parallel with work but cache-invalidating pushes serialize).

---

## Minor findings

### M1. Plan bundles commits A.3/A.5/A.9 for lefthook cold-clippy — risk of too-big commits

Plan §3.3 ties bundling to cold-clippy cost. But bundling A.3 (types) + A.4 (tests) + A.5 (impl) into a single commit means 11 hours of work → hard to review, hard to bisect. Consider keeping test-commits separate from impl-commits so bisect still works; bundle only across impl-commits sharing a clippy invocation. Plan Q-plan-1 acknowledges the trade-off but picks bundling; reviewer can push back.

### M2. Plan inherits spec's OpenAPI-hand-maintained misunderstanding

Spec §6.5 line 1406-1410 claims "manifest ... (hand-maintained — no generator script exists)" and "OpenAPI contract ... must be regenerated / hand-patched". The two sentences contradict each other; plan propagated the "hand-patched" reading in §3.3 A.21. File a follow-up to correct the spec after the plan is fixed.

### M3. Plan §4 does not specify CI env-var wiring location

Plan §8.1 says "add `ONESHIM_AUTOSTART_STUB: 1` to the `env:` block of the Rust test job in `.github/workflows/ci.yml`". Plan §4.3 B.3 introduces the stub but does not name the specific CI workflow edit. Since the test job is at `ci.yml:304` (`env: RUN_HEAVY_TESTS: ...`), plan should cite the exact block and mention the job-level `env:` vs step-level `env:` choice. Step-level (per test invocation) is safer — prevents leaking into unrelated builds.

### M4. ADR-003 sub-module extraction threshold audit incomplete for autostart.rs

Plan §4.3 B.3 line 611 says "autostart.rs ≈ 549 lines; adding observer + platform guards will push it to ~650. Recommend sub-module extraction". Actual `wc -l src-tauri/src/autostart.rs` = 549 LoC confirmed. Plan recommends extraction — OK. But missing: the extraction + behavioral fix in **one commit** (B.3) is a large "refactor + fix" mixture. Memory `feedback_holistic_pre_merge_review.md` suggests splitting into pure-refactor (extraction) commit + behavioral-fix commit for easier bisect. Plan Q-plan-2 acknowledges the question; reviewer can push split.

### M5. Plan omits grpc-governance workflow from CI impact analysis

Plan §8 discusses `ci.yml`, `integrity-gates.yml`, `build-smoke.yml`. Missing: `.github/workflows/grpc-governance.yml` (runs on proto changes). Phase 9 adds no proto, so zero impact — but plan should state "grpc-governance.yml: no impact — no proto changes". Silence is ambiguous.

### M6. Plan "42 wire codes" vs workspace CLAUDE.md "41" — follow-up registered but not acted on

Plan §11.4 line 1305 says CONS-C12 → "42 wire codes (CLAUDE.md drift noted)". Memory `reference_doc_drift` is registered. Workspace `/Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/CLAUDE.md` is technically not in this repo's scope (it's the parent oneshim umbrella). Plan correctly defers. Acceptable.

### M7. Plan §7.2 test count arithmetic minor drift

Plan table §7.2 sums to "~90" but narrative §6.2 says "~75 new tests". §9.4 commit-count table also imply breakdown that adds up slightly differently. Minor but reader-confusing. Pick one number and propagate.

### M8. Plan §4.6 Snap refresh risk not promoted to Critical despite being "broken today"

Spec §4 identifies Snap (Ubuntu Snap Store) autostart as "YES — broken — Snap users must re-enable after each snap refresh". Plan §4.4 defers to follow-up per CONS-M13. Since the plan **also** lands a "Repair Autostart" button (B.5 `repair_autostart`), there's opportunity to specify explicitly that Snap users get the repair path. Plan §4.1 does say "'Repair Autostart' button cross-platform (D6)" — but the connection to Snap is indirect. Add one sentence in §4.8 risk register tying the Repair button to Snap user guidance.

### M9. Plan does not enumerate rate-limiting for `repair_autostart` IPC

IPC command `repair_autostart` (B.5 line 639) is idempotent but spammable. A misbehaving frontend could invoke it in a retry loop — each call spawns `systemctl --user enable`. Plan §4.8 risk register does not model this. Fix: add a 5-second min-interval per repair_autostart invocation (trivial — `AtomicU64 last_repair_at` + elapsed check).

---

## Dimensions checklist

### A. Platform coverage — PARTIAL PASS

| Platform concern | Coverage | Comment |
|---|---|---|
| Tracking Schedule DST/TZ | ✓ PR-A §3.3 A.2+A.5 tests chrono-tz DST | Good |
| Autostart macOS | ✓ B.1 + B.4 (`#[cfg(target_os = "macos")]`) | Gated, will NOT run in CI (C3) |
| Autostart Linux | ✓ B.1 + B.4 with `ONESHIM_AUTOSTART_STUB=1` | Runs in CI |
| Autostart Windows | ✓ B.1 with `#[cfg(target_os = "windows")]` | Gated, will NOT run in CI (C3) |
| Bulk Tag | ✓ platform-agnostic | No regression risk |
| Wayland | ✓ B.2 + B.3 (Wayland env detection) | Best-effort (acknowledged) |
| Alpine/musl/Snap | ✓ deferred per CONS-M13 + M8 Snap risk | Acceptable for Phase 9 |

### B. Effort realism — NEEDS WORK (I8)

Plan claims 19 days serial. Per memory `feedback_3loop_yields_real_catches.md` + industry-standard review-cycle tax of 30-50%, realistic is 26-30 days serial, 14-18 parallel. Lefthook cold-clippy budget (§8.6) approximately sane but only models happy path; push-cancel during review adds retries.

### C. CI implications — FAIL (C1, C2, C3)

Plan gets the integrity script wrong (C1), the OpenAPI handling wrong (C2), and silently accepts that PR-B Windows+macOS tests are CI-invisible (C3). Plan §8 is the thinnest section of the whole doc despite being most load-bearing.

### D. Failure modes — PARTIAL PASS

Plan §3.8, §4.8, §5.8 risk registers enumerate many modes but miss:
- `watch::Receiver` coalescence (I6)
- `repair_autostart` flooding (M9)
- Snap refresh user-visible breakage (M8)
- `tracking_schedule_active` config hot-reload latency (not specified — dimension I below)

### E. Observability — FAIL (I7)

Plan lacks any explicit observability section. `err.code` convention, tracing spans, counters, audit log — none specified. Plan claims "convention preserved" implicitly; reviewer demands explicit.

### F. Binary-size + runtime impact — PASS

`chrono-tz` +2.1MB documented (spec D16, plan §1 risk #5 + §3.7 risk register). Plan includes `du -sh target/release/oneshim-app` in A.1 acceptance — good. Bulk-tag SQL stress test absent (plan §7.5 defers to benchmark); MAX_BATCH_SIZE=1000 chosen without citation.

### G. Sandbox / capabilities — PARTIAL PASS

Plan §3.3 A.14 mentions `ALLOWED_KEYS` at commands/settings.rs:44. Tauri capabilities at `src-tauri/capabilities/default.json` don't currently enumerate custom commands (Tauri v2 pattern auto-allows commands via `generate_handler!`). Plan §10 Q-plan-4 asks this — good; no action needed. macOS App Sandbox: plan doesn't re-verify staying unsandboxed, but Phase 9 adds no new entitlement-requiring surface.

### H. Migration + rollout risk — PASS

`#[serde(default)]` on all new fields (A.3, A.9). No SQLite migration needed. Feature flag absent but acceptable for Phase 9 — each PR has `git revert` rollback path documented. Plan §3.7, §4.7, §5.7 rollback paths good.

### I. Stress / load — WEAK

- Bulk-tag MAX_BATCH_SIZE=1000: plan §7.5 defers benchmark, target "< 50ms on dev Mac". No actual benchmark result.
- Tracking schedule hot-reload latency: not specified. `config_manager::subscribe` is `watch::Receiver` = ~1ms. OK but should be stated.
- Autostart repair rate-limit: not specified (M9).
- Concurrent writer vs batch-tag: plan §5.3 C.1 `batch_ops_compete_with_concurrent_writer` test — only deadlock check, not perf check.

### J. Plan open-questions relevance — MIXED

Plan §10 surfaces 5 questions. From platform/risk lens:
- **Q-plan-1 (commit bundling)**: yes, CI cost risk real. Keep bundled but acknowledge bisect tradeoff.
- **Q-plan-2 (autostart sub-module)**: yes, relates to M4 split suggestion.
- **Q-plan-3 (naming drift)**: cosmetic, not CI/platform.
- **Q-plan-4 (Tauri vs REST)**: relates to sandbox (G) — OK as surfaced.
- **Q-plan-5 (landing order)**: yes, relates to risk burndown. Keep A → B → C.

---

## Open-questions disposition (platform/risk lens)

1. **Q-plan-1**: Keep bundling for cold-clippy but split A.3 (types) from A.5 (impl). Rationale: A.2 test commit is red until A.3 lands; A.5 impl is the hot-scrutiny commit — reviewers want to read it alone. Accept 1-2 extra cold runs vs bisect clarity.

2. **Q-plan-2**: Split into two commits — B.3a pure refactor (autostart.rs → sub-modules), B.3b behavior fix (non-zero exit Err + timeout + stub). Per M4. Cold clippy paid twice, but bisect works and pure-refactor commit has zero semantic risk.

3. **Q-plan-3**: Platform/risk-neutral — punt to R2 product.

4. **Q-plan-4**: Confirm frontend uses REST (client.ts) for settings UI. Tauri IPC reserved for native-only paths (none in Phase 9 settings). No sandbox impact.

5. **Q-plan-5**: Keep A → B → C. Highest-value (privacy-gate) first; autostart is Linux-only high-risk behavior fix and wants to land with clear REST path; bulk-tag lowest risk so ships last.

---

## Verdict

**FAIL** — 3 Critical + 8 Important must be resolved before Loop 3 kicks off.

The plan is overall well-structured and thorough on TDD cadence, commit boundaries, and CONS-mapping. The weaknesses are concentrated in §8 (CI) and §3/§4 platform-gating integration. Critical findings C1, C2, C3 require substantive §8 rewrite; C2 requires per-PR doc-commit language change. I1-I6 require ~4-6 sentences of targeted edits per commit description. I7 requires a new ~40-line §6.6 observability section. I8 requires a 2-line buffer update to §9.1.

Once addressed, the plan clears the platform + effort + CI + risk gate and Loop 3 can begin.

**Non-blocking recommendation**: add §12 "Phase 9 plan review 3 — platform/risk findings" summary back-ref into this doc in the final version so future readers see both plan and its R3 verdict.

---

_End of Review 3._
