# Phase 4 Updater Hardening — Ralph Loop Progress Tracker

**Mandate:** 3-loop discipline until each loop has 0 Critical + 0 Important issues.
1. **Loop 1 — Spec deep review** → iterate until 0 Critical + 0 Important.
2. **Loop 2 — Plan deep review** → iterate until 0 Critical + 0 Important.
3. **Loop 3 — Impl deep review** → iterate until no issues.

**Branch:** `feat/phase4-updater-hardening-spec`
**Spec path:** `docs/reviews/2026-04-18-phase4-updater-hardening-design.md`
**Ralph loop state:** `.claude/ralph-loop.local.md`

---

## Loop 1 — Spec Deep Review

### Iter 1 — 2026-04-18

- **Starting commit:** `a242924c` (initial spec, 487 LOC)
- **Reviewer:** superpowers:code-reviewer (agentId `a0e74089c62b2f0cb`)
- **Verdict:** **Ready to exit? No.** — 3 Critical + 6 Important + 7 Minor
- **Findings:**
  - **C-1** — Spec claims `require_signature_verification` flag flips false→true; actual default is already `true` (storage.rs:349-351). Public key already hardcoded (storage.rs:353-355). Breaking-change v0.5.0 premise invalid.
  - **C-2** — Spec proposes new `released_at` field on PendingUpdateInfo; `published_at` field already exists (update.rs:25) and is populated (update_coordinator.rs:446).
  - **C-3** — Rollback backup file selection unspecified; `.rollback.{ts}` files stack without a deterministic pick mechanism, no cleanup.
  - **I-1** — Probe I/O error path not addressed (probe crash fails rollback).
  - **I-2** — Windows shell-helper rollback unverified + UAC/PATH security-sensitive.
  - **I-3** — Test count inconsistency + tokio paused time doesn't accelerate std::fs.
  - **I-4** — installation_id None handling can race with first-launch scheduler spawn order.
  - **I-5** — cliff.toml + release_notes.md header work not concretely scoped.
  - **I-6** — Multi-key trust procedure conflates scheduled rotation with compromise.
  - **M-1 to M-7** — Prefix inconsistency, missing file refs, test name ambiguity, untyped timestamps, duplicated mention, missing CHANGELOG in files-modified, macOS app-bundle state file location.
- **Verified critical findings against code**: C-1 + C-2 confirmed with grep/read.
- **Status:** revision in progress

### Iter 2 — 2026-04-18

- **Starting commit:** `a242924c`
- **Ending commit:** `f44b7099` (spec rewrite addressing iter 1 Critical + Important + Minor)
- **Reviewer:** fresh code-reviewer (agentId `a03d360742aaf83ae`)
- **Verdict:** **Ready to exit? No.** — 2 new Critical + 5 Important + 7 Minor
- **Key findings:**
  - **C1** — `cliff.toml` already exists (~30 lines); my spec claimed "Created" which would have overwritten committed template.
  - **C2** — Backup filename format is `{binary_name}.rollback.{ts}` per `install.rs:378-392::backup_path_for`, not `.rollback.{ts}`. Cleanup glob would match nothing.
  - **I1** — Self-reinstall idempotency (manual same-version reinstall → phantom rollback).
  - **I2** — `debug_assert!` is compiled out in release; production regression invisible.
  - **I3** — Configured-key-first precedence always shadows built-in rotation array.
  - **I4** — `install_pending` writer call-site timing + orphan-backup cleanup undefined.
  - **I5** — `published_at: Option<String>` render contract + i18n fallback undefined.
  - **Minors** — test count math, trust-anchor platform specifics, spike output target, test file refs, toast fallback, cliff variable availability.

### Iter 3 — 2026-04-18

- **Starting commit:** `f44b7099`
- **Ending commit:** `0570e023`
- **Reviewer:** fresh code-reviewer (agentId `a172b2dc20614f08c`)
- **Verdict:** **Ready to exit? No.** — 0 Critical + 5 Important + 5 Minor (progress from 2C/5I/7m)
- **Findings:**
  - **I-1** — Telemetry API `telemetry::increment_counter` cited without verification; stub-and-defer contract not stated.
  - **I-2** — `write_install_pending` failure recovery on Windows symmetrically constrained (running executable).
  - **I-3** — `execute_rollback -> Result<(), UpdateError>` contradicts "self-restart" semantics; needs `Infallible` success type.
  - **I-4** — Staleness step missing from §4.4 `check_startup_state_inner` step list.
  - **I-5** — Integration test "exit code is rollback-specific" — no constant defined.
  - M1-M5 — git-cliff fallback, Windows CI-row caveat, toast i18n keys, release.yml separation, Linux manual-trust wording.

### Iter 4 — 2026-04-18

- **Starting commit:** `0570e023`
- **Ending commit:** `811b87e1`
- **Reviewer:** fresh code-reviewer (agentId `a4ca0dce8c6d2e432`)
- **Verdict:** **Ready to exit? No.** — 0 Critical + 1 Important + 3 Minor
- **Findings:**
  - **I-1** — cliff.toml template whitespace-control fidelity: existing template uses trailing `\` on Tera control-flow lines; my amendment in §6.3 dropped them and would produce blank-line drift in rendered output.
  - **M-1** — §4.5 `app_runtime_launch.rs` "and exit" wording could mislead implementer (Infallible success type means function doesn't return; caller only has Err arm).
  - **M-2** — §4.7 test name `rollback_e2e_restores_previous_binary` overstates coverage (body says it skips process replacement); true e2e is in smoke script.
  - **M-3** — §3.3.2 code block shows `telemetry::increment_counter(...)` without comment; prose says stub-and-defer but code reads as real API.

### Iter 5 — 2026-04-18

- **Starting commit:** `811b87e1`
- **Ending commit:** `ff2b4cf6`
- **Reviewer:** fresh code-reviewer (agentId `a90bb5791e463af5f`)
- **Verdict:** **Ready to exit Loop 1 — YES.** — 0 Critical + 0 Important + 2 Minor (optional polish only)
- **Findings:**
  - m1 — `+` blank-line intent in cliff.toml diff (optional clarification note).
  - m2 — `debug_assert!` release-vs-debug dual intent inline comment (optional).
- **Post-verdict polish** (applied after EXIT to preserve zero-issue finish): both m1/m2 clarifications landed inline.

**Loop 1 EXIT — 2026-04-18.** Final spec commit after polish: pending. Total iters: 5 (iter 1 initial + 4 revisions). Total findings addressed: 5 Critical + 17 Important + 24 Minor.

---

## Loop 2 — Plan Deep Review

### Iter 1 — 2026-04-18 (in progress)

Start commit: post-Loop-1-polish (pending).
Plan location: `docs/reviews/2026-04-18-phase4-updater-hardening-plan.md`.

Plan structure follows Phase 5-D8 precedent:
- Task 0: audit baseline + verify telemetry API surface + cliff.toml dry-run.
- Tasks 1-N: break spec sections into shippable units.
- Each task: step-by-step + commit message + cargo verify commands + push cadence.
- Bug-discovery policy: ≤20 LOC in-PR fixes allowed; >20 LOC → defer to follow-up.
- Explicit SKIP markers for any steps that turn out redundant after Task 0 audit.

---

## Loop 2 — Plan Deep Review

_Not yet started. Begins after Loop 1 EXIT._

---

## Loop 3 — Impl Deep Review

_Not yet started. Begins after Loop 2 EXIT._
