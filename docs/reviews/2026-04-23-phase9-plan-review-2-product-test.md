# Phase 9 Plan Review 2 — Product + Test + Rollback

**Reviewer**: 2 of 3
**Lens**: Product completeness, test strategy, rollback safety, regulatory preservation
**Date**: 2026-04-24
**Plan under review**: `docs/reviews/2026-04-23-phase9-quick-wins-plan.md` (1353 lines)
**Spec anchor**: `docs/reviews/2026-04-23-phase9-quick-wins-spec.md` (1599 lines, 22 Decisions locked)
**Synthesis anchor**: `docs/reviews/2026-04-23-phase9-spec-review-synthesis.md` (46 consolidated findings)

---

## Summary
- **Critical**: 2
- **Important**: 6
- **Minor**: 7

Zero-Critical-Zero-Important gate **FAILS** on 2 Critical + 6 Important. Plan is not ready for Loop 2 clearance. Recommended rewrite scope: ~40 lines targeted patches in §3.3 (PR-A helper signature), §3.3 (A.2 test set), §3.3 (A.8 test scope), §6.2 vs §7.2 math, §3.7 risk register add.

---

## Critical findings

### PR2-C01. `capture_permitted_now` helper drops the `consent × capture_paused` composition — the GDPR transparency guarantee is weakened

- **Lens**: regulatory preservation, product completeness, rollback safety
- **Plan location**: §3.3 commit A.5 (line 197-207); composite gate wrapper; §3.3 commit A.9 (line 282-297) which wires the call sites.
- **Spec anchor**: §3.4 (composition rule), §3.4b (consent top-authority conflict-resolution table). The spec's composition rule is:
  ```
  capture_allowed(now, tier) = consent_granted(tier)
                            AND active_hours_gate(now)
                            AND NOT tracking_schedule_active(now)
                            AND NOT capture_paused
  ```
- **What the plan ships**: `capture_permitted_now(cfg, now) := should_run_now_with_time(cfg, now) && !tracking_schedule_active(cfg, now)` — 2 of 4 required gates.
- **What's missing**:
  1. No `consent_granted(tier)` term. The spec explicitly states consent is **top authority** (no other gate can re-permit capture when consent is revoked). Plan's helper silently drops it.
  2. No `!capture_paused` term. The plan assumes callers will AND it in separately at the call site (A.7 line 246 does `if capture_permitted_now && !capture_paused`). But the **9 newly gated call sites in A.9** (analysis/focus/coaching/events/input/clipboard/file-access/cross-device-sync/audio) use the composite gate **without** composing `capture_paused` in — see A.9 line 293: `if crate::scheduler::tracking_schedule_active(&cfg.get()) || !crate::scheduler::should_run_now(&cfg.get()) { continue; }`. That's `!TS || !active_hours`, which omits **both** consent and capture_paused from the new sites.
- **Product impact**: user toggles "pause capture" from tray → 9 of 13 newly gated pipelines keep running (analysis, focus, coaching, process events, input events, clipboard events, file-access events, cross-device-sync, audio). User sees a paused tray but analysis still ticks on their last events; GDPR Art. 13/14 transparency requirement is violated silently.
- **Consent regression**: if consent is revoked mid-session, the 9 newly gated pipelines keep running because they don't consult `ConsentManager`. Spec §3.4.b says "consent revocation is always top-authority" — plan breaks this.
- **Rollback risk**: post-merge a reviewer discovers the consent gap; the fastest revert is A.9 (un-gates the 9 new loops), which loses the D13 scope-expansion gains. This is a bad trade.
- **Required fix**:
  1. Add `consent_granted(tier, cfg)` and `!capture_paused` to `capture_permitted_now`. The helper must compose all 4 terms per spec §3.4.
  2. The 9 new call sites in A.9 must use the composite gate, not the 2-term `tracking_schedule_active || !should_run_now` shortcut.
  3. `ConsentManager` is in `oneshim-core/src/consent.rs:102`. Thread an `Arc<ConsentManager>` through to the scheduler helper or expose via `AppConfig`.
  4. Add a truth-table test to A.4 covering the 4-term composition with consent revoked (top authority) vs consent granted.
  5. Each A.8 integration test must run with consent in a consent-granted state; add one more sanity test `consent_revoked_top_authority_overrides_ts_inactive` — consent revoked + TS inactive + active_hours true → capture still blocked.

Severity is Critical because this is the single-biggest product-contract gap: the spec's Art. 5/13/14/25 grounding depends on the composition rule being fully honored, and the plan's helper signature makes it impossible for the 9 new gate sites to comply.

### PR2-C02. PR-A test-count inconsistency — three different totals across §3.3 A.21, §6.2, and §7.2 (product-quality health gate broken)

- **Lens**: test strategy completeness, docs deliverables
- **Three locations disagree**:
  - **§3.3 commit A.21 (line 483)**: "PR-A adds ~28 new tests" — but the same sentence's breakdown is A.2=8 + A.4=9 + A.6=3 + A.8=13 + A.10=3 + A.13=5 + A.15=4 + A.18=4 + A.19=7 = **56**. The prose says 28 and the itemization sums to 56.
  - **§6.2 (line 998)**: "PR-A adds ~28; PR-B adds ~20; PR-C adds ~27. Total Phase 9: ~75 new tests".
  - **§7.2 table (line 1079-1082)**: PR-A 42 + PR-B 21 + PR-C 27 = **90 tests**.
- **Impact**:
  1. `docs/STATUS.md` test-count bump cannot be committed correctly — the plan doesn't specify the real number. STATUS.md is the single source of truth for test counts; drift is a known recurring issue (CLAUDE.md has a whole "STATUS.md is the single source of truth" section).
  2. PR review gating: "All tests pass" becomes meaningless if the expected count is ambiguous. Reviewers will discover the discrepancy on the first PR and bounce it back.
  3. The confusion happens exactly in the "docs/follow-ups registration" commit (A.21), where the test-count update is the most scrutinized line.
- **Root cause**: plan-writer revised the count in some sections (from ~40 → ~90 reflecting D13 scope) but didn't propagate the revision consistently. The "~28" figure appears to be a stale pre-D13 estimate.
- **Required fix**:
  1. Pick the authoritative count. Based on the breakdown, the correct PR-A count is either **56** (sum of A.2/A.4/A.6/A.8/A.10/A.13/A.15/A.18/A.19) or **~42** per §7.2 itemization. Reconcile.
  2. Update §3.3 A.21 line 483 to match.
  3. Update §6.2 total to match §7.2 or vice versa.
  4. Add a cross-reference check in the plan's meta/final checks section.
  5. If `ts_inactive_allows_events` (A.8 line 271), `heartbeat_loop_continues_during_ts` (line 277), and `oauth_refresh_loop_continues_during_ts` (line 278) are counted as "pipeline gates", the gate count is 13; if they are counted as sanity tests only, the gate count is 10. Clarify in §7.2 "13 scheduler gating" explicitly.

Severity is Critical because it's a zero-gate-blocker in the spirit of CONS-C11 (test-count drift blocks health gates). Three divergent numbers in one plan document indicates the review hasn't been internally coherent.

---

## Important findings

### PR2-I01. §3.9 "pre-flush drain at window-entry" is silently dropped from the plan — potential data loss on long windows

- **Lens**: feature completeness vs spec, rollback safety
- **Spec anchor**: §3.9 clause 4 ("Pre-flush drain at window-entry (long-window overflow protection)"): if the uploader queue approaches `max_queue_size` during a long suppression window, the `drop_oldest()` fallback at `batch_uploader.rs:136-156` kicks in and silently drops pre-window events. Spec proposes a scheduler-side `on_window_boundary_approaching` hook that flushes JUST BEFORE TS flips on.
- **Plan coverage**: **none**. Grep for "pre-flush drain", "on_window_boundary_approaching", "long-window overflow" in plan → **zero hits**. A.11 (suppression predicate) and A.12 (DI wiring) only implement the flush gate during the window — not the pre-flush drain.
- **Impact**:
  1. Product: a user configures a 10-hour overnight TS window; during that time the upstream gates are closed (good), but any events already in the uploader queue at window entry may exceed `max_queue_size` over 10 hours if flush is blocked, causing `drop_oldest` to silently drop them. Data loss affects pre-window (legitimately captured) events.
  2. Rollback: absent from plan means absent from in-PR follow-up registration. The TODO will never get created.
- **Spec says it's optional** ("deferred to follow-up if complexity is too high"), so this isn't a Critical. But the plan must either:
  1. Explicitly defer it and register the follow-up in `project_next_tasks.md` (add to A.21 follow-up bullet list), OR
  2. Include it as commit A.11.5 / A.12.5 with its own test.
- **Recommended**: explicit deferral with a registered follow-up. Add a bullet to §3.4 "PR-A cross-cutting docs / Follow-up TODOs" (line 498).

### PR2-I02. Clock-irregularity tests (§3.7a) partially covered — suspend-across-window + forward-clock-jump-past-window-end have no test

- **Lens**: test strategy completeness
- **Spec anchor**: §3.7a "Clock irregularities" — 5-row table of suspend/skew behaviors.
- **Plan coverage**:
  - Backward clock jump within 60s → A.18 `notifier_debounces_within_60s` (line 444) — OK.
  - Suspend crossing a window boundary → **no test**. Spec says "both notifications missed" is accepted behavior. Plan should assert this explicitly (pass test: `suspend_across_window_gate_returns_true_across_period`).
  - Forward clock jump past window end (skip entire window) → **no test**. Spec says "accepted — user-self-inflicted".
  - Forward clock jump INTO a future window → **no test**.
  - Forward clock jump past window end (gate correctly resumes) → **no test**.
- **Impact**: if a future refactor breaks suspend/resume handling, there's no regression detection. GDPR DPIA documentation says "hour-boundary granularity acceptable" — but "DST-passed-through-suspend correctly suppresses" is an audit-defensible claim that needs a test.
- **Recommended**: add 3 more pure-fn tests to A.4 (they are all 2-arg `(cfg, now)` composable without a mock clock):
  1. `window_active_across_suspend` — assert `tracking_schedule_active(cfg, t1=11:59)` + `tracking_schedule_active(cfg, t2=13:05)` — both evaluate `true` for a window [12:00, 13:00] because t1=11:59 is pre-window and t2=13:05 is post-window. Actually that test is for the boundary case — revise to test that `tracking_schedule_active(cfg, t_during_window=12:30)` is true regardless of whether the scheduler ticked between suspend and resume.
  2. `forward_clock_jump_into_future_window` — `tracking_schedule_active` returns true after the jump.
  3. `forward_clock_jump_past_window_end` — returns false after jump.
- Minor effort (+30 min), substantial GDPR-audit value.

### PR2-I03. A.2 test set misses `config.invalid` (invalid IANA timezone) + empty-end + end-before-start validation cases

- **Lens**: test strategy completeness, regulatory-grounded commitments
- **Spec anchor**: §6.3 error codes table — `config.invalid` for "tracking_schedule.timezone is not a known IANA name"; `validation.invalid_field` for "end or days_of_week empty".
- **Plan coverage**: A.2 has `dst_fall_back_fires_twice`, `dst_spring_forward_window_in_skipped_hour_never_fires`, `empty_days_never_active`, but **no** test for:
  - Invalid IANA name (`"Foo/Bar"` → should fail validation at `set_tracking_schedule`).
  - Empty `end: ""` (malformed window config).
  - `end < start` on a same-day window (vs overnight).
  - Malformed HH:MM (`"25:61"` or `"12:XX"`).
- A.13 has `ipc_error_on_invalid_hhmm_format` (line 369) — partial coverage at the IPC layer, but the underlying config serde layer has no test. If `TrackingWindow` deserialization silently accepts garbage strings, the whole validation chain is fragile.
- **Recommended**: add to A.2:
  1. `serde_rejects_invalid_hhmm` — parse config JSON with `"start": "25:00"` → `serde_json::Error` or custom validation error.
  2. `serde_rejects_invalid_iana_timezone` — parse with `"timezone": "Foo/Bar"` → post-parse validation returns error.
  3. `window_with_empty_end_is_invalid`.
  4. `window_end_before_start_not_same_day_is_invalid` (or document overnight interpretation).

### PR2-I04. A.8 `ts_inactive_allows_events` sanity test is too weak to catch false-negatives on 4 event variants

- **Lens**: test strategy completeness
- **Plan location**: A.8 line 271.
- **Plan text**: `ts_inactive_allows_events — sanity: TS window not yet active, events do arrive`.
- **Issue**: one sanity assertion covers five event variants (Window/Process/Input/Clipboard/FileAccess). If a broken gate over-suppresses ONE variant (e.g., only Window events), this single test may still pass because "events do arrive" is satisfied by any single variant appearing.
- **Recommended**: split into 5 sanity tests paired with the 5 suppression tests:
  - `ts_inactive_allows_window_events`
  - `ts_inactive_allows_process_events`
  - `ts_inactive_allows_input_events`
  - `ts_inactive_allows_clipboard_events`
  - `ts_inactive_allows_file_access_events`
- Effort: +5 tests to A.8 (currently 13). Brings A.8 total to 18 and makes PR-A test count more accurate.
- Also refine the plan's variant enforcement: each suppression test asserts `COUNT(*) = 0` for its variant; each sanity test asserts `COUNT(*) > 0` for its variant.

### PR2-I05. Autostart Repair button trigger condition is under-specified — `needs_repair` flag not contractually defined

- **Lens**: product completeness, rollback safety
- **Spec anchor**: §4.7 — "Trigger the button only when `is_autostart_enabled() == true` AND the recorded path doesn't match `current_exe()`".
- **Plan location**:
  - B.5 line 639: `pub async fn repair_autostart() -> Result<AutostartStatus, IpcError> — calls enable_autostart() idempotently` — impl is correct.
  - B.5 line 641: `pub struct AutostartStatus { enabled: bool, mechanism: String, fallback_used: bool }` — **no `needs_repair` field**.
  - B.8 line 682: `repair_button_appears_when_path_mismatch — stub /status returning fallback_used: false but a needs_repair: true extension flag (reuse existing flags if any; otherwise add a simple stale-path detection)` — "reuse existing flags if any; otherwise add" is hand-waved.
- **Issue**: the AutostartStatus contract in B.5 doesn't declare `needs_repair`; the frontend test assumes it exists. If plan intends to add `needs_repair` but omits it from the struct, the test fails. If plan intends NOT to add it and use `fallback_used` as a proxy, the frontend expectation is wrong (fallback_used means "using xdg_desktop" — orthogonal to "binary moved").
- **Required clarification**:
  1. Add `pub needs_repair: bool` to `AutostartStatus` in B.5 line 641.
  2. Define "needs_repair" computation in the handler: `is_autostart_enabled() && !recorded_path_matches_current_exe()`.
  3. Require B.5 (Tauri command) + B.7 (REST) to compute `needs_repair` consistently across platforms. On Linux, check the `ExecStart=` line in the unit file. On macOS, parse the plist. On Windows, read the registry value.
  4. Add a backend test to B.4/B.6: `needs_repair_true_when_recorded_path_differs_from_current_exe`.
- **Rollback implication**: if `needs_repair` logic is wrong (false positives on Flatpak sandbox paths, for instance), rolling back B.5 alone reverts the IPC surface; frontend repair button becomes dead code. Low risk but better to define upfront.

### PR2-I06. Frontend consumer of D15 (200→500 behavior change) under-tested — only one test, no error-path coverage

- **Lens**: rollback safety, test strategy
- **Plan location**: C.4 line 857: `change data.tagged_count → data.affected_count in onSuccess; add onError handler to surface HTTP 500 (the D15 behavior change) as a toast (i18n key timeline.batchTagError — add to en.json + ko.json)`.
- **Issue**: only one frontend test covers this behavior change. D15 is flagged as the **highest coordination risk in PR-C** per plan §1 and §5.3. The existing TimelineLayout.tsx:131-140 consumer change is the single point where silent-200-partial → explicit-500 error can leak into user experience.
- **Required coverage**:
  1. Vitest test: mutation returns 500 with wire code `storage.failed` → `onError` fires, toast shows localized `timeline.batchTagError` message.
  2. Vitest test: mutation returns 500 with wire code `validation.invalid_arguments` (batch > 1000) → different toast message.
  3. Vitest test: mutation succeeds with `affected_count: 0` (all frames already tagged) → success toast with count "0 frames newly tagged" (not an error).
  4. Playwright E2E (C.9): "select 5 frames, attempt batch-add during forced-error scenario → 500 → toast → selection clears? or stays? Verify UX".
- C.7 has `remove_tag_toast_shows_x_of_n_format` but that's for the remove flow. Add 3 tests for the add-error path.
- Reasonable to bundle into C.7 or extend C.4 test commit.

---

## Minor findings

### PR2-M01. Follow-up registration location ambiguous — "project_next_tasks.md OR follow-ups.md"

- §3.4 line 498: `Follow-up TODOs registered in project_next_tasks.md (or a new docs/follow-ups.md if the file doesn't exist — verify)`. The memory MEMORY.md indicates `project_next_tasks.md` exists as an active project-context file.
- Plan should commit to `project_next_tasks.md` (existing) rather than leave an either/or. Verify and update.

### PR2-M02. Korean i18n term "추적 일정" enforcement is only a grep check in A.20, not an automated lint

- §3.3 A.20 line 471: `grep for "스케줄" → 0 hits in added strings`.
- This is a manual spot-check. Biome lint won't catch. Consider adding a tiny script or CI step. Low priority (the grep check is acceptable for quick-wins).

### PR2-M03. `heartbeat_loop_continues_during_ts` and `oauth_refresh_loop_continues_during_ts` sanity tests (A.8 lines 277-278) claim "row 14" and "row 15" of §3.8 — but §3.8 is 16 rows

- Plan text is accurate for heartbeat (row 14) and OAuth (row 15) but the plan prose in §3.3 A.9 line 297 says "row 15 of §3.8" for oauth_refresh which matches.
- §3.8 row 16 (metrics/process/aggregation/notification — ungated) has no sanity test. Consider adding `metrics_loop_continues_during_ts` for consistency.
- Low priority; the ungated loops are less PII-sensitive.

### PR2-M04. Tray rendering test `tray_diff_detects_tracking_schedule_change` (A.17) is a synthetic test — the real user-visible behavior (icon + tooltip swap) stays manual-QA only

- A.17 line 432: `Add a focused test tray_diff_detects_tracking_schedule_change that exercises the diff logic only (purely synchronous); full tray re-render stays manual QA`.
- The propagation mechanism (ADR-016 subscribe) + the tooltip swap is a core user-facing acceptance criterion, but no automation. Acceptable for quick-wins but flag for follow-up (Playwright/Tauri-native E2E).
- Register as follow-up: tray E2E integration test.

### PR2-M05. AllFrames.tsx "Remove tag" popover test (C.7 line 896) only covers the content (show all tags) and the selection behavior (popover lists, click fires), but NOT error-path rendering when batchRemoveTag returns 500

- C.7 has `remove_tag_toast_shows_x_of_n_format` (happy path toast) but no `remove_tag_toast_shows_error_on_500`.
- Minor: the rendering path is smaller than the add path (fewer user-facing surfaces), but the 500 path still merits one test per parallel with the add path.

### PR2-M06. `en.json` anchor line 1343 (per spec §6.4) — plan B.9 line 689 says "Anchor to general-tab section (en.json:1343)" — acceptable

- Plan correctly cites spec-corrected line. No drift. Keep vigilance through impl — drift since the spec was written could change it.

### PR2-M07. `tracking_schedule_helper.rs` helper lives under `scheduler/loops/` but exposes `pub(crate) fn` — plan commit A.4 line 181 registers it as `mod tracking_schedule_helper;` without re-export.

- Plan says "do not re-export yet (caller will use the path)". This is OK, but inconsistent with the sibling helpers (`coaching_helper.rs`, `focus_auto_helper.rs`, `vision_helper.rs`) which are re-exported.
- Minor style point. Either match the sibling pattern or document why this one differs.

---

## Dimensions checklist (A–J)

### A. Feature completeness vs spec

**Tracking Schedule (spec §3)**:
- Multi-range-per-day support (D22) — **covered** (A.3 struct definition, A.2 serde tests).
- Overnight handling (§3.4 truth table) — **covered** (A.2 `overnight_window_wraps` test, A.5 `should_run_now` fix).
- DST semantics (§3.7) — **covered** (A.2 `dst_fall_back_fires_twice`, `dst_spring_forward_window_in_skipped_hour_never_fires`).
- Upload-defer FIFO + pre-window-cap (§3.9) — **partial** (suppression predicate in A.11; pre-flush drain missing — PR2-I01).
- Tray indicator propagation via ADR-016 (§3.11) — **covered** (A.17).
- Consent × schedule composition (§3.4) — **missing** (PR2-C01).
- IPC + REST surface (§3.12) — **covered** (A.13-A.16).
- Settings UI frontend component — **covered** (A.19-A.20).

**Autostart IPC wiring (spec §4)**:
- Enable/Disable/Is-enabled/Repair IPC — **covered** (B.5).
- REST endpoints GET/PUT /api/autostart + POST /api/autostart/repair — **covered** (B.7).
- Err on non-zero exit (3 platforms) — **covered** (B.3).
- 5s timeout wrapper (3 platforms) — **covered** (B.3).
- `ONESHIM_AUTOSTART_STUB=1` env-var stub — **covered** (B.3).
- `OnceLock<bool>` memoization — **covered** (B.3).
- Error-copy mapping (wire codes → translation keys) — **covered** (B.5 map_autostart_error + B.9 i18n keys).
- Settings UI toggle — **covered** (B.9).
- Repair button trigger condition — **under-specified** (PR2-I05).

**Timeline Bulk Tag (spec §5)**:
- Transactional add/remove — **covered** (C.2).
- POST refactor (200→500) — **covered** (C.4).
- DELETE /api/frames/batch-tags — **covered** (C.6).
- `affected_count` rename — **covered** (C.4).
- `MAX_BATCH_SIZE=1000` — **covered** (C.3, C.4, C.6).
- OpenAPI + manifest — **covered** (C.10).
- Frontend remove-tag popover + toast — **covered** (C.7, C.8).
- E2E in timeline-actions.spec.ts — **covered** (C.9).

**Scorecard**: 2 gaps (consent composition, pre-flush drain).

### B. Test strategy completeness

- Test-first ordering — **compliant** (each `feat:` commit has a paired `test:` commit with red tests landing first per TDD).
- Unit + integration + E2E balance — **compliant** (~50 unit + ~20 integration + ~18 frontend unit + ~2 E2E per §7.2).
- `serial_test` usage — **compliant** (A.8 integration, B.11 autostart integration).
- Fixtures + mocks specified — **mostly compliant** (U3 Option B 2-arg pure fns; `SqliteStorage::new_in_memory()`; `ONESHIM_AUTOSTART_STUB=1`). Weak point: A.8 sanity tests (PR2-I04).
- Pure fn `tracking_schedule_active(cfg, now)` — **compliant**.
- Batch-tag transaction rollback tests (CONS-I16) — **compliant** (C.1 all 9 tests).
- DST fixtures (US/Eastern) — **compliant** (A.2 has 2 DST tests).
- 13 pipeline gate integration tests — **compliant** (A.8 has 13 tests — but see PR2-I04 for sanity-test weakness).
- Autostart stub tests (Linux CI) — **compliant** (B.1, B.4).
- Additional testing gaps: consent composition (PR2-C01), clock-irregularity completeness (PR2-I02), config validation edge cases (PR2-I03), D15 error path (PR2-I06).

### C. Rollback + feature-flag safety

- Per-task rollback paths — **documented** for each PR (§3.7, §4.7, §5.7).
- Default `enabled: false` enforcement — **compliant** (A.2 test, A.3 impl).
- Bulk-tag 200→500 behavior change + frontend patch bundled — **compliant** (C.4 does both in one commit).
- Autostart Err failure mode + UI fallback — **compliant** (Repair button in B.9; but see PR2-I05 for needs_repair contract gap).
- Config migration via `#[serde(default)]` — **compliant**.
- No task marked "cannot be rolled back" — **compliant**.
- **Rollback risk cluster**: PR2-C01 makes A.9 rollback damaging (consent composition fix scattered across multiple commits).

### D. Regulatory-grounded commitments preserved in plan

- 13 pipeline-gate tasks — **compliant** (A.8 has 13 tests; A.9 implements 13 gates).
- DPIA-adjacent docs — **partially compliant** (§6.3 PHASE-HISTORY.md entry mentions GDPR Art. 5, 13/14, 25, 35 but doesn't cite spec §2.1 Art. 17 delineation OR CCPA/CPRA/state-acts scope boundaries).
- User-facing tracking-schedule tray indicator + notification — **compliant** (A.17 tray + A.18 notifier, notification on by default).
- Notifications ON during window (§2.1 transparency) — **compliant** (A.18 with 60s debounce; default `tracking_schedule_enabled=true`).
- **Consent composition missing (PR2-C01)** — this is the regulatory hole. GDPR Art. 13/14 transparency at collection means "user is informed about suppression", but if consent is revoked, ALL capture should stop — the plan's composite gate misses this.

### E. i18n coverage

- New keys in en + ko only — **compliant** (A.20, B.9, C.8).
- Korean term consistency "추적 일정" — **compliant** (A.20 line 471 grep check; see PR2-M02).
- Fallback notes for es/ja/zh-CN — **compliant** (§3.4 follow-up bullet line 500 registers).

### F. Documentation deliverables

- STATUS.md test-count bump — **listed** but inconsistent (PR2-C02).
- PHASE-HISTORY.md Phase 9 entry — **drafted** in §6.3 (plan includes the actual markdown content — good practice).
- CLAUDE.md updates — **listed** (§6.1 covers both worktree-level and workspace-level; workspace-level correctly deferred per §6.1 line 992).
- Follow-up registration — **listed** (§6.4 enumerates 8 follow-ups; but PR2-I01 adds a 9th: pre-flush drain).

### G. Frontend deliverables

- Settings UI Tracking Schedule section — **covered** (A.20).
- Autostart toggle in GeneralTab — **covered** (B.9).
- Timeline multi-select + "Remove tag" popover — **covered** (C.8).
- Tray tooltip update mechanism — **covered** (A.17).
- Error-copy mapping uses existing i18n — **covered** (B.9, A.20).

### H. User-facing acceptance criteria

- Concrete user-facing criteria per feature — **partially compliant**. Each PR section has acceptance criteria (§3.5, §4.5, §5.5) but these are **CI command gates**, not behavioral ACs. For example, §3.5 says `cargo test --workspace` passes; it doesn't say "user configures TS → tray shows 'Tracking paused' → capture stops → window ends → capture resumes".
- Edge-case ACs (overnight, DST, hot-reload) — only implied via unit tests. Not called out as ACs.
- **Recommendation**: add a "User-visible acceptance flow" bullet to each PR's acceptance criteria section (§3.5, §4.5, §5.5). 3-5 lines per feature.

### I. 22→90 test count jump audit

- §6.2 says "~75 new tests" — **inconsistent with §7.2's ~90** (PR2-C02).
- Breakdown per §7.2 table: PR-A=42, PR-B=21, PR-C=27. Sum=90.
- Per §3.3 A.21 breakdown, PR-A=8+9+3+13+3+5+4+4+7=56. But §3.3 line 483 says 28.
- No test counted twice obvious to this review.
- D13 scope expansion (13 scheduler-loop integration tests in A.8) is the primary driver — **verified** — it matches the 13-row §3.8 table.

### J. Plan open-questions product relevance

Five open questions (§10). Product lens:

1. **Q-plan-1 (Commit bundling)** — plan internal; OK to defer.
2. **Q-plan-2 (autostart sub-module extraction B.3)** — architectural, R1 owns. From product lens: splitting autostart.rs into 4 files (mod/linux/macos/windows/test_observer) during this PR inflates reviewer surface and is a scope creep. Product-side recommendation: **defer sub-module extraction** to a follow-up cleanup PR and keep autostart.rs as a single 650-line file for Phase 9. The ADR-003 threshold is 500-600; 650 is tolerable short-term. Splitting introduces new module paths in 4 files + visibility changes + re-export patterns that will be re-reviewed later anyway when the typed `AutostartError` upgrade (D-errtype follow-up) happens. Defer = lower risk.
3. **Q-plan-3 (NotificationConfig.tracking_schedule_enabled naming)** — plan's Alternative B (accept spec's chosen name despite sibling drift) is correct from product lens. The user-facing control is in Settings UI i18n keys, not the field name. Keep Alternative B.
4. **Q-plan-4 (Tauri IPC vs REST consumer)** — from product lens, **confirm REST-only consumption** for both PR-A and PR-B features. The Settings UI is the sole surface; using Tauri IPC would require a Tauri-native Settings window (which doesn't exist per spec). All 6 IPC commands (3 tracking_schedule + 3 autostart) can still be registered for future native-Settings-window consumers, but frontend must not depend on Tauri IPC when the web-dashboard path is available. Plan's current choice (REST from frontend) is correct.
5. **Q-plan-5 (PR-A vs PR-B order)** — product lens: **A→B→C** is correct. Tracking Schedule has the biggest user-visible GDPR story; landing it first maximizes trust signal. PR-B's autostart is credibility-win but lower-urgency. PR-C's bulk-tag is dominant-support-feedback-request but least regulatory.

---

## Open-questions disposition (5 items from product lens)

| ID | Product-relevant? | Defer or decide now? | Rationale |
|----|-------------------|----------------------|-----------|
| Q-plan-1 | No | Defer | Plan internal; reviewer can request split per-review if needed. |
| Q-plan-2 | Low | Defer; keep single file in Phase 9 | ADR-003 threshold 500-600; 650 is tolerable; deferral avoids speculative refactor. |
| Q-plan-3 | No (field-name drift invisible to user) | Accept Alternative B per spec | i18n copy is what users see; field name is internal. |
| Q-plan-4 | Low | Confirm REST-only | Frontend is web dashboard; Tauri IPC path is a future-native-Settings concern. |
| Q-plan-5 | Yes | Keep A→B→C | Tracking Schedule is the GDPR-headline win; ship first for trust-signal. |

---

## Verdict

**FAIL**

The plan gates fail on:
- **2 Critical** blockers (consent composition gap in `capture_permitted_now`; test-count inconsistency across §3.3 A.21 / §6.2 / §7.2).
- **6 Important** gaps (pre-flush drain missing, clock-irregularity test gaps, serde validation test gaps, A.8 sanity-test weakness, `needs_repair` contract under-specified, D15 error-path test coverage).

The plan has strong structural quality — TDD discipline is strict, commit ordering is sound, rollback paths are documented per PR, the 3-PR split is well-justified. The spec's 22 Decisions are faithfully translated into commits.

But the Critical gaps are regulatory-preservation issues that the spec specifically flagged (PR2-C01 breaks §3.4 composition rule and §3.4.b consent top-authority; PR2-C02 propagates CONS-C11 test-drift into the plan itself).

**Required before Loop 2 clearance**:
1. Fix PR2-C01: redefine `capture_permitted_now` to include consent + capture_paused; rewrite A.9 gate sites to use the full 4-term composite; add a consent-top-authority integration test.
2. Fix PR2-C02: reconcile the 28/42/56 PR-A test count discrepancy and propagate to §6.2, §7.2, A.21 line 483.
3. Address PR2-I01 (pre-flush drain) via explicit follow-up registration in §3.4.
4. Extend A.4 with 3 clock-irregularity tests (PR2-I02).
5. Extend A.2 with 4 serde validation tests (PR2-I03).
6. Split A.8 sanity test into per-variant tests (PR2-I04).
7. Define `needs_repair` in AutostartStatus (PR2-I05).
8. Extend C.7 with 3 error-path tests for D15 (PR2-I06).

Expected post-fix: 0 Critical, 0 Important remaining. Minor findings are policy matters and can be deferred.

Estimated rework cost: ~3h plan edits + ~2h spec consult. No new Decision reopening needed (all fixes are implementation-plan-level, not spec-level).

---

_End of review._
