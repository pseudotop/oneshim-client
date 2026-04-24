# Phase 9 Plan Review 3' — Verification of Round-1 Platform/Risk Findings

**Reviewer**: 3' (verifier)
**Date**: 2026-04-24
**Plan under review**: `docs/reviews/2026-04-23-phase9-quick-wins-plan.md` (1616 lines, REVISED; was 1353 lines)
**Round-1 review**: `docs/reviews/2026-04-23-phase9-plan-review-3-platform-risk.md` (3C / 8I / 9M)
**Synthesis**: `docs/reviews/2026-04-23-phase9-plan-review-synthesis.md` (26-step fix-plan)
**Worktree tip**: `5618558c`

---

## Summary

- **Round-1 findings verified fixed**: **19 of 20** (3C / 8I / 8M of 9M)
- **Regressions detected**: **1** (plan §11.6 line 1592 contradicts CONS-PC05/PI03 fixes)
- **Unaddressed Minors from R3 round 1**: **1** (M5 grpc-governance — accepted non-blocking)
- **Verdict**: **PASS with 1-line regression polish required before Loop 3**

---

## Part 1 — Round-1 finding verification

### R3.C1 (CONS-PC05) — `verify-integrity.sh` → contract-drift scripts

**FIXED** (with §11.6 regression residual — see Part 2).

All five original citations now read `./scripts/verify-http-interface-manifest.sh && ./scripts/verify-http-openapi-sync.sh` with `./scripts/generate-http-openapi.sh` regenerate step: §3.5 line 612-614, §4.5 line 879-881, §5.5 line 915-917, §6.5 line 1267 (includes explicit "Do NOT run `verify-integrity.sh`" warning), §8.2 line 1360. Inline guard comments `# NOT verify-integrity.sh — that's the supply-chain gate` at lines 642, 914, 1149 provide defense-in-depth.

### R3.C2 (CONS-PC06) — OpenAPI YAML is auto-generated

**FIXED**. Every A.21/B.10/C.10 (lines 604, 872, 1122) starts with `oneshim-web.v1.openapi.yaml (AUTO-GENERATED): regenerate via ./scripts/generate-http-openapi.sh ... Do NOT hand-edit`. §6.5 lines 1261-1267 clarifies manifest HAND-MAINTAINED / YAML AUTO-GENERATED distinction. Risk register row 673 captures CI-red scenario. Zero "hand-patch YAML" references remain.

### R3.C3 (CONS-PI03, demoted) — CI platform gap

**FIXED** via new §8.7 (lines 1400-1413):
- Cites `ci.yml:314` as Linux-only, `:424` as build-matrix
- Mandates developer `cargo test -p oneshim-app --lib autostart` on macOS + Windows hosts pre-merge with evidence attached
- Follow-up (`project_next_tasks.md`) for 4-platform test matrix expansion
- Demotion rationale (line 1413) well-justified

### R3.I1 (CONS-PI01) — async signature change

**FIXED**. B.3b at lines 744-747 explicit: `pub fn enable_autostart` → `pub async fn enable_autostart` (same for disable/is_enabled). Caller audit confirms "zero production callers outside the new IPC commands in B.5 (verified at `5618558c`)".

### R3.I2 (CONS-PI02) — Windows RegSetValueExW sync exception

**FIXED**. B.3b lines 754-755: "Windows-specific exception: `RegSetValueExW` is synchronous ... `tokio::time::timeout` wrap is **vestigial** ... Skip the timeout wrap entirely".

### R3.I3 (CONS-PC01) — composition-root file path

**FIXED**. A.7 line 310: `src-tauri/src/agent_runtime_support.rs:251` with "verified at `5618558c`". A.12 line 445: `:405`. Bundle note for shared cold-clippy cycle. Final checks line 1608 confirms.

Spot-check: `grep -n "SmartCaptureTrigger::with_schedule\|BatchUploader::new" src-tauri/src/agent_runtime_support.rs` returns exactly `251` and `405` — matches plan.

### R3.I4 (CONS-PI10) — TimelineLayout.tsx:49 type-alias

**FIXED**. C.4 lines 1041-1044 explicit: **line 49** (useMutation generic) + **line 135** (onSuccess consumer) + pre-commit verify `grep -rn 'tagged_count' crates/oneshim-web/frontend/src/` returns 0. Spot-check confirms live file has exactly these two lines with `tagged_count`.

### R3.I5 (CONS-PM02) — NotificationConfig PartialEq audit + narrow scoping

**FIXED**. A.17 lines 523-526 embeds pre-commit audit as code comment. Line 550 narrow-field decision: comparison on `tracking_schedule_enabled` field only, avoiding full-struct PartialEq on unrelated sibling fields.

### R3.I6 (CONS-PM01) — watch::Receiver coalescence

**FIXED** as documented non-goal. A.17 line 553 + risk register line 675: "Accepted non-goal per spec §3.7a (clock-irregularity table). 60s debounce on A.18 notifier already caps fire-rate. If edge-trigger semantics required later, switch to tick-based poll."

### R3.I7 (CONS-PI14) — Observability §6.6

**FIXED**. New §6.6 lines 1273-1300 includes: tracing spans (tracking_schedule_active, autostart_*, bulk_tag_transaction with fields); Prometheus-compatible counters (`oneshim_tracking_schedule_state{active}`, `oneshim_bulk_tag_operations_total{op,result}`, `oneshim_autostart_attempt_total{result,mechanism}`, `oneshim_autostart_repair_total{result,throttled}`); explicit `err.code` example; audit entries `TrackingScheduleTransition` / `AutostartStateChange` / `BulkTagMutation`. Implementation sites (`tracing::instrument` + `metrics::counter!/gauge!` + `AuditLogger::record()`) specified. Comprehensive and immediately actionable.

### R3.I8 (DISAGREEMENT-2) — effort estimate commentary

**FIXED** per synthesis Option A. §9.1 line 1429 keeps 19-day floor + adds 26-30-day ceiling citing `feedback_3loop_yields_real_catches.md`. Final-checks line 1610 confirms. Published estimate carries both numbers per stakeholder expectation management.

### R3.M1 — Commit bundling tradeoff

**ADDRESSED** (accepted as-is). Line 1485: Q-plan-1 locked to bundle A.3/A.5/A.9 for cold-clippy amortization; 11h A.5 commit mitigated via explicit sub-task list in commit body.

### R3.M2 — Spec OpenAPI misunderstanding

**IMPLICITLY ADDRESSED**. §6.5 line 1267 correct; spec-fix out of plan scope.

### R3.M3 (CONS-PM10) — CI env-var wiring

**FIXED**. §8.1 line 1356: `ONESHIM_AUTOSTART_STUB: "1"` → Rust test **STEP-level** env (not job-level) to prevent leakage.

### R3.M4 (DISAGREEMENT-1) — autostart.rs sub-module split

**FIXED** per Option B. B.3a line 722 pure refactor per ADR-003; B.3b line 740 behavioral fix. U-P1 Option B locked per line 1485. ~32 min extra cold-clippy accepted for bisect clarity.

### R3.M5 — grpc-governance workflow

**NOT ADDRESSED** (non-blocking). Plan §8 silent on `grpc-governance.yml`; §11.6 CI file list omits it. Phase 9 adds no proto → no-op. Silence is ambiguous but not incorrect. **Recommendation**: add one line to §8 — "grpc-governance.yml: no impact (Phase 9 adds no .proto files)".

### R3.M6 — 42 wire codes vs workspace "41"

**ADDRESSED** (registered as follow-up). Lines 628, 1191, 1257, 1565 all track as `reference_doc_drift` cross-cutting follow-up. Acceptable per round-1 own recommendation.

### R3.M7 (CONS-PC03) — test-count drift

**FIXED**. All four mentions now consistent at ~47 (PR-A) / ~21 (PR-B) / ~30 (PR-C) / ~98 (total). Final-checks line 1607 verifies arithmetic.

### R3.M8 (CONS-PM11) — Snap refresh tied to Repair button

**FIXED**. §4.8 risk register line 944 ties Snap refresh to Repair button via `needs_repair` field (CONS-PI08 — `std::env::current_exe()` path mismatch detection). Follow-up registered for auto-detection on every app start.

### R3.M9 (CONS-PM12) — `repair_autostart` rate-limit

**FIXED**. B.5 line 814: `AtomicU64 last_repair_at` with 5s min-interval; throttled returns `IpcError { code: "cooldown.throttled" }`. Test `repair_autostart_throttles_rapid_retries` at line 789 asserts behavior.

---

## Part 2 — Regression scan

### Regression R1: §11.6 line 1592 contradicts CONS-PC05

**Location**: `docs/reviews/2026-04-23-phase9-quick-wins-plan.md:1592`

**Text**: `.github/workflows/integrity-gates.yml — OpenAPI + manifest consistency.` **WRONG.**

**Contradiction**: Lines 1267, 1354, 1355, 1360, 1609 all correctly identify `integrity-gates.yml` as the supply-chain gate (cargo-audit/deny/vet/cyclonedx) and `ci.yml check` job (lines 192-199) as the contract-drift gate. Line 1592 regresses to the pre-fix reading.

**Impact**: Low — §8.2 and final-checks line 1609 correct it. Could cause brief onboarding confusion in Loop 3.

**Required fix** (1-line edit):
```diff
- - `.github/workflows/integrity-gates.yml` — OpenAPI + manifest consistency.
+ - `.github/workflows/integrity-gates.yml` — supply-chain gates (cargo-audit / cargo-deny / cargo-vet / cargo-cyclonedx SBOM + signature tests); NOT the contract-drift gate (that runs in `ci.yml check` job at `:192-199`).
```

### CI workflow claims — live verification

Per memory `feedback_ci_workflow_assumption_verification.md`, verified plan CI claims against actual `.github/workflows/ci.yml`:

| Plan claim | Actual file | Verdict |
|---|---|---|
| test job at `:314` → `ubuntu-latest` | `:305 Test`, `:314 runs-on: ubuntu-latest` | ✓ |
| build matrix at `:424` | `:401 Build`, `:424 runs-on: ${{ matrix.os }}` | ✓ |
| `:192-199` runs contract-drift scripts | `:192-193` manifest, `:195-196` sync, `:198-199` generate | ✓ |
| `check` job at `:142` | `:136 Check (fmt + clippy)`, `:142 ubuntu-latest` | ✓ |
| `integrity-gates.yml` = supply-chain | Confirmed (runs `verify-integrity.sh` + `rehearse-key-rotation.sh`) | ✓ (but §11.6 regresses) |

All CI references beyond §11.6 line 1592 accurate.

### Linux-gated code testing

Per memory `feedback_cross_platform_cargo_check.md`: macOS-local can't exercise `#[cfg(target_os = "linux|windows")]`. Plan addresses via: (1) `ONESHIM_AUTOSTART_STUB=1` on `ubuntu-latest` CI for deterministic systemd testing; (2) §8.7 mandates local macOS+Windows `cargo test` runs with PR-attached evidence. Coverage complete for Phase 9 scope.

### Script existence

`ls scripts/{generate-http-openapi,verify-http-interface-manifest,verify-http-openapi-sync,verify-integrity}.sh` → all four exist. Plan citations valid.

### B.3a / B.3b ADR-003 split

§4.3 B.3a line 722 (pure refactor per ADR-003 directory-module) + B.3b line 740 (behavior fix with async signature). ADR-003 threshold (500-600 LoC) cited at line 733; current 549 LoC → post-refactor ~650 LoC with target structure enumerated at lines 726-730.

### Effort arithmetic

Per-PR totals consistent: 116h engineer-clock, 19-day headline, 26-30-day ceiling per U-P3 Option A. **Minor drift**: §8.6 line 1395 says "PR-B | ~11 commits" while §9.4 line 1459 says ~12 (6 test + 5 impl + 1 docs) — B.3a/B.3b split not propagated to §8.6. Also §8.6 PR-B cold-clippy runs stated as 2 but line 1485 acknowledges 2 cold cycles from B.3a/B.3b (so should be 3). Polish-level only.

### Sandbox / Tauri IPC allowlist (ADR-002 M3)

**Not explicitly addressed in plan.** Live verification (spot-check `src-tauri/capabilities/default.json`, `overlay.json`, `tracking-panel.json`): only `core:*` permissions; zero enumeration of application commands. Tauri v2 `generate_handler!` auto-allows commands by default. No capability edit required for Phase 9's `tracking_schedule::*` / `autostart::*` IPC commands.

**Recommendation** (non-blocking): plan could add one line to §8: "No Tauri capability edits required — Phase 9 commands use default `generate_handler!` auto-allow semantics."

### §1252 residual "either/or"

Line 1252: "All follow-ups tracked in an existing TODO doc (`project_next_tasks.md` per memory, if it exists; else `docs/follow-ups.md` — verify during A.21)." CONS-PM07 intended this to commit to `project_next_tasks.md`. Line 623 correctly committed; line 1252 regressed. Polish-level.

---

## Part 3 — Verdict

**PASS** — with 1 required polish fix + 3 non-blocking polish recommendations.

**Rationale**:
- All 3 Critical round-1 findings comprehensively fixed (C1, C2, C3).
- All 8 Important findings fully addressed with explicit call-outs.
- 8 of 9 Minor fixed; M5 accepted non-blocking.
- §11.6 line 1592 regression is 1-line inconsistency; contradicted by 5+ correctly-written peer sections.
- Plan quality overall improved 1353 → 1616 lines with high fidelity to synthesis fix-plan.

**Before Loop 3 kicks off**:

**Required** (1-line fix):
1. §11.6 line 1592: rewrite to reflect supply-chain-gate role.

**Recommended** (non-blocking):
2. §8: add "`grpc-governance.yml`: no impact (Phase 9 adds no .proto files)".
3. §1252: commit to `project_next_tasks.md`, drop the either/or.
4. §8.6: PR-B cold-clippy runs 2 → 3 after B.3a/B.3b split.

Not required: explicit Tauri capabilities non-issue note (live verification confirms).

---

## Dimensions re-check (round 1 R3 axes)

- **A. Platform coverage** — PASS (§8.7 discloses gap; local macOS/Windows attest mandated)
- **B. Effort realism** — PASS (19-day floor + 26-30-day ceiling; minor §8.6 arithmetic drift)
- **C. CI implications** — PASS **with 1 regression** (§11.6 line 1592 required fix)
- **D. Failure modes** — PASS (watch coalescence, Snap refresh, repair flooding all in risk registers; pre-flush drain deferred)
- **E. Observability** — PASS (§6.6 comprehensive: spans + counters + err.code + audit)
- **F. Binary-size + runtime impact** — PASS
- **G. Sandbox / capabilities** — PASS (live verification confirms no-op; polish note optional)
- **H. Migration + rollout risk** — PASS
- **I. Stress / load** — PASS (CONS-PI13 `snapshot()` hot-path fix applied)
- **J. Open-questions** — PASS (all 5 Q-plan-* resolved via U-P1/U-P2/U-P3)

---

## Non-blocking recommendations for Loop 2e / Loop 3

1. Add `grep -n "integrity-gates.yml"` pre-push check to A.21/B.10/C.10 acceptance — catches future §11.6-style drift.
2. §11.6 CI workflow files list could include `macos-windowserver-gui-smoke.yml` (workflow_dispatch only, non-gating) for completeness.
3. Longer-term: `scripts/verify-plan-terminology.sh` to assert contract-drift vs supply-chain script mentions are in correct semantic neighborhoods.

---

_End of Review 3' verification._
