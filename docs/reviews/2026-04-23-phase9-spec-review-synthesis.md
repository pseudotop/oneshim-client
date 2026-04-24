# Phase 9 Spec — Review Synthesis (Loop 1c → 1d)

**Date**: 2026-04-24
**Spec under review**: `docs/reviews/2026-04-23-phase9-quick-wins-spec.md` @ 1247 lines / branch `feature/phase9-quick-wins` tip `5618558c`
**Inputs**: R1 (445L, 7C/9I/11M) + R2 (281L, 3C/6I/7M) + R3 (274L, 2C/7I/9M) = **61 findings**
**Consolidated**: **46 findings** after dedup (12 Critical, 16 Important, 18 Minor)
**Disagreements**: **2 items**
**User-decision blockers**: **9 items**
**Fix-plan length**: 28 ordered steps
**Gate**: **NOT READY** for Loop 2 plan — zero-Critical gate fails on 12 items.

---

## 0. Reading key

| Marker | Meaning |
|--------|---------|
| `CONS-Cxx` | Consolidated Critical finding (must fix) |
| `CONS-Ixx` | Consolidated Important finding (must fix) |
| `CONS-Mxx` | Consolidated Minor finding (can defer) |
| `🚨 DISAGREEMENT` | Reviewers contradict; user decision needed |
| `⚠ USER-INPUT` | Can't mechanically fix; awaits decision |
| `✅ Verified` | Evidence check by this synthesis confirmed |
| `❌ Rebutted` | Evidence check failed (none this round) |

All file:line references assume worktree tip `5618558c`.

---

## 1. Consolidated Critical (12 — zero-gate blockers)

### CONS-C01. Analysis loop has no current schedule gate (fabricated in spec §3.8 row 4)

- **Source**: R1.C1 (unique)
- **Severity**: Critical
- **Spec section**: §3.8 row 4
- **Evidence** (✅ Verified): `rg -n should_run_now src-tauri/src/` → only 3 hits: `mod.rs:548` (def), `mod.rs:584` (test), `monitor.rs:203` (only consumer). `rg active_hours|schedule src-tauri/src/scheduler/loops/intelligence.rs` → **zero matches**. The analysis loop currently runs unconditionally.
- **Impact**: implementer would hunt for a nonexistent gate. Implicitly expands scope (is a schedule gate now being *added* to analysis where none existed?).
- **Fix**: rewrite §3.8 row 4 to reflect reality (no current gate) and explicitly state Phase 9 *adds* a tracking-schedule gate. Promote to Decision D13 ("does Phase 9 add a schedule gate to the Analysis loop?").
- **Dependency**: intersects CONS-C02 (scope enumeration), must be unified.

### CONS-C02. Scope suppression is incomplete — 9+ event pipelines leak PII during tracking-schedule windows

- **Source**: R2.C1 (primary) + R3.I4 (complementary enumeration)
- **Severity**: Critical (R2 call, stronger than R3.I4)
- **Spec section**: §3.1 + §3.8 table
- **Evidence** (✅ Verified):
  - `rg should_run_now|active_hours|capture_paused src-tauri/src/scheduler/loops/` → only `monitor.rs` + `health.rs` use any gate.
  - `sed -n '60,135p' src-tauri/src/scheduler/loops/events.rs` → Process/Input/Clipboard/File events have NO schedule gate.
  - `intelligence.rs:14,124,160` — 3 spawn_* loops (analysis/focus/coaching), none gated.
  - `sync.rs` — oauth_refresh + cross_device_sync, neither gated.
- **Impact**: GDPR Art. 5 purpose-limitation breach. User sees red-border off, but window-title + keystroke + clipboard + file-access events continue to hit the server. Active GDPR misrepresentation.
- **Fix**: §3.8 table must enumerate EVERY data-producing loop with a per-loop disposition (gated / ungated / domain-decision). Minimum 9 rows:
  1. Capture decision (`trigger.rs:138-148`) — already in spec
  2. Monitor-loop capture guard (`scheduler/loops/monitor.rs:200-207,292`) — already in spec
  3. Upload flush (`batch_uploader.rs:199`) — already in spec
  4. **Analysis loop** (`intelligence.rs:14`) — CONS-C01
  5. **Window-switch events** (`monitor.rs:181-189`, save before line 207 gate)
  6. **Input activity events** (`events.rs:94-110`)
  7. **Process snapshot events** (`events.rs:63-92`)
  8. **Clipboard events** (`events.rs:117-124`)
  9. **File-access events** (`events.rs:128-…`)
  10. **Focus analyzer loop** (`intelligence.rs:124`)
  11. **Coaching loop** (`intelligence.rs:160`)
  12. **Cross-device sync** (`sync.rs:cross_device_sync_loop`)
  13. `commands::audio::start_audio_capture` (not a loop — imperative command; must refuse during window)
- **Dependency**: must precede CONS-C03 (upload-defer correctness depends on upstream gating being complete).
- **⚠ USER-INPUT**: (a) enumerate-all and gate all, OR (b) scope-reduce Phase 9 to capture+upload only and defer event-loop gating to Phase 10.

### CONS-C03. Upload-defer semantics self-contradict when combined with CONS-C02

- **Source**: R2.C2 (unique angle; R1.C2 is about the misnamed precedent, not semantic)
- **Severity**: Critical
- **Spec section**: §3.9
- **Evidence** (✅ Verified via CONS-C02 chain): `events.rs:94-110` enqueues to uploader unconditionally. Even if capture-gate short-circuits in `monitor.rs:207`, the events loops still pump in-window-timestamped rows into `uploader.enqueue`.
- **Impact**: exit-flush ships in-window PII. Silent GDPR violation.
- **Fix**: §3.9 must state:
  1. **Intent**: drain pre-window events on window exit in FIFO order.
  2. **Prove non-interference**: no in-window-timestamped event can enter the queue (depends on CONS-C02 being resolved upstream).
  3. **Long-window overflow**: if queue approaches `max_queue_size` during suppression, flush pre-window events JUST before the window starts, not after (pre-flush drain).
  4. **Integration test**: window `[12:00, 13:00]`; event at T=11:30 queued → flush at T=13:01 ships only 11:30 row; no capture at T=12:30 attempted (CONS-C02 guarantee).

### CONS-C04. DST fall-back fires TWICE, not once — spec §3.7 semantics are wrong

- **Source**: R2.C3 (unique)
- **Severity**: Critical (correctness + test-plan + GDPR DPIA docs)
- **Spec section**: §3.7 DST subsection
- **Evidence** (analysis, not grep): the evaluator at §3.7 uses wall-clock `"HH:MM"` string comparison. During DST fall-back (US/Europe), wall-clock 02:30 occurs twice; predicate fires both times. Spec claims "exactly once" — wrong. Spring-forward "lost hour" means a window entirely in 02:00–03:00 fires ZERO times on DST Sunday — user-visible anomaly (claim "no anomaly" is wrong).
- **Impact**: DPIA docs will be incorrect; integration tests asserting "fires once" will be broken or falsely green.
- **Fix**: §3.7 rewrite:
  - Fall-back: "fires on both occurrences of the ambiguous hour. Because this is a suppression primitive (over-suppress-safe), this is acceptable."
  - Spring-forward: "window entirely within skipped hour → does not fire on that day. UI should warn when configured range overlaps skipped hour in configured TZ for current year."
  - Integration test: US/Eastern DST transition fixtures.

### CONS-C05. `should_run_now` does not handle overnight windows — composition with `active_hours` is broken

- **Source**: R1.C5 (unique)
- **Severity**: Critical
- **Spec section**: §3.4 + §3.8
- **Evidence** (✅ Verified): `scheduler/mod.rs:548-571`: `hour >= schedule.active_start_hour && hour < schedule.active_end_hour` — no wrap branch. `trigger.rs:69-77`: has wrap branch. User with `active_hours_enabled=true, start=22, end=6` at 23:00 Wed: `should_capture()` permits, `should_run_now()` blocks. Monitor loop uses `should_run_now` → capture fails despite trigger saying it should succeed.
- **Impact**: pre-existing latent bug; Phase 9 "hoist to scheduler" recommendation amplifies it by deleting the one correct impl.
- **Fix**: §3.4a new subsection + Decision D-new. Three options for user to pick:
  - **Option A**: Phase 9 fixes `should_run_now` to match `is_within_active_hours` wrap logic (latent-bug fix riding along).
  - **Option B**: Document the limitation, defer to separate PR, accept that overnight `active_hours` users get inconsistent behavior for the duration.
  - **Option C**: Fix AND hoist both checks out (delete duplication).
- **⚠ USER-INPUT**: pick A/B/C.

### CONS-C06. Test infrastructure: no mock-clock exists in workspace

- **Source**: R3.C1 (unique)
- **Severity**: Critical
- **Spec section**: §6.1 (integration test strategy)
- **Evidence** (✅ Verified): `rg mock_clock|FakeClock|MockClock|TestClock|fake_clock` across workspace → only hits are in review file itself. Zero symbols in actual code.
- **Impact**: spec's §6.1 "use a mock clock" is unimplementable as written.
- **Fix**: pick one of three shapes:
  - **(a)** `resolve_now()` takes `Fn() -> DateTime<Local>` injection, defaults to `chrono::Local::now`.
  - **(b)** `tracking_schedule_active(config, now: DateTime<Local>)` pure 2-arg function. Recommended; aligns with `should_run_now` shape. Cheapest.
  - **(c)** Accept real-clock tests with narrow windows + flakiness-retry, document flakiness risk.
- Update §6.1 to reflect chosen shape and the testability it enables.
- **⚠ USER-INPUT**: decide a/b/c (R3 recommends b).

### CONS-C07. Linux autostart REST roundtrip CI tests will silently pass with broken systemd

- **Source**: R3.C2 (unique) + reinforced by R3.I6 + CONS-C08
- **Severity**: Critical
- **Spec section**: §6.1 (Feature 2 test strategy)
- **Evidence** (✅ Verified): `.github/workflows/ci.yml` test job runs on `ubuntu-latest`, which has `systemctl` installed but no user D-Bus session. Current `autostart.rs:389-401` swallows non-zero `systemctl --user enable` exit with `warn!` and returns `Ok(())`. A roundtrip test `GET→PUT(true)→GET` returns `{enabled: true, mechanism: "systemd"}` based on file presence, not actual systemd registration.
- **Impact**: green CI with broken Linux autostart. Worst-case user-trust outcome.
- **Fix** (multiple parts):
  1. `autostart.rs:389-401` must return `Err` on non-zero exit, not `warn! + Ok`.
  2. Spec §6.1 must document CI strategy for Linux:
     - **(a)** `#[ignore]`-gate the live-systemd test; add a separate Linux matrix job with `systemd-run --user` session bootstrap.
     - **(b)** Env-var escape hatch in tests (e.g. `ONESHIM_AUTOSTART_STUB=1`) to avoid actual systemctl invocation; assert command-shape-would-be-correct.
  3. Add explicit test: "when systemctl enable returns non-zero, handler returns 5xx and `is_enabled()` returns false" — verifies UI doesn't lie.
- **⚠ USER-INPUT**: pick (a) add Linux CI matrix vs (b) env-stub (R3 prefers a, but b is cheaper).

### CONS-C08. `with_capture_paused` is on `Scheduler`, not `BatchUploader` — spec §3.9 cites false precedent

- **Source**: R1.C2 + R3.I5 (duplicate — R3 angle is weaker but same root)
- **Severity**: Critical (R1 call, sourced on bad precedent leading implementer astray)
- **Spec section**: §3.9 last paragraph
- **Evidence** (✅ Verified): `rg with_capture_paused|capture_paused crates/oneshim-network/src/batch_uploader.rs` → 0 hits. `crates/oneshim-network/src/batch_uploader.rs:74` is the real precedent: `pub fn with_health_flag(mut self, flag: Arc<AtomicBool>) -> Self`.
- **Impact**: implementer looks for nonexistent builder pattern; also misses that the proposed closure shape differs from the real `Arc<AtomicBool>` pattern.
- **Fix**: §3.9 final paragraph rewrite:
  > "This introduces a **new** injection shape in the crate. The closest existing pattern is `BatchUploader::with_health_flag(Arc<AtomicBool>) -> Self` at `crates/oneshim-network/src/batch_uploader.rs:74` (circuit-breaker gating flag), but our tracking-schedule predicate requires a closure `Arc<dyn Fn() -> bool + Send + Sync>` to read through `AppConfig` state rather than a single atomic. The crate stays free of `AppConfig` dependency because the closure is constructed by the composition root."

### CONS-C09. Existing `batch_add_tag` handler silently swallows per-row errors — transactional refactor is a 200→500 behavior change

- **Source**: R1.C7 + R3 Note #4 (same observation, R1 classified Critical)
- **Severity**: Critical
- **Spec section**: §2.3 + §5.5 + §7 decisions log + §9 open questions
- **Evidence** (✅ Verified): `handlers/tags.rs:83-98`:
  ```rust
  for frame_id in &req.frame_ids {
      match ... .add_tag_to_frame(*frame_id, req.tag_id) {
          Ok(_) => tagged_count += 1,
          Err(e) => { tracing::warn!("..."); }  // silently continues
      }
  }
  Ok(Json(BatchTagResponse { tagged_count }))  // ALWAYS 200
  ```
  Response shape gives no signal to frontend which rows failed.
- **Impact**: §5.5 refactor flips "silent partial 200" → "explicit 500 all-or-nothing". Frontend `onSuccess` → `onError` paths change. Must be documented.
- **Fix**:
  1. §2.3 last paragraph add: "Current handler returns 200 even when per-row ops fail — caller has no signal."
  2. New Decision **D15** in §7: "Transactional refactor is intentionally a 200→500 behavior change on mixed-partial-failure inputs. Frontend updated to handle 500."
  3. New Q in §9: "Are any known callers relying on batch_add returning 200 on partial success?" (Known answer: only `TimelineLayout.tsx:131-140`; frontend updated in same PR.)

### CONS-C10. Autostart `enable()` error paths swallow non-zero exits on macOS, Linux, and Windows (deepens C07)

- **Source**: R3.I6 (primary) — cross-referenced with R3.C2 / CONS-C07
- **Severity**: Critical (R3.I6 is "Important" but combined with CONS-C07 the aggregate is Critical — "trust-eroding silent success" is the same root bug)
- **Spec section**: §4.10 + §4.7
- **Evidence** (✅ Verified): `autostart.rs:398-401` Linux swallows non-zero exit. `autostart.rs:137-141` (macOS) returns `Err` only on spawn error, not on `launchctl load` non-zero exit. Windows path not re-checked here but R3 asserts same pattern. No command timeout on any path.
- **Impact**: `is_enabled()` returns true (file exists) while actual boot behavior is broken. Same class of bug as CONS-C07.
- **Fix**:
  1. Upgrade §4.10 mapping table with a "command non-zero exit" row → `internal.io` (500).
  2. Add `Command::output` wrapped in `tokio::time::timeout(Duration::from_secs(5), …)` on all 3 platforms.
  3. Return `Err` on non-zero exit for all three platforms.
  4. Update §4.10 failure table to list: `enable()/disable()` non-zero exit, timeout, spawn-error (3 separate rows).

### CONS-C11. Test count drift — autostart has 9 tests, not 14; trigger has 13, not 8

- **Source**: R1.C3 + R1.C4 (two findings, one root cause — test count drift)
- **Severity**: Critical (R1 classification; baseline-inaccuracy breaks health gates)
- **Spec section**: §4.1, §6.1 (autostart); §3.8 last paragraph (trigger)
- **Evidence** (✅ Verified): `grep -c '#\[test\]' src-tauri/src/autostart.rs` → **9**. `grep -c '#\[test\]' crates/oneshim-vision/src/trigger.rs` → **13**. Schedule-specific tests in trigger: `blocks_capture_outside_active_hours` (373), `allows_capture_when_schedule_disabled` (398), `handles_overnight_active_hours` (409) — **3, not 2**.
- **Impact**: impl plan's test-migration count is wrong; CI-gate baseline is wrong.
- **Fix**:
  - §4.1 → "**9 unit tests** (lines 460-549)"
  - §6.1 Feature 2 → "existing 9 tests at `autostart.rs:460-548` stay passing"
  - §3.8 last paragraph → "verify this refactor does not break the **13 existing trigger unit tests at `trigger.rs:193-435`**; three schedule tests (`blocks_capture_outside_active_hours`, `allows_capture_when_schedule_disabled`, `handles_overnight_active_hours`) would migrate to scheduler-side"

### CONS-C12. Wire-contract snapshot count is 42, not 41 (CLAUDE.md and spec both stale)

- **Source**: R1.C6 (unique)
- **Severity**: Critical (R1 classification; doc drift rot)
- **Spec section**: §6.3
- **Evidence** (✅ Verified): `grep -c "^[a-z]" crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` → **42**. Extra codes beyond the set of 41 the spec implies: `not_found.resource_missing`, `consent.expired`, `consent.required`, `provider.bedrock.unsupported`.
- **Impact**: guidance ("Wire-format contract locked at 41 codes" in workspace CLAUDE.md) drifted from reality; spec carries the same error. Spec's conclusion ("no new wire codes needed") unchanged, but baseline is incorrect.
- **Fix**: §6.3 → "**42 locked codes** (CLAUDE.md says 41 — file as `reference_doc_drift` follow-up). No new wire codes required for Phase 9." Verify spec's 6 listed mappings against the real 42-row catalog (already verified in synthesis — all 6 present).

---

## 2. Consolidated Important (16 — must address before Loop 2 plan)

### CONS-I01. Line-range + anchor drift (aggregated)

- **Source**: R1.M1–M10 (minor individually; aggregated as Important because they collectively break "anchors verified" claim)
- **Severity**: Important (ensemble)
- **Spec sections**: §3.6, §3.7, §5.3, §5.4, etc.
- **Drift list** (✅ all verified):

  | Spec claim | Actual |
  |---|---|
  | `monitoring.rs:57-86` ScheduleConfig | `58-73` + `75-85` |
  | `enums.rs:12-36` Weekday | `11-35` |
  | `coaching.rs:119-125` TimeRange | `118-124` |
  | `batch_uploader.rs:135-156` drop_oldest | `136-156` |
  | `events.rs:127` transaction | line `126` |
  | `maintenance.rs:420` transaction | line `418` |
  | `runtime_state.rs:371` indicator_visible | line `366` |
  | `api/client.ts:183-201` fetchFrames | `183-198` |
  | `api/client.ts:579-588` batchAddTag | `579-587` |
  | `capture_status.rs:48-153` | `62-153` |
  | `autostart.rs:92-95,325-329,188-190` | single-line calls at `93`, `189`, `326` |
  | `i18n/locales/en.json:1336` general | line `1343` |

- **Fix**: mechanical rewrite. Each citation updated per table above.

### CONS-I02. `chrono-tz` is an `oneshim-core` hexagonal concern, not just a binary-size Q

- **Source**: R1.I1 + R3.M1 (complementary — R1 architectural, R3 size)
- **Severity**: Important
- **Spec section**: §3.7 + §9 Q1 → promote to §7 Decisions
- **Evidence** (✅ Verified): `chrono-tz` not in workspace (`grep -c chrono-tz Cargo.lock Cargo.toml` → 0).
- **Impact**: adding to `oneshim-core` (leaf crate) pulls +2.1MB tzdata into every consumer. Architectural alternative: port-in-adapter (`TimezoneResolver` port in `oneshim-core`, impl in a new or existing adapter).
- **Fix**: promote Q1 to Decision **D-new (D16)** with two options:
  - **(a)** Accept `chrono-tz` in `oneshim-core` (simplest; spec's implicit choice).
  - **(b)** Define `TrackingScheduleConfig::timezone: String` in core; `TimezoneResolver` port in core; `chrono-tz` impl in an adapter crate.
- **⚠ USER-INPUT**: pick (a) or (b).

### CONS-I03. Tray indicator propagation mechanism unspecified (§3.11)

- **Source**: R1.I2 (unique)
- **Severity**: Important
- **Spec section**: §3.11
- **Evidence**: spec says "tray tooltip updates" but never says HOW. ADR-016 config-change-bus exists as a potential mechanism but is not chosen. Tray loop has no re-eval tick today.
- **Fix**: add §3.11a "Tray indicator propagation mechanism" with 3 options and a choice:
  - **(a)** ADR-016 `subscribe()` — tray task subscribes to config changes and re-renders.
  - **(b)** Tauri event emit on `set_tracking_schedule` IPC.
  - **(c)** Tray re-eval tick (1-5s).
- **⚠ USER-INPUT**: pick (a/b/c). R1 recommends (a).

### CONS-I04. `SmartCaptureTrigger::with_schedule` refactor is cross-crate breaking — Q4 must become Decision

- **Source**: R1.I3 (unique; R1.I3 + §9 Q4)
- **Severity**: Important
- **Spec section**: §3.8 + §9 Q4 → §7 Decisions
- **Evidence**: refactor removes `schedule: ScheduleConfig` param from trigger constructor, touching DI wiring at `app_runtime_launch.rs` + 3 schedule tests (`trigger.rs:373,398,409`) + any other trigger ctor callsite.
- **Fix**: promote Q4 to Decision **D-new (D17)**:
  - **(a)** In-scope for same PR — spec §3.8a enumerates all DI callsites and migration tests.
  - **(b)** Defer to follow-up — spec explicitly accepts "half-migrated trigger" state during Phase 9.
- **⚠ USER-INPUT**: pick (a/b).

### CONS-I05. Autostart returns `Result<_, String>` — ADR-019 typed-code mapping is lossy

- **Source**: R1.I4 (unique)
- **Severity**: Important
- **Spec section**: §4.10
- **Evidence**: autostart module returns `String` error; spec §4.10 maps via substring at handler boundary (substring-matching is exactly what ADR-019 was created to eliminate).
- **Fix**: §4.10 add a paragraph + Decision **D-new (D18)**:
  - **(a)** Accept substring-mapping at boundary; Phase 9 is quick-win scope (defer proper typed error to follow-up).
  - **(b)** Upgrade autostart to typed error `AutostartError` enum per ADR-019 §7 (adds scope to PR).
- **⚠ USER-INPUT**: pick (a/b).

### CONS-I06. `scheduler/loops/monitor.rs` is at 498 lines — guardrail violation imminent

- **Source**: R1.I5 (unique)
- **Severity**: Important
- **Spec section**: §3.8
- **Evidence** (✅ Verified): `wc -l src-tauri/src/scheduler/loops/monitor.rs` → **498 lines**. CLAUDE.md guardrail: "must stay under 500 lines".
- **Fix**: §3.8 add sentence naming helper extraction: "Per CLAUDE.md monitor-loop complexity guardrail, the tracking-schedule check body lives in new `scheduler/loops/tracking_schedule_helper.rs` (free fn `evaluate_tracking_schedule(&cfg) -> bool`), mirroring `coaching_helper.rs`, `focus_auto_helper.rs`, `vision_helper.rs` precedent."

### CONS-I07. Overnight-active-hours rows missing from §3.4 truth table (downstream of CONS-C05)

- **Source**: R1.I6 (unique; depends on CONS-C05)
- **Severity**: Important
- **Spec section**: §3.4
- **Fix**: once CONS-C05 is decided (A/B/C), add §3.4 truth-table row covering overnight active_hours × overnight tracking_schedule × both-shapes-of-overnight. Minimum one overnight × overnight row and one overnight × normal row.

### CONS-I08. Backend page-size cap (Q3) needs to be a Decision — unbounded batch is a real footgun

- **Source**: R1.I7 + R2.I4 (complementary — R1 about caller-side third parties, R2 about "Select all" scope)
- **Severity**: Important
- **Spec section**: §9 Q3 → §7 Decisions
- **Evidence**: `crates/oneshim-web/src/handlers/frames.rs:12-18` does not cap `limit`. A third-party script can send `frame_ids = [1..100000]`.
- **Fix**: promote Q3 to Decision **D-new (D19)**:
  - Hard cap `MAX_BATCH_SIZE = 1000` in `handlers/tags.rs`.
  - `> 1000` → reject `400 validation.invalid_arguments`.
  - Frontend "Select all" scope clarified in §5.9: "Select all selects all frames currently loaded in the active page viewport (≤ pageSize = 50)."
  - Test added: `batch_remove_tag` with 1001 ids → 400; 1000 ids → 200 < 50ms.

### CONS-I09. `BatchTagResponse.tagged_count` → `affected_count` rename: frontend consumer verified

- **Source**: R1.I8 + R2 Note 2 (both agree the rename is safe)
- **Severity**: Important (action needed, but consensus is "safe")
- **Spec section**: §5.6 D8-alt
- **Evidence** (✅ Verified): only consumer is `TimelineLayout.tsx:131-140` `data.tagged_count`. OpenAPI at `docs/contracts/oneshim-web.v1.openapi.yaml:1375` uses `GenericObject` (untyped), so no OpenAPI contract break.
- **Fix**: §5.6 lists exact 3 lines to edit in-PR: `api-contracts/src/tags.rs` field rename, `client.ts:579-587` return type, `TimelineLayout.tsx:131-140` `onSuccess` handler. Add changelog note.

### CONS-I10. Scope-enumeration weakness also applies to regulatory framing (CCPA/state acts)

- **Source**: R2.I2 (unique)
- **Severity**: Important
- **Spec section**: §2.1 "Regulatory grounding"
- **Fix**: expand §2.1 to enumerate:
  - GDPR Art. 5, 13/14, 25, 35 — supported.
  - CCPA/CPRA — not addressed; separate privacy-notice UI needed.
  - US state monitoring acts (NY §52-c, DE §19, CT §31-48d) — written notice obligation orthogonal.
  - GDPR Art. 17 — existing `DELETE /data`.

### CONS-I11. Consent × tracking-schedule composition unspecified

- **Source**: R2.I3 (unique)
- **Severity**: Important
- **Spec section**: §3.4
- **Evidence** (✅ Verified): `ConsentManager` exists in `oneshim-core/src/consent.rs:102`. Spec's composition rule in §3.4 omits consent.
- **Fix**: §3.4 composition rule:
  ```
  capture_allowed(now, tier) = consent_granted(tier)
                            AND active_hours_gate(now)
                            AND NOT tracking_schedule_active(now)
                            AND NOT capture_paused
  ```
  Add §3.4.a conflict-resolution table: consent revocation is always top-authority.

### CONS-I12. "Remove tag" popover UX (Q8) deferred — must be Decision D13

- **Source**: R2.I5 (unique; overlaps with D13 id but numbering updated below)
- **Severity**: Important
- **Spec section**: §5.8 + §9 Q8
- **Fix**: promote Q8 to Decision **D-new (D20)**:
  - **(a)** Show all tags; silent no-op for non-attached frames; toast `"{affected_count} of {selected_count}"`.
  - **(b)** Show intersection; requires pre-fetch.
- **⚠ USER-INPUT**: R2 recommends (a).

### CONS-I13. Autostart UX has no first-run prompt, no repair UI, no error copy

- **Source**: R2.I6 (unique; overlaps with R3.M3)
- **Severity**: Important
- **Spec section**: §4.7 + §4.9 + §4.10
- **Fix**: add:
  - Decision **D-new (D21)**: first-run prompt — (a) minimal prompt in welcome dialog, (b) defer to separate onboarding PR.
  - §4.7 Repair-Autostart button: spec current/future state in-scope; write one-sentence behavior spec or defer explicitly.
  - §4.10 user-facing error copy: map `internal.io`, `storage.failed` wire codes → translation keys in §6.4.
- **⚠ USER-INPUT**: pick D21 option.

### CONS-I14. Suspend/resume + clock-skew behavior undocumented

- **Source**: R3.I3 (unique)
- **Severity**: Important
- **Spec section**: §3.7 + §3.11
- **Fix**: §3.7a "Clock irregularities" subsection:
  - Suspend across window boundary → missed notifications; gate was correct throughout.
  - Backward clock-jump re-entering window → duplicate notifications; debounce via `last_notification_at: Instant` cooldown (e.g. 60s).
  - Forward clock-jump skipping window end → "stuck in suppression" until clock is sane; user-self-inflicted.

### CONS-I15. `has_systemctl()` caching (Q5) should be decided — `OnceLock`-per-process

- **Source**: R3.I2 (unique)
- **Severity**: Important
- **Spec section**: §9 Q5 → §4.3
- **Fix**: promote Q5 to design note in §4.3: "`has_systemctl()` memoized via `static HAS_SYSTEMCTL: OnceLock<bool>` — one-time init per process. Eliminates Q5."

### CONS-I16. Batch-tag test coverage gaps (FK violation, concurrent writer, empty-input, cache reuse)

- **Source**: R3.I7 (unique)
- **Severity**: Important
- **Spec section**: §6.1 Feature 3
- **Fix**: §6.1 Feature 3 test list adds:
  - `add_tag_to_frames_rolls_back_on_fk_violation` — nonexistent `frame_id` → entire batch rolls back.
  - `remove_tag_from_frames_handles_missing_pairs_transactionally` — (frame_id, tag_id) pair doesn't exist in `frame_tags` → `n` counts only actually-deleted rows.
  - `batch_ops_compete_with_concurrent_writer` — second thread (e.g. event-writer) holds lock; test verifies batch call blocks then succeeds.
  - `empty_input_is_lock_free` — `frame_ids = []` returns `Ok(0)` without acquiring the connection lock.
  - Statement-cache reuse across rolled-back transactions.

---

## 3. Consolidated Minor (18 — can defer to Loop 2/3)

### CONS-M01. Korean i18n inconsistency ("스케줄" vs "일정")

- **Source**: R2.I1 (R2 called "Important" — downgraded because i18n copy is normally a Minor)
- **Fix**: §6.4 Korean — pick one term; apply to all surfaces. Industry enterprise-Korean convention favors "일정" over loanword "스케줄"; user may choose.

### CONS-M02. Leftover "Blackout" comment string in `trigger.rs:370`

- **Source**: R2.M1 (✅ Verified: `grep -n "Blackout\|blackout" crates/oneshim-vision/src/trigger.rs` → line 370)
- **Fix**: §6.5 CI implications — add sweep for `blackout` identifiers; rename or remove during trigger test migration.

### CONS-M03. "Start minimized" toggle doesn't exist — spec §4.9 fabrication

- **Source**: R1.M10 + R2.M2 (duplicate; ✅ Verified: `grep -rn "startMinimized\|minimized" frontend/src` → 0 hits)
- **Fix**: §4.9 rewrite: "`GeneralTab` already hosts update-lifecycle toggles ('Check for updates', 'Auto-install updates') + the ScheduleSettings section. Autostart belongs to the same app-level lifecycle category."

### CONS-M04. ARIA multi-select a11y gap — pre-existing; acknowledge

- **Source**: R2.M3 (unique)
- **Fix**: §5.8 add sentence: "A11y gap is pre-existing; adding ARIA is out-of-scope for Phase 9; file as follow-up sprint item."

### CONS-M05. `notification.tracking_schedule_notifications` field name tautological

- **Source**: R2.M4 (unique)
- **Fix**: §3.11 rename field to `tracking_schedule_enabled` (section name provides context).

### CONS-M06. Single-schedule-vs-multiple-schedule design choice implicit

- **Source**: R2.M5 (unique)
- **Fix**: add §3.14 "Single schedule, multiple windows" — Decision **D-new (D22)**: chosen single-`TrackingScheduleConfig` over peer-product named-multiple pattern. Rationale: simpler MVP; add multiple named in follow-up.

### CONS-M07. `*` (any day) notation in §3.4 worked example

- **Source**: R2.M6 (unique)
- **Fix**: §3.4 row 7 — either add `*` as shorthand with explicit definition, or spell out `[Mon..Sun]`.

### CONS-M08. No "pre-configured presets" rejected option

- **Source**: R2.M7 (unique)
- **Fix**: §8.1 add: "Pre-configured presets (9-to-5, lunch, after-hours) — rejected; user-crafted schedules only at launch."

### CONS-M09. `is_enabled()` cross-platform file-existence lie

- **Source**: R3.I1 (R3 called "Important"; downgraded because it's pre-existing and Phase 9 only surfaces it)
- **Fix**: §4.3 + §4.7 note that `is_enabled() == true` ≠ "will run at boot"; applies to all 3 platforms. Repair-Autostart (CONS-I13) applies to all 3.

### CONS-M10. Observability: `err.code` convention not propagated to all autostart sites

- **Source**: R3.M7 (unique)
- **Fix**: §6.2 enumerate all structured-log sites in autostart (macOS `launchctl load` failure, Linux `systemctl enable` failure, Windows `RegSetValueExW` failure). Change `?mech` → `%mech` for `Display` stability.

### CONS-M11. Playwright E2E dir is `e2e/`, not `tests/` (✅ Verified)

- **Source**: R3.M2 (unique; ✅ Verified `ls frontend/e2e/` shows 32 spec files; `frontend/tests/` doesn't exist)
- **Fix**: §6.1 Feature 3 → "add test cases to `frontend/e2e/timeline-actions.spec.ts`".

### CONS-M12. Tray icon asset (Q7) — decide or defer explicitly

- **Source**: R3.M4 (unique)
- **Fix**: §3.11 state "reuse Paused icon, change tooltip" (recommendation already in spec). Q7 → resolved in-place.

### CONS-M13. Alpine / musl / OpenRC + Snap coverage

- **Source**: R3.M5 + R3 Note 10 (unique; Snap is an additional gap)
- **Fix**: §4.8 add sentence: "XDG fallback is best-effort; DE-less non-systemd distros may not honor it. Snap packages relocate binaries on refresh — Snap users must re-enable after each snap refresh."
- §4.7 binary-path table: add row `Snap — Changed each refresh — YES — broken`.

### CONS-M14. `serial_test` needed for live-FS autostart integration tests

- **Source**: R3.M6 (unique)
- **Fix**: §6.5 update "`serial_test` not needed" sentence to "`serial_test` required for new autostart integration tests that touch real FS state (`~/.config/systemd/user/*.service`, `~/Library/LaunchAgents/*.plist`, HKCU registry); existing unit tests (file-content generation) do not need it."

### CONS-M15. Contract files (OpenAPI + manifest) are integrity-gate, hand-maintained

- **Source**: R3.M8 (unique)
- **Fix**: §6.5 Lefthook/CI bullet → "OpenAPI + `http-interface-manifest.v1.json` deltas are an **integrity gate** (`.github/workflows/integrity-gates.yml`). Plan must name the responsible step for hand-patching both files and add regression tests."

### CONS-M16. Docs updates (STATUS/PHASE-HISTORY/companion) not scoped

- **Source**: R3.M9 (unique)
- **Fix**: §6.5 add bullet: "`docs/STATUS.md` test-count bump; `docs/PHASE-HISTORY.md` new Phase 9 entry; explicit 'no user-facing guide needed' OR 'guide + .ko.md companion in this PR'."

### CONS-M17. i18n keys added only to en/ko — es/ja/zh-CN gap

- **Source**: R1.M11 (unique; ✅ Verified 5 locales)
- **Fix**: §6.4 note: "Adding keys only to en/ko triggers fallback to English in es/ja/zh-CN." Decide: translate now vs defer to i18n PR. Recommend defer with tracking TODO.

### CONS-M18. `ALL_TABLES` transaction precedent at `maintenance.rs:420` is different in character

- **Source**: R1.I9 (R1 called "Important"; downgraded because it's a minor citation-clarity issue)
- **Fix**: §5.4 "Transaction precedent" cell → cite only `events.rs:126` (line number also fixed per CONS-I01); annotate `maintenance.rs:418` as "separate but demonstrates multi-table transactional DELETE".

---

## 4. Disagreements requiring user input (🚨)

### 🚨 DISAGREEMENT 1: R1 vs R3 on "the precedent claim in §3.9"

- **R1.C2** (Critical): claims the entire "mirrors `with_capture_paused`" sentence is false and must be rewritten to reference `with_health_flag`.
- **R3.I5** (Important): same observation but classifies as Important, and frames it as "introducing a new pattern is fine; reword".
- **Resolution**: no real disagreement on facts — both agree the precedent claim is wrong. Disagreement is only on severity. **Taking R1 call (Critical)** because an implementer looking for a nonexistent `with_capture_paused` on `BatchUploader` is a concrete blocker, not a stylistic issue.
- **Action**: no user input needed; consolidated as CONS-C08.

### 🚨 DISAGREEMENT 2: R1.I4 vs R3.I6 on autostart error-type upgrade scope

- **R1.I4** (Important): autostart `Result<_, String>` violates ADR-019 spirit; offers (a) accept at boundary, (b) upgrade to typed. Wants decision.
- **R3.I6** (Important, related to C2): wants functional behavior fix (return `Err` on non-zero exit, add timeout) but does NOT mandate typed-error upgrade.
- **Resolution**: these are **different** fixes at different layers:
  - CONS-C10 (R3.I6 upgraded): functional — return Err on non-zero, add timeout. REQUIRED.
  - CONS-I05 (R1.I4): type shape — `Result<_, String>` vs typed `AutostartError`. OPTIONAL.
- **Action**: user must decide CONS-I05 (a/b) independently of CONS-C10 (which is non-negotiable).

---

## 5. User-decision blockers (9 items)

These cannot be mechanically rewritten; they await user direction before Loop 1d can close.

| ID | Question | Options | Recommended | Reviewer |
|----|----------|---------|-------------|----------|
| **U1** | Does Phase 9 add a schedule gate to the Analysis loop + 8 other currently-ungated pipelines, OR scope-reduce Phase 9? | (a) enumerate all; gate all (CONS-C02); (b) scope-reduce to capture+upload only | (a) — the GDPR framing requires completeness | R1+R2 |
| **U2** | `should_run_now` overnight bug — fix, defer, or fix+hoist? | A/B/C per CONS-C05 | C (fix+hoist together) — cleanest | R1 |
| **U3** | Test harness shape for tracking-schedule tests | (a) injected closure; (b) 2-arg pure fn `tracking_schedule_active(cfg, now)`; (c) real-clock + retry | (b) — cheapest, aligns with `should_run_now` | R3 |
| **U4** | Linux autostart CI strategy | (a) new Linux matrix with `systemd-run --user`; (b) env-var stub escape hatch | (b) cheaper; (a) more correct | R3 |
| **U5** | `chrono-tz` in `oneshim-core` vs adapter port | (a) accept `chrono-tz` in core; (b) `TimezoneResolver` port in core + impl in adapter | (a) — simpler; spec's implicit choice | R1 |
| **U6** | Tray indicator propagation mechanism | (a) ADR-016 subscribe; (b) Tauri event emit; (c) tray re-eval tick | (a) — cleanest per R1 | R1 |
| **U7** | `SmartCaptureTrigger::with_schedule` refactor in-scope? | (a) in-PR; (b) follow-up | (a) per R1 ("half-migrated leaves bad state") | R1 |
| **U8** | Autostart typed-error upgrade? | (a) accept `Result<_, String>` at boundary; (b) upgrade to typed `AutostartError` | (a) — quick-wins scope; upgrade is separate PR | R1 |
| **U9** | First-run autostart prompt? | (a) minimal prompt in welcome dialog; (b) defer to onboarding PR | (b) — scope discipline | R2 |

Secondary decisions (lower-impact but still need resolution before Loop 2 plan):

| ID | Question | Default |
|----|----------|---------|
| U10 | "Remove tag" popover content (show all vs intersection) | show all + toast `"{affected_count} of {selected_count}"` |
| U11 | Korean i18n term for "Tracking Schedule" | "추적 일정" (enterprise-Korean) |
| U12 | i18n translations for es/ja/zh-CN — in-PR or follow-up? | follow-up with tracking TODO |
| U13 | User-facing tracking-schedule guide — in-PR or skip? | skip for Phase 9; TODO for docs sprint |

---

## 6. Fix plan (ordered, 28 steps)

Ordering: (1) baseline anchors & counts first; (2) user-decision blockers flagged; (3) scope expansions before dependent specs; (4) minor polish last.

### Phase A — Baseline corrections (anchors, counts, drift)

1. **CONS-I01** Anchor+line drift sweep — mechanically replace 12 known drifts. No user input.
2. **CONS-C11** Test counts — §4.1 autostart "14 → 9"; §3.8 trigger "8 → 13"; §6.1 parallel updates. No user input.
3. **CONS-C12** Wire-contract snapshot count — §6.3 "41 → 42" + reference_doc_drift follow-up note. No user input.
4. **CONS-M03** "Start minimized" fabrication — §4.9 rewrite to actual GeneralTab contents. No user input.

### Phase B — User-decision capture (blocks everything downstream)

5. **U1 (CONS-C02 + CONS-C01)** — request user answer on scope enumeration. Once chosen, §3.8 table expands to ≥13 rows with per-loop dispositions, OR §1 Overview scope-reduces. **User-input required**.
6. **U2 (CONS-C05 + CONS-I07)** — request user answer A/B/C on `should_run_now` overnight. Once chosen, §3.4 truth table and §3.8 final para update. **User-input required**.
7. **U3 (CONS-C06)** — user picks test-harness shape. Once chosen, §6.1 Feature 1 rewrites with concrete test strategy. **User-input required**.
8. **U4 (CONS-C07)** — user picks Linux CI strategy. Once chosen, §6.1 Feature 2 rewrites. **User-input required**.
9. **U5 (CONS-I02)** — user picks chrono-tz placement. Once chosen, §3.7 and §9 Q1 resolved into §7 Decisions D16. **User-input required**.
10. **U6 (CONS-I03)** — user picks tray propagation. §3.11a added. **User-input required**.
11. **U7 (CONS-I04)** — user picks trigger refactor scope. §3.8a added if in-scope. **User-input required**.
12. **U8 (CONS-I05)** — user picks autostart error-type strategy. §4.10 clarified. **User-input required**.
13. **U9 (CONS-I13)** — user picks first-run prompt strategy. §4.7a or equivalent added. **User-input required**.

### Phase C — Critical rewrites (depend on Phase B)

14. **CONS-C01** — §3.8 row 4 rewrite (no current gate); Phase 9 adds analysis-loop gate. Depends on U1.
15. **CONS-C02** — §3.8 table expansion to ≥13 rows. Depends on U1.
16. **CONS-C03** — §3.9 rewrite: FIFO-exit semantics + pre-flush drain + no-in-window-PII proof. Depends on CONS-C02.
17. **CONS-C04** — §3.7 DST semantics rewrite (fires twice on fall-back; skip case is a user-visible anomaly). Add integration test.
18. **CONS-C05** — §3.4a subsection added + §3.4 truth table extended. Depends on U2.
19. **CONS-C07** — §6.1 Feature 2 Linux CI strategy rewritten. Depends on U4. Plus `autostart.rs:389-401` fix (returns Err on non-zero); covered by CONS-C10.
20. **CONS-C08** — §3.9 last paragraph rewrite (new pattern, not mirror of `with_capture_paused`).
21. **CONS-C09** — §2.3 expanded to note 200-with-silent-failure today; new D15 in §7 (200→500 behavior change); new Q in §9.
22. **CONS-C10** — §4.10 add command-non-zero row + timeout; `autostart.rs:137-141` (macOS), `:389-401` (Linux), Windows path — all return Err on non-zero exit.

### Phase D — Important rewrites (depend on Phase B/C or independent)

23. **CONS-I06** — §3.8 helper extraction (`tracking_schedule_helper.rs`). Independent.
24. **CONS-I08** — §9 Q3 → §7 D19 `MAX_BATCH_SIZE = 1000`; §5.9 "Select all" scope explicit. Independent.
25. **CONS-I09** — §5.6 lists 3 exact lines for `tagged_count → affected_count`. Independent.
26. **CONS-I10** — §2.1 regulatory scope expanded (CCPA + state acts). Independent.
27. **CONS-I11** — §3.4 composition rule includes consent; §3.4.a conflict-resolution. Independent.
28. **CONS-I12** — §9 Q8 → §7 D20 popover-shows-all. Depends on U10 (default: yes).
29. **CONS-I13** — §4.7/§4.9/§4.10 first-run prompt + repair UI + error-copy mapping. Depends on U9.
30. **CONS-I14** — §3.7a clock-irregularity subsection + 60s notification debounce. Independent.
31. **CONS-I15** — §4.3 `OnceLock<bool>` memoization for `has_systemctl`. Independent.
32. **CONS-I16** — §6.1 Feature 3 expanded test list (FK, concurrent, empty, cache). Independent.

### Phase E — Minor polish (after Criticals + Importants)

33. **CONS-M01** — §6.4 Korean term uniformity. Depends on U11.
34. **CONS-M02** — §6.5 Blackout-sweep bullet.
35. **CONS-M04** — §5.8 ARIA out-of-scope note.
36. **CONS-M05** — §3.11 field rename `tracking_schedule_notifications → tracking_schedule_enabled`.
37. **CONS-M06** — §3.14 new + §7 D22 single-schedule-multi-window.
38. **CONS-M07** — §3.4 row 7 `*` notation cleanup.
39. **CONS-M08** — §8.1 "no presets" rejected option.
40. **CONS-M09** — §4.3 + §4.7 cross-platform `is_enabled()` lie note.
41. **CONS-M10** — §6.2 full autostart log-site enumeration + `?mech → %mech`.
42. **CONS-M11** — §6.1 Feature 3 E2E path `e2e/timeline-actions.spec.ts`.
43. **CONS-M12** — §3.11 Q7 inlined: reuse Paused icon + change tooltip.
44. **CONS-M13** — §4.8 + §4.7 Alpine/musl/Snap notes.
45. **CONS-M14** — §6.5 `serial_test` for autostart integration tests.
46. **CONS-M15** — §6.5 OpenAPI/manifest integrity-gate note + responsible step.
47. **CONS-M16** — §6.5 STATUS.md/PHASE-HISTORY.md/companion-doc checklist.
48. **CONS-M17** — §6.4 es/ja/zh-CN fallback note. Depends on U12.
49. **CONS-M18** — §5.4 `maintenance.rs:418` annotation.

---

## 7. Expected remaining issues post-fix (sanity check)

Assuming Phase A–E complete with user decisions captured:

- **Expected zero Critical** remaining (all 12 have concrete fixes).
- **Expected zero Important** remaining (16 have fixes; 6 require user input but all have defaults).
- **Expected ≤3 Minor** remaining — these are defer-to-follow-up by design:
  - `BatchTagResponse` rename propagated to OpenAPI manual re-gen (will be caught by integrity gate — expected, not a spec issue).
  - Alpine/OpenRC/Snap edge cases — explicitly out-of-scope for Phase 9, TODO sprint item.
  - a11y ARIA gap on multi-select — pre-existing; out-of-scope.

- **Known follow-up TODOs created by Phase 9**:
  - `reference_doc_drift` for CLAUDE.md "41 wire codes"
  - TimeWindow unification (already in `project_next_tasks.md`)
  - i18n es/ja/zh-CN translations
  - `AutostartError` typed-error upgrade (if U8 → (a))
  - ARIA multi-select hardening
  - User-facing tracking-schedule guide + companion

- **Contract integrity gate risk**: OpenAPI + `http-interface-manifest.v1.json` deltas for new routes (`GET/PUT /api/tracking-schedule`, `GET /api/tracking-schedule/status`, `GET/PUT /api/autostart`, `DELETE /api/frames/batch-tags`). Plan must name responsible step or CI will red.

- **Not addressed by this synthesis** (because reviewers didn't raise):
  - Backward-compat for `active_days` enum in ScheduleConfig — confirmed unchanged.
  - `AppState` 3+ fields guardrail — spec §3.10 correctly addresses (no new atomic).
  - Port-trait contract test coverage for new ports — no new ports added by Phase 9.
  - ADR-037/040 event-sourcing — no event emission introduced.

---

## Appendix A — R1 Critical evidence verification log

| Finding | Evidence command | Result | Verdict |
|---------|------------------|--------|---------|
| R1.C1 | `rg should_run_now src-tauri/src/` | only `monitor.rs:203`, `mod.rs:548`, `mod.rs:584` | ✅ Confirmed |
| R1.C1 | `rg active_hours\|schedule src-tauri/src/scheduler/loops/intelligence.rs` | 0 matches | ✅ Confirmed |
| R1.C2 | `rg with_capture_paused\|capture_paused crates/oneshim-network/src/batch_uploader.rs` | 0 hits | ✅ Confirmed |
| R1.C2 | `batch_uploader.rs:74` | `pub fn with_health_flag(mut self, flag: Arc<AtomicBool>) -> Self` | ✅ Confirmed |
| R1.C3 | `grep -c '#\[test\]' src-tauri/src/autostart.rs` | 9 | ✅ Confirmed |
| R1.C4 | `grep -c '#\[test\]' crates/oneshim-vision/src/trigger.rs` | 13 | ✅ Confirmed |
| R1.C5 | `sed -n '548,571p' src-tauri/src/scheduler/mod.rs` | `hour >= start && hour < end` no-wrap | ✅ Confirmed |
| R1.C6 | `grep -c "^[a-z]" crates/oneshim-core/tests/wire_contract_snapshot.expected.txt` | 42 | ✅ Confirmed |
| R1.C7 | `sed -n '83,98p' crates/oneshim-web/src/handlers/tags.rs` | silent warn! + Ok always | ✅ Confirmed |

## Appendix B — R2 Critical evidence verification log

| Finding | Evidence command | Result | Verdict |
|---------|------------------|--------|---------|
| R2.C1 | `rg should_run_now\|active_hours\|capture_paused src-tauri/src/scheduler/loops/` | only `monitor.rs`, `health.rs` | ✅ Confirmed |
| R2.C1 | `sed -n '60,135p' src-tauri/src/scheduler/loops/events.rs` | Process/Input/Clipboard/File un-gated | ✅ Confirmed |
| R2.C2 | follow-on from R2.C1 — enqueue_event in events.rs un-gated | pre-window + in-window events mix in queue | ✅ Confirmed |
| R2.C3 | analytical from DST semantics of `hhmm.format("%H:%M")` comparison | fires twice on fall-back | ✅ Confirmed (analytical) |

## Appendix C — R3 Critical evidence verification log

| Finding | Evidence command | Result | Verdict |
|---------|------------------|--------|---------|
| R3.C1 | `rg mock_clock\|FakeClock\|MockClock\|TestClock\|fake_clock` | only in R3 review itself | ✅ Confirmed |
| R3.C2 | `sed -n '380,410p' src-tauri/src/autostart.rs` | `systemctl --user enable` non-zero swallowed | ✅ Confirmed |
| R3.C2 | `.github/workflows/ci.yml` test job runs ubuntu-latest with no user D-Bus bootstrap | correct | ✅ Confirmed (cross-checked via R3 citation) |

---

_End of synthesis._
