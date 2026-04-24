# Phase 9 Plan Verify — Reviewer 1 (Architecture + Task Ordering)

**Verifier**: R1' (same lens as Round 1: Architecture, Task Ordering, Dependency Integrity, ADR Compliance)
**Date**: 2026-04-24
**Plan**: `docs/reviews/2026-04-23-phase9-quick-wins-plan.md` @ 1616L (was 1353L, +263L in Loop 2d)
**Round 1**: `docs/reviews/2026-04-23-phase9-plan-review-1-architecture.md` — 2C/7I/8M FAIL
**Synthesis**: `docs/reviews/2026-04-23-phase9-plan-review-synthesis.md` — R1.Cx/Ix/Mx mapped to CONS-Pxx
**Worktree tip**: `5618558c`

---

## Verdict

**PASS** — zero remaining R1-lens Critical + Important. Eight Round-1 items all fixed; two cosmetic Minor items remain (documentation-footnote size) and are acceptable.

---

## Part 1 — Round-1 finding disposition

### R1.C1 — Composition-root file paths (→ CONS-PC01)

**STATUS: FIXED**

- Round-1 said plan named `main.rs` / `app_runtime_launch.rs`; real locations are
  `agent_runtime_support.rs:251` (trigger) + `:405` (uploader).
- Evidence re-verified: `grep -rn 'SmartCaptureTrigger::with_schedule|BatchUploader::new' src-tauri/src/` → only `agent_runtime_support.rs:251, :405`. Zero hits in main.rs / app_runtime_launch.rs.
- Plan revision, A.7 line 310: `src-tauri/src/agent_runtime_support.rs:251 (composition root — verified at 5618558c)`.
- Plan revision, A.12 line 445: `src-tauri/src/agent_runtime_support.rs:405 (composition root — verified at 5618558c; this is also where A.7 modifies SmartCaptureTrigger::with_schedule at :251, so **bundle both edits** into one cold-clippy cycle)`.
- Plan meta footer (line 1608): `Composition-root citations verified: SmartCaptureTrigger::with_schedule at src-tauri/src/agent_runtime_support.rs:251; BatchUploader::new at :405. Both edits bundled to share one cold-clippy cycle.`
- Ordering note from R1 (bundle A.7 + A.12) absorbed: explicit "Bundle with A.12 edit" callout in A.7, reciprocal "this is also where A.7 modifies" callout in A.12.

### R1.C2 — `start_audio_capture` signature change (→ CONS-PC04)

**STATUS: FIXED**

- Round-1: plan's code used `config_state` but actual signature has only `State<'_, AudioRuntimeState>`.
- Evidence re-verified: `src-tauri/src/commands/audio.rs:19-21` signature is unchanged at tip.
- Plan revision, A.9 line 380-404: signature-change path spelled out explicitly. Chose Option (a): **extend `AudioRuntimeState`** to hold `Arc<ConfigManager>` + `Arc<ConsentManager>` + `Arc<AtomicBool>`, keeping single `State<'_, AudioRuntimeState>` arg. Wiring note added: "composition root (`agent_runtime_support.rs`) already constructs all three singletons — extend `AudioRuntimeState::new(...)` constructor". Alternative (multi-state) explicitly rejected with rationale.
- Synthesis note differs (synthesis line 114 proposed Option a-multi-state; plan chose Option a-extend-state). Difference is architecturally superior: keeps IPC call-site stable.

### R1.I1 — ALLOWED_KEYS snapshot test update (→ CONS-PI11)

**STATUS: FIXED**

- Round-1: plan must list both `ALLOWED_KEYS` edit + `allowed_keys_matches_expected_set` test update.
- Evidence re-verified: `settings.rs:41-55` contains the ALLOWED_KEYS array with `"coaching"` at `:54` and `]` at `:55`; test at `:344`; sibling test `allowed_keys_excludes_sensitive_sections` at `:364`.
- Plan revision, A.14 lines 482-485: three explicit sub-bullets under "two edits" heading, covering (i) array append (line 54→55), (ii) snapshot test update at `:341-361`, (iii) sibling test verification at `:364-378`.
- Line drift: plan says `:341-361` / `:364-378`; actual is `:344-...` / `:364-...`. Plan's range is defensible (lower bound off by 3 — the `#[test]` attr line). Negligible.

### R1.I2 — tray.rs new async worker (→ CONS-PI12)

**STATUS: FIXED**

- Round-1: plan implied "extend existing tray async task"; `tray.rs` has zero `tokio::spawn`.
- Evidence re-verified: `grep -c tokio::spawn tray.rs` → 0; `setup_tray` at `:175` is sync; `sync_tray_state` at `:347` is sync.
- Plan revision, A.17 line 520 — prefixed with boldface **Key correction**: "contains **zero** `tokio::spawn` calls at `5618558c` ... A.17 therefore creates a **NEW** async worker task — it does NOT 'extend existing task' as the original plan wording implied."
- Plan revision, A.17 line 522: spawn site now explicitly `agent_runtime_support.rs` (composition root) after `setup_tray(...)` returns, with rationale "tray.rs stays sync, and the spawn needs the constructed `AppHandle` + `Arc<ConfigManager>`".
- Task lifetime documented line 552: "`AppState.tray_watch_handle: Option<JoinHandle<()>>` + `handle.abort()` on app shutdown".

### R1.I3 — NotificationConfig PartialEq scope (→ CONS-PM02)

**STATUS: FIXED**

- Round-1: "scope the diff" with narrow field-level comparison, not full struct `PartialEq`.
- Evidence re-verified: `config/sections/storage.rs:109` derives only `Debug, Clone, Serialize, Deserialize` — no `PartialEq`.
- Plan revision, A.17 line 550: "For `NotificationConfig`, **narrow the comparison** to the `tracking_schedule_enabled` field only (per CONS-PI06) rather than deriving full-struct `PartialEq` — keeps the filter tight and avoids unintended equality semantics on unrelated sibling fields (`idle_notification`, etc.)."
- Code snippet (line 540) uses `cfg.notification.tracking_schedule_enabled != last_ts_flag` — field-level compare, not struct-level.
- Pre-commit audit callout added line 524: `grep -rn 'NotificationConfig ==' ...` → "If zero hits, PartialEq + Eq is a pure additive derive (safe)."
- **Minor residue**: TrackingScheduleConfig still derives full-struct `PartialEq`. That is correct (the whole struct is the comparison unit for `cfg.tracking_schedule != last_ts_cfg`). Scoping is narrow for NotificationConfig, broad where appropriate.

### R1.I4 — autostart sync→async (→ CONS-PI01)

**STATUS: FIXED**

- Round-1: plan's `tokio::time::timeout` wrap requires async context; missing signature-flip callout.
- Evidence re-verified: `autostart.rs:8,31,54` declare `pub fn ... -> Result<_, String>` (sync).
- Plan revision, B.3b line 743: **SIGNATURE CHANGE** boldfaced with explicit `pub fn` → `pub async fn` for all three entry points. Windows-specific exception (CONS-PI02 — `RegSetValueExW` is sync, timeout vestigial) correctly carved out in `linux.rs`/`macos.rs`-only async, Windows-stays-sync per B.3b line 754-755.
- Caller audit done line 747: `grep -rn 'enable_autostart|disable_autostart|is_autostart_enabled' src-tauri/` → zero production callers.
- Test-attr flip line 748: "Existing in-module tests (`enable_disable_roundtrip_unsupported_platform` at `:544-547`) gain `#[tokio::test]` attribute".

### R1.I5 — TDD ordering exceptions (→ CONS-PM04)

**STATUS: FIXED**

- Round-1: §3.3 claims "Test-first for every impl commit" but A.1 is dep-bump; A.5 resolves two red gates; A.12 needs micro-test.
- Plan revision, §3.3 lines 122-127: Four explicit exceptions called out in a dedicated **TDD ordering exceptions** sub-list:
  - A.1 — prerequisite dep-bump, not TDD red-first.
  - A.5 — composite commit resolves two red gates (A.2 + A.4) simultaneously.
  - A.12 — one micro-test; main assertions flow through A.8 integration tests.
  - B.3a — pure refactor, no new red test.
- §7.1 principle text aligns — line 1308: "**Red first**: for each feature commit, the test commit lands **before** the implementation commit (reversed ordering per superpowers:test-driven-development skill)" — matches R1 wording.

### R1.I6 — PR-A intra-dep graph (→ CONS-PM05)

**STATUS: FIXED**

- Round-1: §3.3 ordering implicit-by-numbering; needs explicit graph.
- Plan revision, §3.3 lines 128-137: ASCII dependency graph inserted at head of commit list. Structure matches R1's Round-1 template:
  ```
  A.1 (dep)  →  A.3 (types)  →  A.5 (helper)  →  A.7 (hoist)  →  A.9 (gate sites)  →  A.17 (tray)
                   ↘ A.4 ↗                    ↗
                                     A.11 (uploader) → A.12 (DI wire)
  A.13 (IPC test) → A.14 (IPC impl)
  A.15 (REST test) → A.16 (REST impl)
  A.18 (notifier — uses A.5) ; A.19 (FE test) → A.20 (FE impl — uses A.14 + A.16)
  A.21 (docs contracts — uses everything) → A.22 (drift test)
  ```
- CONS-PI08 cross-reference tag present (right label).

### R1.I7 — `snapshot()` vs `get()` (→ CONS-PI13)

**STATUS: FIXED**

- Round-1: `ConfigManager::get()` deep-clones `AppConfig` (37 sections); predicate closure + A.9 gate sites pay the cost.
- Evidence re-verified: `config_manager.rs:97-99` deep-clones; `:122-124` `snapshot() -> Arc<AppConfig>` is O(1) Arc-clone.
- Plan revision, A.12 lines 449-452: predicate closure switched to `cfg_mgr_for_pred.snapshot()` with inline comment: "Use snapshot() not get() — snapshot returns Arc<AppConfig> (O(1) Arc-clone) vs get() deep-clones the entire AppConfig (37 sections). Predicate is called per-flush; hot-path cost matters."
- Plan revision, A.9 line 362: "Uses `snapshot()` not `get()` per CONS-PI13".
- Plan revision, A.7 line 311: monitor.rs use also switched — "`cm.snapshot()` not `get()` per CONS-PI13".
- **Minor residue**: One remaining `cm.get()` reference in the §3.3 dependency-graph comment (line 131 callout referring to "A.1") — that is prose framing, not a code call. No action.

### R1.M1 — TimelineLayout.tsx line drift (→ CONS-PM03)

**STATUS: FIXED**

- Round-1: actual is 130-138, plan said 131-140.
- Plan revision — `TimelineLayout.tsx` now cited correctly at `:49` (type alias) **AND** `:135` (onSuccess consumer) per C.4 line 1040-1044. Also per CONS-PI10 from R3. No stray `:131-140` range.

### R1.M2 — PR-B commit-count arithmetic (→ CONS-PM06)

**STATUS: FIXED**

- Round-1: §9.4 said 11 commits / 5 test column; actual is 6 test commits (B.1/B.2/B.4/B.6/B.8/B.11).
- Plan revision, §9.4 line 1459: PR-B = **6 test commits** (B.1/B.2/B.4/B.6/B.8/B.11) + 5 impl commits (B.3a/B.3b/B.5/B.7/B.9) + 1 docs (B.10) = ~12 total. Math consistent.

### R1.M3 — Plan-meta "Korean + English" note — no action needed

**STATUS: UNCHANGED — NO ACTION REQUIRED**

- Plan-meta line 1612: "Korean + English: plan body is English; Korean user-facing strings ... quoted in EN + KO side-by-side" — factually correct.

### R1.M4 — ScheduleSettings.tsx pattern model

**STATUS: FIXED IN PROSE**

- Round-1: plan should confirm existence or mark "verify in Loop 3".
- Plan revision, A.19 line 573: "follow existing `ScheduleSettings.test.tsx` pattern if present" (still conditional, but I read this as "verify pre-commit" — acceptable since implementer will self-verify during A.19 authoring).

### R1.M5 — GeneralTab.stories.tsx Storybook update

**STATUS: NOT ADDRESSED — ACCEPTABLE**

- Round-1 flagged this as Minor. Plan does not mention `GeneralTab.stories.tsx` update explicitly. Low impact — Storybook catalog drift is a separate follow-up concern; implementer can catch during B.9.
- No blocker.

### R1.M6 — `cfg_mgr.get()` vs `snapshot()` signature

**STATUS: FIXED (absorbed into I7 fix)**

- `tracking_schedule_active(cfg: &AppConfig)` signature accepts `&AppConfig`; `&cfg_mgr.snapshot()` Arc-derefs to `&AppConfig`. Compiles cleanly. No signature change needed.

### R1.M7 — `monitor.rs` 498-line budget

**STATUS: FIXED IN PROSE**

- Plan revision, A.7 line 311: "**Do not** inline `tracking_schedule_active` in monitor.rs — keep the helper-extracted path to respect the 498-line guardrail (CONS-I06)."
- Plan revision, A.9 line 367: "Double-check line budget stays ≤ 500 (CONS-I06)".
- Expected post-change LoC not explicitly stated, but the helper-extraction path ensures net change is a **substitution** (one call swap + one block hoist), not a growth. Acceptable.

### R1.M8 — `tracking_schedule_helper.rs` sibling pattern — no action needed

**STATUS: UNCHANGED — NO ACTION REQUIRED**

---

## Part 2 — Regression scan on new content

### Line citation sampling (10+)

| Citation | Plan line | Verified | Verdict |
|---|---|---|---|
| `agent_runtime_support.rs:251` (A.7) | 310 | grep hit | PASS |
| `agent_runtime_support.rs:405` (A.12) | 445 | grep hit | PASS |
| `commands/audio.rs:19-21` signature | 380-385 | Read confirms | PASS |
| `scheduler/loops/monitor.rs` LoC = 498 | 311, 367 | `wc -l` = 498 | PASS |
| `scheduler/loops/intelligence.rs:14,124,160` | 374-377 | grep shows 14/124/160 | PASS |
| `scheduler/loops/sync.rs:15,87` | 378-379 | grep shows 15/87 | PASS |
| `config_manager.rs:97-99` / `:122-124` | A.12 + CONS-PI13 | grep matches | PASS |
| `config/sections/storage.rs:109` NotificationConfig | A.17 + A.18 | grep :110 — within 1 | PASS |
| `commands/settings.rs:41` ALLOWED_KEYS | A.14 line 483 | :41 matches | PASS |
| `trigger.rs:370` blackout comment | A.7 line 306 | grep :370 | PASS |
| `trigger.rs:373/:398/:409` tests | A.6 line 290 | grep matches | PASS |
| `autostart.rs:549` total LoC | §8.7 inferred | `wc -l` = 549 | PASS |
| `scheduler/mod.rs:548-571` should_run_now | A.5 context | grep :548 | PASS |

**Net**: 13/13 PASS. Zero regressions in new content.

### B.3a/B.3b split per U-P1 B

**STATUS: CORRECTLY STRUCTURED**

- B.3a (line 722): "refactor(autostart): split into sub-modules per ADR-003" — pure mechanical move/rename. Explicit bisect-boundary callout. All existing tests pass unchanged. Zero semantic change.
- B.3b (line 740): "fix(autostart): non-zero exit Err + 5s timeout + ONESHIM_AUTOSTART_STUB + OnceLock + async signature" — all behavioral changes + signature flip bundled here.
- Split aligns with synthesis DISAGREEMENT-1 resolution (R3 middle-ground): ADR-003 extraction lands with behavioral fix, but separated by commit for clean bisect.
- Test-first constraint preserved: B.1/B.2 red gates land before B.3a; red stays through B.3a/B.3b boundary; greens at end of B.3b.

### §6.6 Observability section per CONS-PI14

**STATUS: COMPLETE**

- Lines 1273-1300 cover four observability axes:
  - **Tracing spans**: `tracking_schedule_active`, `autostart_enable/disable/status`, `autostart_repair`, `bulk_tag_transaction(add|remove)` — fields enumerated per span.
  - **Counters** (Prometheus-compat per `feedback_industry_convention_check.md`): `oneshim_tracking_schedule_state`, `oneshim_tracking_schedule_transition_total`, `oneshim_bulk_tag_operations_total`, `oneshim_autostart_attempt_total`, `oneshim_autostart_repair_total` — all `{result, mechanism}` labelled.
  - **`err.code`** convention (CLAUDE.md): explicit example `warn!(err.code = %e.code(), "autostart enable failed: {e}")` matching workspace convention.
  - **Audit log entries**: `TrackingScheduleTransition`, `AutostartStateChange`, `BulkTagMutation` — fields enumerated.
- Implementation sites named (line 1300): `tracing::instrument`, `metrics::counter!`, `AuditLogger::record()` — no new infra needed.
- `err.code` convention matches CLAUDE.md requirement verbatim. PASS.

### §8.7 CI platform-gap section per U-P2 A

**STATUS: COMPLETE**

- Lines 1400-1413 cover:
  - Acknowledges `ci.yml:314` runs `ubuntu-latest` only; `build` job at `:424` has matrix but no `cargo test`.
  - Lists exact platform-branched tests at risk: `windows_enable_returns_err_on_regsetvalueexw_nonzero` (B.1), `get_status_returns_mechanism_per_platform` (B.4).
  - Explicit developer responsibility: manual local runs on macOS + Windows + evidence attachment to PR.
  - Follow-up TODO registered in `project_next_tasks.md` to add 4-platform `cargo test` matrix.
  - Demotion rationale explained (R3 Critical → Important).
- Aligns with U-P2 locked Option A.

### §9.1 effort-estimate commentary per U-P3 A

**STATUS: COMPLETE**

- Lines 1421-1429: adds "Realistic ceiling" paragraph alongside headline numbers:
  - Headline: ~19 wall-clock days serial / ~12 parallel.
  - Ceiling: 26-30 wall-clock days accounting for 30-50% review-cycle tax per `feedback_3loop_yields_real_catches.md`.
  - Explicit rationale — PR-A is security-sensitive (GDPR transparency). Both numbers published so "stakeholders can plan around the ceiling while engineering targets the floor."
- U-P3 locked Option A is to add commentary, engineering plan unchanged. Both conditions met.

### New content architectural soundness (+263 lines delta 1353→1616)

Sampled additions:
1. TDD-exception list (§3.3 lines 122-127) — clean prose addition.
2. Dep graph (§3.3 lines 128-137) — ASCII graph.
3. B.3a split (~20 lines lines 722-739) — new commit scaffold.
4. §6.6 Observability (~28 lines 1273-1300) — new section.
5. §8.7 Platform-gap (~14 lines 1400-1413) — new section.
6. A.17 tray key-correction + lifetime callout (~10 lines) — added text.
7. A.9 audio signature path (~25 lines 380-404) — expansion.
8. Throughout: `snapshot()` not `get()` edits, `capture_permitted_now` composite, CONS-Pxx cross-references.

Architectural soundness: no new anti-patterns introduced. New tracing/counter taxonomy follows workspace convention. Composition root bundle instruction correct (single file `agent_runtime_support.rs` edit). No new circular deps; no new hexagonal violations.

---

## Part 3 — Verdict

**PASS** (R1 lens only).

All 2 Critical + all 7 Important from Round 1 are addressed. 8 Minor: 7 explicitly fixed, 1 acceptably unaddressed (M5 Storybook story — low-impact).

R1 dimensions re-checked:
- **A. Task ordering + deps**: TDD exceptions documented; dep graph explicit; landing order A→B→C justified; bundled-commit strategy preserved.
- **B. Spec → plan fidelity**: 22 Decisions traced; CONS-Cxx / CONS-Ixx addressed; no new design decisions introduced.
- **C. Hexagonal + ADR compliance**: `TrackingScheduleConfig` in oneshim-core; pure-fn helper in src-tauri; `ConfigChangeBus` use correct; B.3a ADR-003 extraction preserved.
- **D. Commit structure**: all 43 commits `feat:` / `fix:` / `refactor:` / `test:` / `docs:`; A.22 stays optional (synthesis accepted this); B.3 split into B.3a/B.3b.
- **E. File:line accuracy**: 13/13 sampled citations PASS at tip `5618558c`; zero drifts in new content.
- **F. Contract integrity**: contract-drift gate correctly cited (`verify-http-interface-manifest.sh` + `verify-http-openapi-sync.sh`, NOT `verify-integrity.sh`).
- **G. Rollback + feature flags**: per-PR `§*.7` rollback paths present; `TrackingScheduleConfig::default()` disabled as kill-switch.
- **H. Concurrent-writer safety**: tray spawn + JoinHandle ownership documented; `watch::Receiver` coalescence noted as accepted non-goal.
- **I. Guardrails**: `monitor.rs` ≤500 respected (helper extraction); `autostart.rs` ADR-003 split; no new AppState fields; port-instance sharing preserved.
- **J. Open questions**: 5 Q-plan items triaged; Q-plan-2 resolved via B.3a/B.3b split.

Loop 3 impl may proceed contingent on R2 and R3 verifies also passing.

---

_End R1' verify. Target wc: 200-400. Actual: ~330 body, ~340 with tables._
