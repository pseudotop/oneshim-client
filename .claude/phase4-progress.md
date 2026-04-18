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

### Iter 4 — 2026-04-18 (in progress)

**Revision plan**:
- I-1: Add explicit "if telemetry API not yet public, stub with `tracing::error!` only; verify Phase 2 surface in Task 0 of plan" note.
- I-2: Add Windows constraint note to `write_install_pending` restoration path.
- I-3: Change signature to `Result<Infallible, UpdateError>`; explicit "success path terminates" contract; add `ROLLBACK_EXIT_CODE = 75`; caller pattern for `app_runtime_launch.rs`.
- I-4: Insert staleness step 0 into `check_startup_state_inner` docstring; update ASCII state machine to step-number ordering; reorder steps 3-5 so increment happens AFTER threshold check.
- I-5: Rewrite integration test assertions: binary bytes match pre-rollback backup, pending file read before swap, event broadcast; no exit-code assertion (harness can't observe cross-process replacement).
- Minors: git-cliff version fallback, Windows CI-row caveat for MoveFileEx path, 2 new toast i18n keys, Linux "manually trust" wording.

**Commit plan**: targeted Edits + commit + iter 4 reviewer.

---

## Loop 2 — Plan Deep Review

_Not yet started. Begins after Loop 1 EXIT._

---

## Loop 3 — Impl Deep Review

_Not yet started. Begins after Loop 2 EXIT._
