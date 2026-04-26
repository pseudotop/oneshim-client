# Phase 9 Quick Wins Spec — Review 1 (Architecture / Anchoring / ADR Compliance)

**Reviewer**: 1 of 3
**Lens**: Architecture, Code Anchoring, Drift vs Reality, ADR Compliance
**Worktree**: `/Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/Documents/vscode/__INDIVISUAL__/oneshim/client-rust/.claude/worktrees/phase9-quick-wins`
**Branch/Tip**: `feature/phase9-quick-wins` @ `5618558c`
**Spec under review**: `docs/reviews/2026-04-23-phase9-quick-wins-spec.md` (1247 lines)
**Convergence dimensions covered**: A, B, C, D, E, F, G, H, I, J

---

## 1. Executive Summary

**Pass-bar status**: **Pass-with-rewrite**. The spec is well-written, industry-grounded and has sensible decisions. But under anchor-verification pressure it drifts in several load-bearing places that would actively mislead an implementer — most notably misstating the current state of scheduler gating on the Analysis loop (§3.8), and misstating the BatchUploader "existing pattern" that it claims to mirror (§3.9). It also miscounts existing test surfaces (autostart: claims 14, actual 9; trigger: claims 8, actual 13) which matters for the test-migration plan.

- **Critical findings**: 7 (block planning until addressed)
- **Important findings**: 9 (must address before Loop 2 plan)
- **Minor findings**: 11 (nits / polish)

The plan-layer reviewer and impl-layer reviewer should not proceed on §3.8 (gate integration points table) or §3.9 (upload-defer "mirrors existing pattern" claim) until the rewrites land.

---

## 2. Critical findings (C1–C7)

### C1. §3.8 table: "Current gate" column for Analysis loop is fabricated

**What's wrong**: Row 4 of the §3.8 gate-integration table claims the Analysis loop (`src-tauri/src/scheduler/loops/intelligence.rs`) currently has `should_run_now(&cfg)` as its gate. This is false.

**Evidence**:
- `rg -n "should_run_now" src-tauri/src/` returns exactly 3 hits: `monitor.rs:203` (only consumer), `mod.rs:548` (definition), `mod.rs:584` (test). No hit in `intelligence.rs`.
- `rg -n "active_hours|schedule" src-tauri/src/scheduler/loops/intelligence.rs` returns zero matches.
- The analysis/intelligence loop currently runs unconditionally on its tick.

**Why this is Critical**: The spec's Phase-9 addition column says "Extend `should_run_now()` helper or add sibling `tracking_schedule_active()`" — but there is nothing to extend because the analysis loop has never called `should_run_now()`. An implementer following this row would:
1. Search `intelligence.rs` for `should_run_now`,
2. Not find it,
3. Either give up (and leave the analysis loop ungated), or
4. Introduce an active_hours gate on the analysis loop as a "pre-requisite," which is a new behavior decision never discussed in §3.

**Proposed rewrite**: Change §3.8 row 4 to:

> | Analysis loop (LLM) | `src-tauri/src/scheduler/loops/intelligence.rs::spawn_analysis_loop` (line 14) | **No current gate** — the loop runs on every tick | New: add `tracking_schedule_active(&cfg.get())` check at loop-body entry; early-return (`continue`) when true |

And update the "Chosen" in §3.1 to explicitly acknowledge that Phase 9 adds a schedule gate to the analysis loop where previously there was none. (This is a scope expansion that deserves reviewer attention — possibly a D13-style decision: "is LLM analysis suppressed during tracking-schedule windows? yes/no".)

---

### C2. §3.9 "mirrors existing `with_capture_paused` pattern at `scheduler/mod.rs:429-430`" — the named pattern is on the Scheduler builder, not BatchUploader

**What's wrong**: The spec claims `BatchUploader::with_suppression_predicate` "mirrors the existing `with_capture_paused(Arc<AtomicBool>)` pattern at `src-tauri/src/scheduler/mod.rs:429-430`". Verified reality:
- `scheduler/mod.rs:429` is `Scheduler::with_capture_paused`, a builder method on `Scheduler`, not `BatchUploader`.
- `rg -n "with_capture_paused|capture_paused" crates/oneshim-network/src/batch_uploader.rs` returns **zero hits**. `BatchUploader` has no `capture_paused`-anything.

**Evidence**:
- `crates/oneshim-network/src/batch_uploader.rs:74` is `with_health_flag(mut self, flag: Arc<AtomicBool>) -> Self` — the actual closest existing builder pattern.
- `crates/oneshim-network/src/batch_uploader.rs:79,85` are `with_dynamic_batch` and `with_max_queue_size` respectively.

**Why this is Critical**: An implementer reading "mirrors the existing pattern" will look for a capture_paused flag inside BatchUploader, fail to find it, and then be unclear whether the suppression predicate is the first such flag or one among many. The entire design argument that "the crate stays free of AppConfig dependency" also needs to be evaluated against `with_health_flag` (which is the actual precedent).

**Proposed rewrite**: Replace the last sentence of §3.9 with:

> This mirrors the existing `BatchUploader::with_health_flag(Arc<AtomicBool>) -> Self` builder at `crates/oneshim-network/src/batch_uploader.rs:74` (which exposes a gating flag for the circuit-breaker), and keeps the `BatchUploader` crate free of any `AppConfig` dependency.

And explicitly note: the tracking-schedule predicate differs from `health_flag` in that it is a closure (not an `Arc<AtomicBool>`), which is worth calling out because the spec is introducing a **new shape** of injected dependency into this crate.

---

### C3. Test count drift: `src-tauri/src/autostart.rs` has 9 tests, not 14

**What's wrong**: §4.1 claims "complete platform implementation … with **14 unit tests**." Actual count via `grep -c "#\[test\]" src-tauri/src/autostart.rs` is **9**.

**Evidence**: The 9 tests are at lines 468, 480, 487, 501, 512, 519, 528, 535, 543. No more exist.

**Why this is Critical**: §6.1 says "existing 14 tests at `autostart.rs:460-548` stay passing. Add: …" A reviewer or implementer counting against an inflated baseline will assume they've broken 5 tests when they've only broken 0. Test-health gate decisions depend on the baseline being accurate.

**Proposed rewrite**: §4.1 → "**9 unit tests** (lines 468–548)". §6.1 → "existing 9 tests at `autostart.rs:460-548` stay passing." Also double-check the spec author wasn't counting sub-module tests nested inside `#[cfg(target_os = "…")]` blocks; if so, the 14 count must be sourced from `cargo test -p oneshim-app autostart` output (which the spec should cite), not from `grep`.

---

### C4. Test count drift: `oneshim-vision/src/trigger.rs` has 13 tests, not 8

**What's wrong**: §3.8 says "Implementation should verify this refactor does not break the **eight existing trigger unit tests** at `trigger.rs:207-435`". Actual count via `grep -c "#\[test\]" crates/oneshim-vision/src/trigger.rs` is **13**, with 3 (not 2) schedule-specific tests:
- `blocks_capture_outside_active_hours` (line 373)
- `allows_capture_when_schedule_disabled` (line 398)
- `handles_overnight_active_hours` (line 409)

**Evidence**: Tests span lines 208–435, not 207–435. Test module `#[cfg(test)] mod tests` starts at line 193, not 207.

**Why this is Critical**: The refactor proposed in §3.8 ("hoisting both checks to the scheduler loop and simplifying `SmartCaptureTrigger` to no longer know about time-of-day at all") would migrate *three* schedule tests to scheduler-side, not two. Missing a test in the migration is how regressions ship. Also the spec's claim of migrating 2 specific tests leaves `allows_capture_when_schedule_disabled` in an orphaned state (it tests schedule-disabled-path, which would need equivalent coverage at the new site).

**Proposed rewrite**: §3.8 final paragraph → "verify this refactor does not break the **13 existing trigger unit tests at `trigger.rs:193-435`**; three of those tests (`blocks_capture_outside_active_hours`, `allows_capture_when_schedule_disabled`, `handles_overnight_active_hours`) exercise schedule logic directly and would migrate to scheduler-side unit tests."

---

### C5. `should_run_now` does not handle overnight windows — silently disagrees with `SmartCaptureTrigger::is_within_active_hours`

**What's wrong**: Spec §3.4 and §3.8 treat `should_run_now(&cfg)` and `SmartCaptureTrigger::is_within_active_hours(hour, weekday)` as semantic equals. They are not.

**Evidence**:
- `scheduler/mod.rs:571`: `hour >= schedule.active_start_hour && hour < schedule.active_end_hour` — no wrap-midnight branch.
- `oneshim-vision/src/trigger.rs:71-77`: `if start <= end { /* normal */ } else { /* overnight: hour >= start || hour < end */ }` — wrap-midnight supported.

A user who configures `active_hours_enabled = true` with `active_start_hour = 22` and `active_end_hour = 6` will:
- Have `SmartCaptureTrigger::should_capture` permit capture at 23:00 (overnight logic).
- Have `should_run_now` **return false** at 23:00 (22 ≥ 6 is false, the "normal" branch fails).
- And the monitor loop at `monitor.rs:201-207` uses `should_run_now`, so capture will NOT happen at 23:00 despite `should_capture` saying it should.

This is a pre-existing latent bug, but the Phase-9 spec is amplifying it:
1. Spec §3.8 "recommends hoisting both checks to the scheduler loop and simplifying `SmartCaptureTrigger` to no longer know about time-of-day at all" — but `should_run_now` is the lesser-featured of the two implementations. If Phase-9 goes with the "hoist + simplify trigger" path, we lose the overnight-active-hours feature unless `should_run_now` is fixed first.
2. Spec §3.4 worked-example row 4 ("`09:00–18:00 Mon–Fri` + ts `[{12:00–13:00 Mon–Fri}]`") is correct only because 12:00 is a non-overnight range. An overnight `active_hours` example is conspicuously absent from the §3.4 truth table.

**Why this is Critical**: The spec promises overnight support for `TrackingWindow` (§3.7, good) but relies on a helper (`should_run_now`) that does not offer overnight support for the positive gate it composes with. The composition `active_hours_gate AND NOT tracking_schedule_active` is broken for overnight-active-hours users.

**Proposed rewrite**: Add §3.4a "Overnight active_hours is not supported by `should_run_now` today" with an explicit choice:
- **Option A**: Phase-9 fixes `should_run_now` to match `is_within_active_hours` (add wrap-midnight branch). Migrates `monitor.rs:201-207` to use the fixed version. This is a latent-bug fix riding alongside the feature.
- **Option B**: Phase-9 documents the limitation and leaves the fix to a separate PR. New tracking-schedule users with overnight active_hours get surprising behavior.
- **Option C**: Phase-9 chooses A and also migrates `is_within_active_hours` out of `SmartCaptureTrigger` (per the "hoist" recommendation), deleting the overnight duplication.

The spec needs to pick one. Currently it straddles.

---

### C6. Wire-contract snapshot count is 42, not 41

**What's wrong**: §6.3 and the referenced `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` — spec says "41 locked codes"; actual count via `grep -c "^[a-z]" crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` is **42**.

**Evidence**: The file contains 42 canonical wire codes. Also present but not mentioned in the spec: `not_found.resource_missing`, `consent.expired`, `consent.required`, `provider.bedrock.unsupported`.

**Why this is Critical**: The number 41 is also carried in the workspace CLAUDE.md ("Wire-format contract locked at **41 codes**"). Both are stale. Since the spec's §6.3 conclusion is "**No new wire codes are required for Phase 9.**" this count error does not change the conclusion, but:
- ADR-019 §7 ties wire-code additions to an 8-step checklist and to the snapshot test. A reviewer whose internal model says "41" will miss a possible 42nd code whose meaning matters for the spec's mapping table.
- The CLAUDE.md guidance ("41") has already drifted from reality. This spec is an opportunity to catch/fix it.

**Proposed rewrite**: §6.3 → "Checked against `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` (**42 locked codes**, not 41 as referenced by older docs — file this as a `reference_doc_drift` follow-up). **No new wire codes are required for Phase 9.**" Re-verify the spec's 6 listed mappings against the actual 42-row catalog to be safe (I verified in review — all 6 are present).

---

### C7. `handlers/tags.rs:83-98` actual behavior is even worse than the spec describes — it silently swallows per-row errors with `warn!`

**What's wrong**: §2.3 and §5.3 correctly flag the absence of a transaction on batch-add, but under-report the current handler's behavior.

**Evidence** (`crates/oneshim-web/src/handlers/tags.rs:83-97`, read today):

```rust
pub async fn batch_add_tag(...) -> Result<Json<BatchTagResponse>, ApiError> {
    let mut tagged_count = 0u32;
    for frame_id in &req.frame_ids {
        match TagsCommandService::new(context.clone()).add_tag_to_frame(*frame_id, req.tag_id) {
            Ok(_) => tagged_count += 1,
            Err(e) => {
                tracing::warn!("batch tag: frame {} failed: {}", frame_id, e);
            }
        }
    }
    Ok(Json(BatchTagResponse { tagged_count }))
}
```

- The handler **always returns 200 OK** regardless of per-row failures.
- Failed rows are logged with `warn!` and counted silently.
- The response shape (`{ tagged_count }`) does NOT tell the caller which rows failed.
- There is no `Err`-return path; the loop is infallible to the caller.

This is strictly worse than the spec's description of "partial-success risk": the current handler actively *hides* errors from the frontend.

**Why this is Critical**: The refactor in §5.5 proposes a bubble-up `?` on `add_tag_to_frames`, meaning the handler will now **start returning 500 for what was previously a silent success**. This is a behavior change that must be called out in:
1. The decisions log (§7) — "D-new: transactional refactor flips 200-with-silent-failure → 500-with-error".
2. The open questions (§9) — "Q-new: do any current consumers rely on batch_add always returning 200?".
3. The frontend changelog — the `useMutation` at `TimelineLayout.tsx:131-140` currently fires `onSuccess` for every batch call; post-refactor it will fire `onError` in cases that previously fired `onSuccess`.

**Proposed rewrite**: §2.3 last paragraph → Add sentence: "Worse still, the current handler catches per-row errors with `tracing::warn!` and always returns 200, so the frontend has no signal that any row failed. The transactional refactor (§5.5) is a behavior change from 'silent partial success' to 'explicit all-or-nothing'; §7 and §9 need a new entry to reflect this."

---

## 3. Important findings (I1–I9)

### I1. `chrono-tz` dependency decision is an oneshim-core hexagonal concern — should not be a Q, should be a D

§3.7 notes "Adding [chrono-tz] is a deliberate trade: +2.1MB binary size" and §9 Q1 asks the reviewer to sign off. But this is not just a binary-size question — it is an **architectural question** under ADR-001 §4 (crate dependency direction).

- `oneshim-core` is the leaf crate. Every adapter crate and the binary depend on it.
- Adding `chrono-tz` to `oneshim-core` pulls tz data into every dependent, which is ~1.5MB compressed per tz db snapshot (+rust structures).
- The alternative is placing `chrono-tz` in an adapter crate (e.g., `oneshim-monitor` or a new `oneshim-time` helper crate) behind a `TimezoneResolver` port.

**Recommendation**: Either keep `chrono-tz` out of `oneshim-core` (define `TrackingScheduleConfig::timezone: String` in core, and resolve via a port whose impl lives in an adapter), or accept the core-dep and document a trade-off entry. The spec currently does the latter implicitly — it belongs in §7 Decisions log as D-new, not §9 Q1.

**Evidence**: `grep -n "chrono-tz\|chrono_tz" Cargo.toml Cargo.lock` returns zero — confirming no pre-existing workspace use.

### I2. ADR-016 (config-change-bus) is referenced but not used

§3.10 explicitly chooses the "poll-per-tick" approach over ADR-016 `subscribe()`. This is defensible (the monitor loop already dereferences config per tick), but the spec does not surface the choice as a tradeoff. ADR-016's key positive — "wake-up on change" — is not needed for tracking_schedule because the schedule is clock-driven and will re-evaluate on the next tick (1s monitor tick ≈ 1s max latency).

However: §3.12 REST + IPC endpoints (`PUT /api/tracking-schedule`) mutate config. If a user configures a window that *starts right now* via PUT, the monitor loop will pick it up on the next tick — but the tray indicator (§3.11 "**Tray**: add a tray state 'Tracking Scheduled' … Recommended: reuse icon, change tooltip") needs to update *immediately* for the user. The spec does not specify how the tray learns. One of:
- Tauri event emit on `set_tracking_schedule`.
- Tray re-evaluates on its own interval (tray.rs does not have a re-evaluate tick today).
- Subscribe to the config-change-bus per ADR-016.

**Recommendation**: Add §3.11a "Tray indicator propagation" — specify the mechanism. ADR-016 subscribe is the cleanest fit for tray (the tray already runs its own task).

### I3. `SmartCaptureTrigger::with_schedule` constructor signature change is a cross-crate breaking change

§3.8 recommends "hoisting both checks to the scheduler loop and simplifying `SmartCaptureTrigger` to no longer know about time-of-day at all." Current signature:

```rust
pub fn with_schedule(throttle_ms: u64, schedule: ScheduleConfig) -> Self  // trigger.rs:37
```

Callsite: `src-tauri/src/app_runtime_launch.rs` or similar DI wiring (spec does not cite exactly). The refactor removes `schedule` from this signature, which affects:
- Every DI wiring path that constructs a trigger.
- Any third-party consumer (if any) — likely none given this is a workspace-internal crate.
- The three schedule-related tests in `trigger.rs` (C4).

Q4 in §9 flags this tension ("in-scope for the same PR or sequenced as a follow-up?"). **Recommendation**: upgrade Q4 to a Decision D-new because the sequencing affects Loop 2 plan structure and Loop 3 test-migration plan. If in-scope, the spec needs an explicit §3.8a "SmartCaptureTrigger constructor migration" subsection listing every DI wiring callsite.

### I4. Autostart module returns `Result<T, String>` — ADR-019 typed code mapping is lossy

`src-tauri/src/autostart.rs::{enable_autostart, disable_autostart, is_autostart_enabled}` all return `Result<_, String>`. Spec §4.10 maps failure sources → wire codes, but the error enum mapping is on the receiving end (IPC handler / REST handler) based on substring matching of the String. This is exactly the anti-pattern ADR-019 was created to eliminate.

**Recommendation**: Either
- (a) Leave autostart as-is and accept the substring-match mapping (current spec), OR
- (b) Upgrade autostart to return a typed error (`AutostartError` enum in autostart.rs with typed `code: AutostartCode` per ADR-019 §7). (b) is the right long-term fix but adds scope.

The spec should state the choice explicitly. §4.10's mapping table is ambiguous about where the mapping happens (handler or module) and by what mechanism (substring? error kind?).

### I5. `scheduler/loops/monitor.rs` is at 498 lines — adding the tracking_schedule check pushes it past the 500-line guardrail

The guardrail is in CLAUDE.md ("Monitor Loop Complexity: `spawn_monitor_loop` in `scheduler/loops/monitor.rs` must stay under 500 lines"). Current size: 498. Proposed addition: the `tracking_schedule_active` call at `monitor.rs:207` plus any required config-field access adds ~3–5 lines minimum.

**Recommendation**: §3.8 should add a sentence naming the helper extraction (mirror `coaching_helper.rs`, `focus_auto_helper.rs`, `vision_helper.rs` precedents). Name proposed: `tracking_schedule_helper.rs` with a free function `evaluate_tracking_schedule(&cfg) -> bool`. This keeps the spawn_monitor_loop body at <500 lines.

### I6. Scheduler `should_run_now` does not pass unit tests for the proposed "active_hours AND NOT tracking_schedule" truth table

§3.4 worked-example row 4 expects: `{active_hours 09-18 Mon-Fri}` + `{ts 12-13 Mon-Fri}` + now = 12:30 Mon → "no (suppressed by TS)". 

This depends on `should_run_now(cfg)` returning `true` at 12:30 Mon (because 12 is within 9-18) — which it does for non-overnight.

But §3.4 row 8 is not currently tested as a truth-table row:
- `{active_hours 22-06 Mon-Fri}` (overnight) + `{ts 00-04 Mon-Fri}` + now = 01:00 Tue.

`should_run_now(cfg)` with start=22 end=6 returns `false` at 01:00 (see C5 above). So the combined predicate is `false && NOT tracking_schedule_active = false`. The tracking-schedule eval is *skipped*, meaning the capture_permitted decision is the same as today's bug.

**Recommendation**: §3.11 test strategy — unit test `capture_permitted_now` must include overnight active_hours rows. Coverage of C5's latent bug is an implicit part of Phase-9 correctness unless deferred.

### I7. Q3 (backend page-size cap) is a real issue, spec correctly identifies it but should upgrade to decision

Spec §Q3: "Is there a backend-side max? Needs confirmation."

Verified via `crates/oneshim-web/src/handlers/frames.rs:12-18` (`get_frames`) — the handler does **not** enforce a limit cap. `TimeRangeQuery` is deserialized directly and passed through. A malicious or confused client can request `limit=50000`.

For Phase-9, this matters because the batch tag/untag endpoints will receive `frame_ids` arrays from user-triggered selection UI. If the frontend upstream paginates at 50, the batch is 50. But the REST endpoints (`POST /api/frames/batch-tags` + new `DELETE /api/frames/batch-tags`) accept any `Vec<i64>`. A third-party scripted caller can send 100000 IDs.

**Recommendation**: Upgrade Q3 to D-new: add `MAX_BATCH_SIZE: usize = 1000` constant in `oneshim-web/src/handlers/tags.rs`, reject with `400 validation.invalid_arguments` when exceeded. Spec already suggests this but leaves it as a question.

### I8. `BatchTagResponse` rename to `affected_count` is a real breaking change — frontend uses `data.tagged_count` today

Spec §D8-alt calls this "backward incompatible for the response field — but this is a local contract not external API; only the frontend consumes it."

Verified consumer: `crates/oneshim-web/frontend/src/pages/timeline/TimelineLayout.tsx:131-140`:
```typescript
const batchTagMutation = useMutation({
  ...
  onSuccess: (data) => {
    ...
    addToast('success', t('timeline.batchTagged', { count: data.tagged_count }))
  },
```

The `data.tagged_count` lookup is the only consumer — the rename is mechanical but *must* happen in the same PR as the server change. Spec's §6.4 i18n keys already call for `timeline.batchUntagged: "{{count}} frames untagged"` which implies a parallel `data.removed_count` or `data.affected_count` read.

**Recommendation**: Change §5.6 → explicitly list the three lines that need edit (client.ts:579-587 `batchAddTag` return type, TimelineLayout.tsx:131-140 onSuccess handler, plus any new `batchRemoveTag` addition). No surprise edits during impl.

### I9. The `ALL_TABLES` transaction precedent at `maintenance.rs:420` is different in character from tags

§5.4 cites both `events.rs:127` (per-row INSERT OR IGNORE in transaction) and `maintenance.rs:420` (whole-table DELETE loop). Only `events.rs:127` is the analogous precedent; `maintenance.rs:420` is a table-scoped DELETE pattern (different use case). Citing both muddles the pattern justification.

**Recommendation**: §5.4 "Transaction precedent" cell should cite only `events.rs:127` or annotate `maintenance.rs:420` as "separate but demonstrates multi-table transactional DELETE".

---

## 4. Minor findings (M1–M11)

### M1. Line-range drift — `ScheduleConfig`

Spec claim: `monitoring.rs:57-86`. Actual: ScheduleConfig struct at 58-73 (16 lines). Default impl at 75-85. Comment separator at 57.
**Fix**: `monitoring.rs:58-85` or `58-73` (struct only).

### M2. Line-range drift — `Weekday`

Spec claim: `enums.rs:12-36`. Actual: `Weekday` enum at 11-20; `impl Weekday` at 22-35.
**Fix**: `enums.rs:11-35`.

### M3. Line-range drift — `CoachingConfig::TimeRange`

Spec claim: `coaching.rs:119-125`. Actual: `TimeRange` struct at 118-124 (off by 1, `#[derive]` line included).
**Fix**: `coaching.rs:118-124`.

### M4. Line-range drift — `batch_uploader.rs` `drop_oldest`

Spec claim: "`batch_uploader.rs:135-156`". Actual: `drop_oldest` at 136-156.
**Fix**: `135-156` → `136-156`.

### M5. Line-range drift — `events.rs:127` (transaction)

Spec repeatedly cites `events.rs:127`. Actual `conn.transaction()` is at `events.rs:126`.
**Fix**: `events.rs:126`.

### M6. Line-range drift — `maintenance.rs:420`

Spec cites `maintenance.rs:420`. Actual `let tx = conn.transaction()` at `maintenance.rs:418`.
**Fix**: `maintenance.rs:418`.

### M7. Line-range drift — `runtime_state.rs:371`

Spec claim: "`runtime_state.rs:371,magic_overlay_driver.rs`". Actual `indicator_visible` at line 366, `focus_mode` at 370. 371 is the middle of the focus_mode block.
**Fix**: `runtime_state.rs:366` for indicator_visible.

### M8. Line-range drift — `fetchFrames` range 183-201

Spec claim: `api/client.ts:183-201`. Actual `fetchFrames` at 183-198.
**Fix**: `183-198`.

### M9. Line-range drift — `capture_status.rs:48-153,76`

Spec claim: `capture_status.rs:48-153`. Actual: `get_capture_status` starts at line 62, `toggle_capture_pause` at 72. Line 48 is near the top of an unrelated block.
**Fix**: `capture_status.rs:62-153`. Line 76 is the `fetch_xor` call inside `toggle_capture_pause`, not the fn signature.

### M10. GeneralTab lacks "Start minimized" — reference is incorrect

§4.9 says "`GeneralTab` already hosts app-lifecycle toggles ('Start minimized', 'Check for updates at startup')". Verified: `GeneralTab.tsx` has ToggleRows at lines 126, 133 for **`updateEnabled`** and **`updateAutoInstall`**. No "Start minimized" anywhere in `GeneralTab.tsx` or the broader frontend (verified via `grep -rn "startMinimized\|minimized" crates/oneshim-web/frontend/src` — zero hits).
**Fix**: §4.9 → "`GeneralTab` already hosts update-lifecycle toggles ('Check for updates', 'Auto-install updates') as well as the ScheduleSettings section. Autostart belongs to the same app-level lifecycle mental category."

### M11. Missing i18n locales — es, ja, zh-CN also exist

Spec §6.4 provides en/ko keys. Actual locales directory has 5 files: `en.json`, `ko.json`, `es.json`, `ja.json`, `zh-CN.json`. New keys added only to en/ko will fall back to English in the other three locales, creating an i18n gap.
**Fix**: §6.4 → acknowledge that adding the keys only to en/ko triggers a fallback-to-English in es/ja/zh-CN, and specify whether the spec requires translations now or defers them to a separate i18n PR.

---

## 5. Verified anchors table

Every file:line citation from the spec, verified against `5618558c`.

| Spec citation | Status | Notes |
|---|---|---|
| `crates/oneshim-core/src/config/sections/monitoring.rs:57-86` ScheduleConfig | ⚠️ drift | ScheduleConfig at 58-73 (struct) + 75-85 (Default) |
| `crates/oneshim-core/src/config/sections/coaching.rs:119-125` TimeRange | ⚠️ drift | TimeRange at 118-124 (off by 1) |
| `crates/oneshim-core/src/config/enums.rs:12-36` Weekday | ⚠️ drift | Weekday enum at 11-20, impl at 22-35 |
| `crates/oneshim-core/src/config/mod.rs:41` schedule field | ✅ | confirmed |
| `crates/oneshim-core/src/config/mod.rs:113` default_config | ✅ | confirmed (schedule: ScheduleConfig::default() on that line) |
| `crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` 41 codes | ❌ | **42 codes**, not 41 (see C6) |
| `crates/oneshim-vision/src/trigger.rs:26-30` SmartCaptureTrigger struct | ✅ | struct at 26-30 |
| `crates/oneshim-vision/src/trigger.rs:52-78` is_within_active_hours | ✅ | fn at 52-79 |
| `crates/oneshim-vision/src/trigger.rs:69-77` overnight wrap logic | ✅ | if/else at 69-77 |
| `crates/oneshim-vision/src/trigger.rs:138-148` should_capture gate | ✅ | gate at 138-148 |
| `crates/oneshim-vision/src/trigger.rs:207-435` tests (8 tests) | ❌ | 13 tests at 193-435 (see C4) |
| `crates/oneshim-network/src/batch_uploader.rs:96-156,185-197,199` | ⚠️ partial | flush() at 199 ✅; drop_oldest at 136-156 (not 135-156); compute_batch_size at 185-197 ✅ |
| `crates/oneshim-storage/src/sqlite/tags.rs:8-52` get_tag_ids_for_frames | ✅ | fn at 8-52 |
| `crates/oneshim-storage/src/sqlite/tags.rs:152-186` add/remove | ⚠️ partial | add_tag_to_frame at 152-166; remove_tag_from_frame at 168-186 |
| `crates/oneshim-storage/src/sqlite/events.rs:127` conn.transaction() | ⚠️ drift | actual line 126 |
| `crates/oneshim-storage/src/sqlite/maintenance.rs:420` | ⚠️ drift | actual line 418 |
| `crates/oneshim-storage/src/migration/v01_v08.rs:186-212` tags schema | ⚠️ partial | migrate_v5 at 180-215; schema-chunk is within |
| `crates/oneshim-web/src/routes.rs:117` batch-tags POST | ✅ | confirmed |
| `crates/oneshim-web/src/routes.rs:122-125` | ⚠️ drift | DELETE single-frame is at line 125 only (122-125 includes add_tag_to_frame POST) |
| `crates/oneshim-web/src/handlers/tags.rs:73-98` | ⚠️ drift | remove_tag_from_frame at 74-81; batch_add_tag at 83-97 |
| `crates/oneshim-web/src/handlers/tags.rs:88-97` loop | ✅ | see C7 — behavior worse than spec describes |
| `crates/oneshim-web/src/services/tags_service.rs:91-114` | ⚠️ drift | add_tag_to_frame service at 91-98; remove at 100-114 |
| `crates/oneshim-api-contracts/src/tags.rs:3-32` DTOs | ✅ | file is 32 lines; BatchTagRequest at 23-26; BatchTagResponse at 29-31 |
| `crates/oneshim-web/frontend/src/pages/timeline/TimelineLayout.tsx:62,111,113-114,116-128,202-204,256-271` | ✅ | all verified |
| `crates/oneshim-web/frontend/src/pages/timeline/AllFrames.tsx:170-181,242-284,302-331,582-603` | ✅ | floating action bar at 582-604 |
| `crates/oneshim-web/frontend/src/api/client.ts:183-201,579-588` | ⚠️ partial | fetchFrames at 183-198 (not 201); batchAddTag at 579-587 (not 588) |
| `crates/oneshim-web/frontend/src/i18n/locales/{en,ko}.json:157,26,1336` | ⚠️ drift | timeline at 157 ✅; nav at 24-26 ✅; general at 1343 (not 1336) |
| `src-tauri/src/autostart.rs:1-549` | ✅ | file is 549 lines |
| `src-tauri/src/autostart.rs:4` allow(dead_code) | ✅ | confirmed |
| `src-tauri/src/autostart.rs:8,31,54` public fns | ✅ | all three confirmed |
| `src-tauri/src/autostart.rs:331-348` generate_service_file | ✅ | fn at 332-350 |
| `src-tauri/src/autostart.rs:343` Environment=DISPLAY=:0 | ✅ | confirmed |
| `src-tauri/src/autostart.rs:365-371` has_systemctl | ✅ | fn at 365-371 |
| `src-tauri/src/autostart.rs:404-417` XDG fallback | ✅ | confirmed |
| `src-tauri/src/autostart.rs:460-549` 14 tests | ❌ | **9 tests** total; module at 460-548 (see C3) |
| `src-tauri/src/autostart.rs:92-95,325-329,188-190` current_exe | ⚠️ drift | actual single-line calls at 93, 189, 326 (not 3-line ranges) |
| `src-tauri/src/autostart.rs:177-178` SUBKEY, VALUE_NAME | ✅ | confirmed |
| `src-tauri/src/commands/mod.rs:1-21` command modules | ✅ | 21 modules confirmed |
| `src-tauri/src/commands/capture_status.rs:48-153` | ⚠️ drift | get_capture_status at 62, toggle_capture_pause at 72 |
| `src-tauri/src/runtime_state.rs:347-384` AppState struct | ✅ | struct at 347-384 |
| `src-tauri/src/runtime_state.rs:667` capture_paused init | ✅ | confirmed |
| `src-tauri/src/runtime_state.rs:371` (indicator) | ⚠️ drift | indicator_visible at 366; focus_mode at 370 |
| `src-tauri/src/scheduler/loops/monitor.rs:58,200-207,292` | ✅ partial | line 58 capture_paused clone ✅; 200-207 gate ✅; 292 end of guard ✅ |
| `src-tauri/src/scheduler/mod.rs:429-430,548-571` | ✅ | with_capture_paused at 429; should_run_now at 548-571 |
| `src-tauri/src/scheduler/mod.rs:582` test should_run_when_disabled | ✅ | confirmed |
| `src-tauri/src/scheduler/loops/intelligence.rs` (has should_run_now) | ❌ | **no should_run_now anywhere** (see C1) |
| `src-tauri/src/tray.rs:181,207,241` capture_paused | ✅ | all three confirmed |

**Summary**: 29 ✅, 14 ⚠️ drift, 4 ❌ fabricated.

---

## 6. ADR compliance checklist

| ADR | Compliance | Notes |
|---|---|---|
| **ADR-001 §1 error strategy** | ⚠️ | Autostart module returns `String`, not typed error. Spec works around at boundary. See I4. |
| **ADR-001 §2 async trait** | N/A | Spec adds no port traits. |
| **ADR-001 §3 DI** | ✅ | The proposed `Arc<dyn Fn() -> bool>` suppression predicate in §3.9 is not a port, but it's a closure injection, which is acceptable; not `Arc<Mutex<Box<dyn T>>>`. |
| **ADR-001 §5 testing** | ✅ | Spec proposes manual mocks (Vitest for frontend, inline `#[cfg(test)]` for Rust). |
| **ADR-003 directory module (500 lines)** | ⚠️ | monitor.rs at 498 will push over with spec additions. autostart.rs already at 549 (single file) unchanged. See I5. |
| **ADR-004 Tauri v2** | ✅ | Tauri command signatures use `#[command]`, `State<'_, T>`, `Result<_, IpcError>`. Consistent. |
| **ADR-008 network resilience** | ✅ | Spec respects existing backoff / queue patterns; doesn't touch. |
| **ADR-016 config-change-bus** | ⚠️ | Spec uses poll-per-tick, not subscribe. Defensible for scheduler loops, but tray propagation (§3.11) is unspecified. See I2. |
| **ADR-017 feedback-signal-sink** | N/A | Not touched. |
| **ADR-019 typed error codes** | ⚠️ | Wire-contract count is 42, spec says 41 (C6). No new wire codes proposed (good). Autostart module is lossy (I4). |
| **ADR-037/040 event sourcing** | N/A | No event emission introduced. |

---

## 7. Open questions for user

Decisions the spec left ambiguous under the architecture/anchoring lens — these need user input before Loop 2:

1. **U1 (C1 follow-on): Does Phase-9 add a new `tracking_schedule` gate to the Analysis (LLM) loop, where previously there was no schedule gate at all?** If yes, is this surfaced as a deliberate scope expansion for §3 / §7? If no, row 4 of §3.8 should be struck.
2. **U2 (C5 follow-on): Does Phase-9 fix the latent `should_run_now` overnight bug as part of the feature (Option A from C5), document it as known limitation (Option B), or fix it and hoist it out (Option C)?**
3. **U3 (I1 follow-on): Accept `chrono-tz` in `oneshim-core` as a workspace-wide dep, or place behind a port in an adapter crate?**
4. **U4 (I3 follow-on): Is the `SmartCaptureTrigger::with_schedule` constructor refactor in-scope for Phase-9 or split to a follow-up? This affects the Loop 2 plan structure.**
5. **U5 (I4 follow-on): Is autostart error-type migration (String → typed `AutostartError`) in-scope, or do we accept substring-mapping at the boundary for Phase-9?**
6. **U6 (I8 follow-on): `BatchTagResponse { tagged_count }` → `{ affected_count }` rename is a frontend breaking change. Confirm in-PR rename acceptable (vs. two sibling DTOs)?**
7. **U7 (M11 follow-on): New i18n keys added to en/ko only — do es/ja/zh-CN translations block the PR, or defer to a separate i18n PR?**
8. **U8 (§3.11 / I2): Tray-indicator propagation mechanism — emit Tauri event on set_tracking_schedule, subscribe to ADR-016 bus, or add a tray re-evaluate tick?**

---

## 8. Convergence dimensions covered

- A. Code-anchor drift — 47 anchors verified (§5). 14 drift, 4 fabricated.
- B. Hexagonal architecture — passed except I1 (core dep direction) and I4 (autostart typed error).
- C. ADR-003 — I5 (monitor.rs 500-line guardrail) flagged.
- D. ADR-019 wire codes — C6 (snapshot count 42 not 41) flagged.
- E. ADR-016 config-change bus — I2 (tray propagation unspecified) flagged.
- F. Dep / crate boundary — I1 (chrono-tz in core) flagged.
- G. Concurrency — §3.10 chooses no new atomic (sound); I5 helper extraction recommended.
- H. Port-contract tests — no new ports added; existing sibling tests at `sqlite/tests.rs:378-420` not referenced by spec.
- I. Event sourcing — N/A.
- J. Monitor loop complexity — I5 (over 500 with additions).

---

_End of review. Hand off to Reviewers 2 (product/UX/GDPR/industry) and 3 (cross-platform/test/risk)._
