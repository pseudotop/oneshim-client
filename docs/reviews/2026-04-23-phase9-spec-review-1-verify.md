# Phase 9 Spec — R1' Verification (Loop 1e)

**Date**: 2026-04-24
**Reviewer**: R1' (Architecture / Anchoring / ADR Compliance re-pass)
**Scope**: verify round-1 architecture findings fixed + scan for regressions introduced by Loop 1d rewrites
**Spec under review**: `docs/reviews/2026-04-23-phase9-quick-wins-spec.md` (1599 lines, up from 1247)
**Worktree tip**: `5618558c`
**Verdict**: **PASS (with 2 new Minor findings — 1 regression, 1 residual)**

Loop 1 architecture-axis gate **clears**. Zero R1-lens Critical remain. Zero R1-lens Important remain. 2 new Minor findings surfaced during regression scan (see Part 2) — none block Loop 2.

---

## Part 1 — Round-1 finding status

### Critical (R1.C1–C7)

| ID | Status | Evidence in revised spec |
|----|--------|--------------------------|
| R1.C1 | ✅ Fixed | §3.8 table row 4 now reads: `**No current gate** (loop runs unconditionally; should_run_now not called from this file — verified by rg active_hours\|schedule src-tauri/src/scheduler/loops/intelligence.rs → 0 matches)`. New Decision **D13** (§7) explicitly promotes to scope expansion and references U1 Option A. Rewrite matches the proposed fix. |
| R1.C2 | ✅ Fixed | §3.9 final paragraphs now cite `BatchUploader::with_health_flag(Arc<AtomicBool>) -> Self at crates/oneshim-network/src/batch_uploader.rs:74` as the true precedent (line 1553 of spec), explicitly disavows the prior `with_capture_paused` citation ("The earlier draft cited … that builder lives on Scheduler, not on BatchUploader"), and calls out the new closure shape vs. atomic-flag precedent. Verified: `with_health_flag` is at line 74 of `batch_uploader.rs`. |
| R1.C3 | ✅ Fixed | §4.1 now reads "with **9 unit tests** (lines 460-549; `grep -c '#\[test\]' src-tauri/src/autostart.rs` → 9)". §6.1 Feature 2 reads "existing **9** tests at `autostart.rs:460-548` stay passing (not 14 as a prior draft claimed)". Verified: actual count is **9**. |
| R1.C4 | ✅ Fixed | §3.8a test-migration table correctly lists **13** tests at `trigger.rs:194-435` (with all 3 schedule tests explicitly enumerated: `blocks_capture_outside_active_hours`, `allows_capture_when_schedule_disabled`, `handles_overnight_active_hours`). Line numbers 373, 398, 409 are one line off from actuals 372, 397, 408 — see **new Minor NR1** below. |
| R1.C5 | ✅ Fixed | §3.4a is new: "**Locked decision (U2 = Option C)**: Phase 9 fixes `should_run_now` to match `is_within_active_hours` wrap logic AND hoists both checks out of `SmartCaptureTrigger`". Pseudocode for post-fix `should_run_now` shown with wrap-midnight branch. §3.4 truth table now has overnight rows (rows 7–9). **D14** in §7 locks the decision. |
| R1.C6 | ✅ Fixed | §6.3 now reads "**42 locked codes** (verified: `grep -c "^[a-z]" crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` → 42). Note: the workspace CLAUDE.md text 'Wire-format contract locked at 41 codes' is stale — file as `reference_doc_drift` follow-up". Verified: actual count is **42**. |
| R1.C7 | ✅ Fixed | §2.3 now contains a dedicated "Note on current behavior-to-preserve" paragraph describing the 200-with-silent-failure today. New **D15** in §7 explicitly documents the 200→500 behavior change as intentional. New Q entry acknowledging the frontend consumer at `TimelineLayout.tsx:131-140`. |

### Important (R1.I1–I9)

| ID | Status | Evidence in revised spec |
|----|--------|--------------------------|
| R1.I1 | ✅ Fixed | §3.7 now has "Adding `chrono-tz` — Decision D16 (U5 locked to Option A)" subsection. Dependency-direction tradeoff explicitly analyzed (+2.1MB binary, ADR-001 §4 note, rejected port-in-adapter alternative with 3 reasons). Verified: `grep -c chrono-tz Cargo.toml Cargo.lock` → 0 confirms it's still a new dep. |
| R1.I2 | ✅ Fixed | §3.11a new subsection: "Tray indicator propagation — Decision D-prop (U6 locked to Option A: ADR-016 subscribe)". Rejected (b) Tauri event emit and (c) tray re-eval tick with rationale. Verified: `ConfigManager::subscribe()` exists at `crates/oneshim-core/src/config_manager.rs:113`. |
| R1.I3 | ✅ Fixed | §3.8a new subsection: "`SmartCaptureTrigger::with_schedule` refactor — Decision D17 (U7 locked to Option A: in-PR)". DI callsites enumerated (composition root, monitor.rs:200-207), all 3 schedule tests explicitly slated for migration. |
| R1.I4 | ✅ Fixed | §4.10a new subsection: "Autostart error-type upgrade — Decision D-errtype (U8 locked to Option A: defer typed upgrade)". Substring-map at handler boundary acknowledged as lossy; typed `AutostartError` tracked as follow-up. |
| R1.I5 | ✅ Fixed | §3.8 explicit helper-extraction sentence (lines 366-376 of spec): "Per CLAUDE.md monitor-loop complexity guardrail, … extract the predicate into a new file `src-tauri/src/scheduler/loops/tracking_schedule_helper.rs` (mirroring the `coaching_helper.rs` / `focus_auto_helper.rs` / `vision_helper.rs` precedents)". Verified: `wc -l src-tauri/src/scheduler/loops/monitor.rs` → 498 — guardrail violation imminent confirmed. |
| R1.I6 | ✅ Fixed | §3.4 truth table now includes two overnight rows (row 7: `22:00–06:00 [Mon..Sun]` TS at 23:00 Wed; row 8: `22:00–06:00 Mon–Fri` active_hours overnight × empty TS at 23:00 Wed referencing §3.4a + U2 latent bug). §6.1 Feature 1 test list explicitly calls out "overnight `active_hours × overnight tracking_schedule` covering CONS-C05 fix". |
| R1.I7 | ✅ Fixed | §5.9 adds "Backend page-size cap — Decision D19 (U — CONS-I08)" with `MAX_BATCH_SIZE: usize = 1000` constant, 400 rejection on overrun, test for 1001 → 400 and 1000 → 200 < 50ms. **D19** in §7 seals it. |
| R1.I8 | ✅ Fixed | §5.6 "Recommend: rename to `affected_count`. … Exact lines to edit in-PR per CONS-I09:" — three exact edit locations listed: `api-contracts/src/tags.rs:29-31`, `client.ts:579-587`, `TimelineLayout.tsx:131-140`. Verified: frontend consumer at `TimelineLayout.tsx:135` reads `data.tagged_count`. |
| R1.I9 | ✅ Fixed | §5.3 "Transaction precedent" table now distinguishes primary (`events.rs:126`) from secondary (`maintenance.rs:419`) with annotation "Separate but demonstrates multi-table transactional DELETE — different character from tags (see CONS-M18)". |

### Minor (R1.M1–M11)

| ID | Status | Evidence |
|----|--------|----------|
| R1.M1 (ScheduleConfig) | ⚠ Partial | Spec cites `monitoring.rs:58-85` (line 1544); actual struct starts at **59** (#[derive]) or **60** (`pub struct`), ends at 73; Default impl 75-86. Off by 1-2 lines. See **new Minor NR2** below — this is minor residual drift. |
| R1.M2 (Weekday) | ⚠ Partial | Spec cites `enums.rs:11-35`; actual: enum starts at 12/13, impl at 22/23, impl ends at 36. Off by 1-2 lines. Same residual drift class as NR2. |
| R1.M3 (coaching TimeRange) | ✅ Fixed | Spec now cites `coaching.rs:118-124` consistently (lines 185, 243, 1545). Verified: `grep -n "pub struct TimeRange" coaching.rs` → line 120 (struct), `#[derive]` at 119, comments at 118. The `:118-124` range covers the block. |
| R1.M4 (drop_oldest) | ✅ Fixed | Spec cites `batch_uploader.rs:136-156` (lines 445, 495); verified actual `fn drop_oldest` at 136. |
| R1.M5 (events.rs:127) | ✅ Fixed | Spec cites `events.rs:126` (multiple sites: 902, 1029, 1118, 1213, 1555); verified: `let tx = conn` starts at line 126 (the `.transaction()` call is at 127). Citing the start of statement is correct. |
| R1.M6 (maintenance.rs:420) | ✅ Fixed | Spec cites `maintenance.rs:419` (lines 903, 1556); verified: `let tx = conn` at 419, `.transaction()` at 420. Citing start of statement is correct — **note synthesis mis-said "418" but spec has 419, which is right**. |
| R1.M7 (runtime_state.rs:371) | ✅ Fixed | Spec now cites `runtime_state.rs:347-384,366,370,667-674` (§6 references line 1583), with clear semantic: `indicator_visible` at 366, `focus_mode` at 370. Verified via grep. |
| R1.M8 (fetchFrames :183-201) | ✅ Fixed | Spec cites `api/client.ts:183-198` (lines 1207, 1565); verified actual range 183-198. |
| R1.M9 (capture_status.rs) | ✅ Fixed | Spec cites `capture_status.rs:62-153` with explicit break-outs "`get_capture_status` (line 62) + `toggle_capture_pause` (line 72) + `fetch_xor` (line 76)" at line 1582. Verified: actual `get_capture_status` at 62, `toggle_capture_pause` at 72, `fetch_xor` at 76. |
| R1.M10 (Start minimized) | ✅ Fixed | §4.9 now reads "(Prior draft incorrectly cited 'Start minimized' — that toggle does not exist in the current codebase; verified via `grep -rn 'startMinimized\|minimized' crates/oneshim-web/frontend/src` → 0 hits.)" |
| R1.M11 (es/ja/zh-CN locales) | ✅ Fixed | §6.4 "i18n locale coverage — Decision D-i18n (U12 locked to Option B: defer)" explicitly enumerates 5 locales and declares English-fallback intent. |

**Part 1 summary**: 7 Critical all **Fixed**, 9 Important all **Fixed**, 9 of 11 Minor **Fixed** + 2 **Partial** (residual 1-2 line drift, downgraded from Minor to "cosmetic-acceptable" — see NR2).

---

## Part 2 — Regression scan

### 2.1 Sampled new citations (≥15 spot-checked)

| Citation in revised spec | Verdict | Evidence |
|---|---|---|
| `crates/oneshim-core/src/config_manager.rs` `ConfigManager::subscribe()` | ✅ exists | line 113 confirms `pub fn subscribe(&self) -> watch::Receiver<Arc<AppConfig>>` |
| `crates/oneshim-core/src/consent.rs:102` ConsentManager | ✅ exists | cited in §3.4 composition rule |
| `crates/oneshim-network/src/batch_uploader.rs:74` `with_health_flag` | ✅ exact | `grep -n with_health_flag` → line 74 |
| `src-tauri/src/autostart.rs:137-141` macOS enable non-zero-exit swallow | ✅ verified | fn `enable()` body confirmed to `.output()` and `map_err` only on spawn |
| `src-tauri/src/autostart.rs:389-401` Linux systemctl enable non-zero-exit swallow | ✅ verified | fn body confirms `systemctl --user enable` non-zero eaten |
| `src-tauri/src/autostart.rs:365-371` `has_systemctl` | ✅ exact | fn at 365, body ≤ 371 |
| `src-tauri/src/autostart.rs:332-350` `generate_service_file` | ✅ exact | fn signature at 332 |
| `src-tauri/src/scheduler/loops/monitor.rs:200-207` should_run_now gate | ✅ exact | `within_active_hours` at 200-203, `if within_active_hours && !capture_paused` at 207 |
| `src-tauri/src/scheduler/loops/monitor.rs:181-189` pre-gate save | ✅ verified | `save_event` + `uploader.enqueue` unconditional at 181-189 |
| `src-tauri/src/scheduler/loops/events.rs:63-92` process_interval branch | ⚠ off by ~3 | actual `process_interval.tick()` at line **60** — close but not exact |
| `src-tauri/src/scheduler/loops/events.rs:94-110` input_interval branch | ⚠ off by 1 | actual `input_interval.tick()` at line **93** |
| `src-tauri/src/scheduler/loops/events.rs:117-124` clipboard branch | ⚠ off by ~5 | actual clipboard poll at line **112**; `"clipboard change detected"` at 121 |
| `src-tauri/src/scheduler/loops/intelligence.rs:14` `spawn_analysis_loop` | ✅ exact | `pub(in crate::scheduler) fn spawn_analysis_loop` at line 14 |
| `src-tauri/src/scheduler/loops/intelligence.rs:124` `spawn_focus_analyzer_loop` | ❌ **WRONG fn name** | actual fn is named `spawn_focus_loop` at line 124 — see **NR1** |
| `src-tauri/src/scheduler/loops/intelligence.rs:160` `spawn_coaching_loop` | ✅ exact | confirmed |
| `src-tauri/src/scheduler/loops/sync.rs:15` `spawn_oauth_refresh_loop` | ✅ exact | confirmed |
| `src-tauri/src/scheduler/loops/sync.rs:87` `spawn_cross_device_sync_loop` | ✅ exact | confirmed (spec §3.8 row 12 cites sync.rs at 87, consistent) |
| `src-tauri/src/scheduler/mod.rs:429` `with_capture_paused` | ✅ exact | `pub fn with_capture_paused(mut self, flag: Arc<...>) -> Self` confirmed |
| `src-tauri/src/scheduler/mod.rs:548-571` `should_run_now` | ✅ exact | `pub fn should_run_now(config: &AppConfig)` at 548 |
| `src-tauri/src/scheduler/mod.rs:582` `should_run_when_disabled` test | ✅ exact | confirmed |
| `crates/oneshim-storage/src/sqlite/events.rs:126` tx start | ✅ exact | `let tx = conn` at 126, `.transaction()` at 127 |
| `crates/oneshim-storage/src/sqlite/maintenance.rs:419` tx start | ✅ exact | `let tx = conn` at 419, `.transaction()` at 420 |
| `crates/oneshim-web/frontend/src/i18n/locales/en.json:1343` general anchor | ⚠ ambiguous | line 1343 is `"general": "General",` but it's a **tab-label**, not the root `"general"` — 4 `"general"` keys exist. Citation is one valid "general" location but spec's intent unclear. Not a regression vs R1 (which raised it as drift already). |
| Cargo.lock chrono-tz | ✅ zero hits | confirms new dep claim |

**Citation regression count**: 1 fabricated function name (NR1), 3 line-off-by-few in events.rs branches (collective NR2), 1 ambiguous en.json anchor (pre-existing).

### 2.2 New Rust code samples — conceptual compile-check

- **§3.4a pseudocode `should_run_now` with wrap branch** — signature preserved (`pub fn should_run_now(config: &AppConfig) -> bool`), branches compile-plausible, handles `start <= end` vs. overnight branches. ✅
- **§3.8 helper extraction `tracking_schedule_active(cfg: &AppConfig, now: DateTime<Local>) -> bool`** — free fn pure arg-signature, `DateTime<Local>` requires `chrono` (already in core). ✅
- **§3.8 wrapper `tracking_schedule_active(config: &AppConfig) -> bool`** that reads `Local::now()` internally — fine. ✅
- **§3.9 `BatchUploader::with_suppression_predicate(mut self, pred: Arc<dyn Fn() -> bool + Send + Sync>) -> Self`** — closure trait-object bound with correct auto-trait bounds for `Arc<dyn T>` across threads. ✅ Consistent with Rust best-practice for cross-thread closures.
- **§3.9 `flush()` body** — `(self.upload_suppressed)()` call-through-deref of `Arc<dyn Fn>` works; `return Ok(0)` early return fine. ✅
- **§4.3 `static HAS_SYSTEMCTL: OnceLock<bool> = OnceLock::new();`** — correct Rust-stable `std::sync::OnceLock` API (since 1.70); thread-safe lazy init guaranteed. ✅
- **§5.4 `add_tag_to_frames` / `remove_tag_from_frames` storage fns** — `prepare_cached` inside scope block for borrow release, `tx.commit()` after block, correct `Result<usize, StorageError>` return shape. ✅
- **§5.6 `batch_remove_tag` handler** — fine; uses `State<StorageWebContext>` and `Json<BatchTagRequest>`. ✅

All new Rust samples pass conceptual compile check. No lifetime/trait-bound defects.

### 2.3 ADR references

| ADR | Referenced | Exists | Verdict |
|---|---|---|---|
| ADR-001 | §3 (hexagonal) + §1 (leaf crate) | ✅ `docs/architecture/ADR-001-rust-client-architecture-patterns.md` | OK |
| ADR-003 | §3.8 (directory module) + §10 references | ✅ `docs/architecture/ADR-003-directory-module-pattern.md` | OK |
| ADR-004 | §3.12 (Tauri v2) | ✅ `docs/architecture/ADR-004-tauri-v2-migration.md` | OK |
| ADR-008 | §3.9 (network resilience) | ✅ `docs/architecture/ADR-008-network-resilience-patterns.md` | OK |
| ADR-016 | §3.11a (config-change-bus subscribe) | ✅ `docs/architecture/ADR-016-config-change-bus.md` | OK |
| ADR-019 | §6.3 (wire codes) | ✅ `docs/architecture/ADR-019-error-code-infrastructure.md` | OK |

All ADRs exist at claimed paths. `ConfigManager::subscribe()` signature verified at `config_manager.rs:113` — matches ADR-016 §Decision.

### 2.4 Port trait placement

No new port traits are introduced by Phase 9. The `Arc<dyn Fn() -> bool + Send + Sync>` predicate is a **closure injection**, not a port. This is acknowledged in the spec and does not violate hexagonal principles (injection of a callable is a functional equivalent of a single-method trait; no contract testing needed).

The refactor of `SmartCaptureTrigger::with_schedule` (D17) moves schedule knowledge from the trigger to the scheduler — this **improves** hexagonal purity (the vision crate no longer imports ScheduleConfig). ✅

### 2.5 New dependencies — Cargo.lock impact

- **`chrono-tz`** — proposed placement in `oneshim-core` (D16). Cargo.lock impact: +2.1MB binary (claimed), new transitive dep graph (tzdata tables), `chrono-tz/default-tz` feature ships full IANA tzdata. Spec acknowledges the cost explicitly and rejected port-in-adapter alternative with 3 stated reasons. ✅ sufficient rationale.
- **No other new deps** proposed. `OnceLock` is in `std` since 1.70 (workspace MSRV is 1.77). ✅

### 2.6 `ConfigChangeBus` (ADR-016) subscription pattern

Verified:
- `ConfigManager::subscribe()` returns `watch::Receiver<Arc<AppConfig>>` (line 113 of `config_manager.rs`).
- Spec §3.11a subscription model says "tray task subscribes … When the tray receives a `ConfigChanged { tracking_schedule }` event, it re-renders the tooltip/icon from the new config state."
- **Minor concern**: the spec names the event type `ConfigChanged { tracking_schedule }`, but the actual ADR-016 API emits `Arc<AppConfig>` wholesale, not a diff-typed event. The tray must diff its previous snapshot against new `AppConfig` to detect the `tracking_schedule` change. Spec's wording is slightly inaccurate but the mechanism works (see spec line 533 "Filter: re-render only when the diff's `tracking_schedule` sub-tree or `notification.tracking_schedule_enabled` changes" — implicitly acknowledging the diff-based detection). Not a regression — just wording nuance.

### 2.7 `OnceLock<bool>` memoization pattern

Verified:
- `std::sync::OnceLock` is thread-safe and provides lazy init.
- Spec uses `OnceLock::new()` + implicit `.get_or_init(|| ...)` pattern — standard Rust idiom.
- `has_systemctl()` currently `pub` at `autostart.rs:365` inside the Linux module. Spec says "needs `pub(crate)` visibility bump" (line 1577). **Minor concern**: the fn is `pub` today (not `pub(crate)`) — the bump is a **downgrade**, not upgrade. But since the module itself is `#[cfg(target_os = "linux")] mod linux`, the `pub` inside linux is only `pub` relative to the module. Spec's intent ("make it callable from outside the autostart module at the autostart crate level") is served by `pub(crate)`. Acceptable.

### 2.8 `MAX_BATCH_SIZE = 1000` enforcement

- Constant defined: spec §5.9 "**adds** `const MAX_BATCH_SIZE: usize = 1000;` to the new batch handlers in `crates/oneshim-web/src/handlers/tags.rs`".
- Rejection: "If `req.frame_ids.len() > 1000`, the handler returns HTTP 400 with wire code `validation.invalid_arguments` and a structured message."
- Wire code `validation.invalid_arguments` exists in the 42-code catalog. ✅
- Test coverage: "`batch_remove_tag` with 1001 ids → 400; with 1000 ids → 200 < 50ms."
- Applied to BOTH `batch_add_tag` AND `batch_remove_tag` (both in `tags.rs`). ✅

### 2.9 Fixer's "synthesis drift" check

Fixer flagged synthesis drift: "maintenance.rs:418 vs 419, trigger.rs:193 vs 194".

Verified in revised spec:
- **maintenance.rs:419** cited consistently (spec lines 903, 1556). Actual `let tx = conn` at line 419 — **correct**.
- **trigger.rs:194** — spec cites test module at `trigger.rs:194-435` (with mod starts at 194). Verified: `#[cfg(test)]` at 193, `mod tests {` at 194. Citing **194** for "where the tests live" is correct; citing **193** for "where the cfg attribute is" is also correct. Spec's choice of 194 is fine.

Fixer correctly corrected these.

### 2.10 New Minor findings (regressions)

#### NR1. Fabricated function name: `spawn_focus_analyzer_loop` in §3.8 row 5 and §10 references

**Issue**: The revised spec introduces the function name `spawn_focus_analyzer_loop` twice:
- §3.8 row 5 (spec line 353): `intelligence.rs:124 (spawn_focus_analyzer_loop)`
- §10 References (spec line 1584): `spawn_focus_analyzer_loop`

**Actual function name** (verified): `pub(in crate::scheduler) fn spawn_focus_loop` at `intelligence.rs:124`. The name `spawn_focus_analyzer_loop` does NOT exist in the codebase. Only `spawn_focus_loop` exists.

Verification: `rg "spawn_focus_analyzer_loop\|spawn_focus_loop" src-tauri/src/` → 2 hits, both on `spawn_focus_loop`.

**Severity**: Minor (not Critical) because:
- The line number (124) is correct.
- The scheduler-loop disposition (add gate, early-continue) is correct.
- The implementer following this row will read the line and find the real function; function-name mismatch is an obvious local fix.

**Fix**: mechanical — rename `spawn_focus_analyzer_loop` → `spawn_focus_loop` in §3.8 row 5 and §10 references. No other content change.

#### NR2. Event-loop branch line numbers off by 1-5 in §3.8 rows 7-10

**Issue**: §3.8 table rows 7-10 cite event-loop branches with line ranges that are 1-5 lines off:
- Row 7 `Process snapshot events` → spec cites `events.rs:63-92`; actual `process_interval.tick()` starts at line **60**.
- Row 8 `Input activity events` → spec cites `events.rs:94-110`; actual `input_interval.tick()` at line **93**.
- Row 9 `Clipboard events` → spec cites `events.rs:117-124`; actual clipboard poll block at **112-124**.
- Row 10 `File-access events` → spec cites `events.rs:128-…`; actual file-watcher block slightly later.

**Severity**: Minor. Ranges bracket the correct branches; implementer will still find the right code block.

**Fix**: mechanical — adjust line ranges:
- Row 7: `events.rs:60-92`
- Row 8: `events.rs:93-110`
- Row 9: `events.rs:112-124`
- Row 10: `events.rs:128-…` (spec already uses ellipsis — fine, but should confirm start line)

#### Residual (from R1.M1 + R1.M2 — re-catalogued as acceptable)

ScheduleConfig and Weekday citations remain 1-2 lines off from strict struct-start lines. Spec uses ranges like `monitoring.rs:58-85` where line 58 is blank and line 59 is `#[derive]`. Acceptable range-citation convention. **Not blocking**.

### 2.11 ADR-001 hexagonal check — cross-crate refactor

The refactor in D17 (`SmartCaptureTrigger::with_schedule` removal) **improves** hexagonal purity:
- **Before**: `oneshim-vision::trigger` imports `ScheduleConfig` from `oneshim-core::config` (a config-section import — tolerable but suggests vision crate knows about active-hours).
- **After**: `SmartCaptureTrigger::new(throttle_ms)` — no schedule knowledge. The gate moves to scheduler layer where config knowledge is appropriate.

✅ Hexagonal compliance improved. No concerns.

### 2.12 AppState guardrail (3+ fields → sub-struct)

§3.10 explicitly chooses "no new atomic". The `tracking_schedule_active(&cfg)` is a pure function of config, requiring zero new `AppState` field. ✅ guardrail respected.

---

## Part 3 — Verdict

### Critical: **0** R1-lens remaining ✅

All 7 R1 Critical findings (R1.C1–C7) verified **Fixed** in the revised spec with evidence.

### Important: **0** R1-lens remaining ✅

All 9 R1 Important findings (R1.I1–I9) verified **Fixed** in the revised spec with evidence.

### Minor: **2 new** from regression scan

- **NR1**: Fabricated function name `spawn_focus_analyzer_loop` — correct fn is `spawn_focus_loop` (mechanical rename).
- **NR2**: Event-loop branch line ranges 1-5 off in §3.8 rows 7-10 (mechanical range adjustments).

These are cosmetic, non-blocking, and mechanically fixable. They do **not** constitute gate failure.

### R1.M1/M2 residual line drift — downgraded from Minor to "cosmetic-acceptable"

1-2 line drift on ScheduleConfig and Weekday is a function of whether citations include `#[derive]` attributes or blank lines. Not worth a revision round.

---

### Gate verdict: **PASS**

Zero-Critical-zero-Important gate **clears** on the architecture axis. Loop 1 architecture-axis advance is **approved**.

**Recommendation for Loop 1d author**: fold NR1 + NR2 into a post-verify polish pass (not a gate-blocking revision round). Alternatively, bundle with Loop 2 plan's "anchor re-verification" step.

**Follow-ups tracked implicitly**:
- NR1 and NR2 from this review.
- All 9 U-decisions (U1–U9) already folded into Decisions D13–D22 with user-locked options per synthesis — no pending user input from R1 lens.
- `reference_doc_drift` TODO for CLAUDE.md "41 codes" (spec §6.3).

_End of R1' verification._
