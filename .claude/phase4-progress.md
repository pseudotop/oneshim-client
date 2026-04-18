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

### Iter 2 — 2026-04-18 (in progress)

**Revision plan**:
- Drop v0.5.0 breaking-change framing → target v0.4.40-rc.1 (feature addition)
- Drop config migration section (no-op against current `true` default)
- Reframe D9 as "multi-key array enhancement" only
- Drop new `released_at` field → reuse `published_at`, fix UI to surface it
- Add `backup_path` to `.install_pending_{VERSION}` state schema
- Add `.rollback.{ts}` cleanup logic after successful self_healthy write
- Add probe-I/O-error-non-fatal contract
- Add Windows rollback spike day + AppData-only install location assumption
- Make `healthy_threshold` injectable; fix test strategy
- Add installation_id spawn-order guarantee + test
- Add cliff.toml template stub + release.yml header commands
- Split §7.3 key-rotation into scheduled vs compromise branches
- Fix 7 minors

---

## Loop 2 — Plan Deep Review

_Not yet started. Begins after Loop 1 EXIT._

---

## Loop 3 — Impl Deep Review

_Not yet started. Begins after Loop 2 EXIT._
