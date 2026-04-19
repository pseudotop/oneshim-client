# Wave 1 — Phase 4 Deferred Follow-ups Design

**Date:** 2026-04-19
**Scope:** 3 small, independent Phase 4 follow-ups shipped as separate PRs.
**Items:** (1) Notarize workflow `head_branch` condition fix, (2) `signature_public_key` default cleanup (holistic I-4), (3) `boot_count` concurrent-process race mitigation.
**Target version:** v0.4.40-rc.1 (non-breaking; cosmetic/hardening changes).
**Predecessor:** main `757e3a80` (after PR #440–#446).

---

## 1. Goals & Scope

### 1.1 Goals

Close three Phase 4 deferred follow-ups tracked in `project_phase4_complete.md` that are genuinely "quick wins" — each is scoped small, independent, and mechanically verifiable.

1. **Notarize auto-trigger for dispatched releases** — current `if:` gate on `notarize-macos-release-assets.yml` only triggers when `workflow_run.head_branch` starts with `v`. That's true for tag-push releases but false for `workflow_dispatch`-triggered releases (head_branch = `main`), so dispatched releases never auto-notarize.
2. **Remove hardcoded default that shadows TRUSTED_PUBLIC_KEYS** — D9 promoted `TRUSTED_PUBLIC_KEYS` (built-in array in `trusted_keys.rs`) as authoritative trust. But `default_update_signature_public_key()` still returns a hardcoded Ed25519 key identical to `TRUSTED_PUBLIC_KEYS[0]`. After key rotation updates `trusted_keys.rs`, pre-rotation configs keep the old default and trigger false "user-configured key override" warnings during incident response.
3. **Eliminate boot_count read-modify-write race** — current `.boot_count_{VERSION}` is a single file; two instances booting concurrently can interleave read+increment+write and undercount failed boots, defeating the auto-rollback threshold.

### 1.2 Scope boundary & non-goals

**Items in scope** (this design):
- Workflow: `.github/workflows/notarize-macos-release-assets.yml` (if-condition + tag resolution).
- Config: `crates/oneshim-core/src/config/sections/storage.rs` (default helper + tests).
- Updater: `src-tauri/src/updater/health_probe.rs` (per-PID boot marker files + aggregate count + cleanup).

**Out of scope (explicitly):**
- **D13 gRPC server exposure** — moved to Phase 4 remainder alongside C5/D7 per user decision; ~1 week implementation.
- **Windows rollback implementation** — Phase 4 deferred #1, requires Windows CI runner; separate follow-up.
- **M-3 simulator deletion** — blocked on v0.4.40 stable promotion.
- Any changes to `TRUSTED_PUBLIC_KEYS` content, rotation procedure, or key format.
- Any refactor of `check_startup_state` state machine semantics — the 5-step flow (§4.4 of Phase 4 spec) is preserved; only the single-file `.boot_count_{VERSION}` shape changes.

### 1.3 Non-goals rationale

- **Notarize fix keeps `workflow_dispatch` path functional** — not addressing the separate concern that `gh run view --json` needs a permission scope; spike will confirm at implementation time whether `actions: read` suffices (already granted per line 20).
- **No config migration for signature_public_key** — existing configs with the old hardcoded key still validate (non-empty overrides pass through verify, and verify walks `TRUSTED_PUBLIC_KEYS` first per D9 inversion). The change only affects freshly-written default configs on new installs.

---

## 2. Item #1 — Notarize `head_branch` condition fix

### 2.1 Current state

`.github/workflows/notarize-macos-release-assets.yml` lines 24-28:

```yaml
if: |
  github.event_name == 'workflow_dispatch' ||
  (github.event_name == 'workflow_run' &&
   github.event.workflow_run.conclusion == 'success' &&
   startsWith(github.event.workflow_run.head_branch, 'v'))
```

Lines 49-56 (tag resolution inside `Resolve release tag and source run id` step):

```bash
if [[ "$EVENT_NAME" == "workflow_run" ]]; then
  RELEASE_TAG="$WORKFLOW_HEAD_BRANCH"
  SOURCE_RUN_ID="$WORKFLOW_RUN_ID"
else
  RELEASE_TAG="${DISPATCH_TAG:-}"
  SOURCE_RUN_ID="${DISPATCH_SOURCE_RUN_ID:-}"
fi
```

**Two defects, both need fixing (Level B from brainstorm):**

1. **Gate**: `startsWith(head_branch, 'v')` drops `workflow_dispatch`-triggered parents (where `head_branch = main`).
2. **Tag resolution**: even if the gate passes for a dispatched parent, `RELEASE_TAG=$WORKFLOW_HEAD_BRANCH` would evaluate to `main`, which then fails the `$RELEASE_TAG != v*` validation at line 58-61.

### 2.2 Approach

**Gate change**: accept `workflow_dispatch`-originated parents by extending the compound condition.

```yaml
if: |
  github.event_name == 'workflow_dispatch' ||
  (github.event_name == 'workflow_run' &&
   github.event.workflow_run.conclusion == 'success' &&
   (startsWith(github.event.workflow_run.head_branch, 'v') ||
    github.event.workflow_run.event == 'workflow_dispatch'))
```

**Tag resolution change**: when parent was dispatched, look up the `tag_name` input via GitHub CLI. The notarize workflow already has `actions: read` permission (line 20).

```bash
if [[ "$EVENT_NAME" == "workflow_run" ]]; then
  if [[ "$WORKFLOW_PARENT_EVENT" == "workflow_dispatch" ]]; then
    # Parent release.yml was manually dispatched; head_branch is the
    # dispatching branch (typically main), not the tag. Extract the tag
    # from the parent workflow's dispatch inputs.
    RELEASE_TAG="$(gh run view "$WORKFLOW_RUN_ID" \
      --repo "$GITHUB_REPOSITORY" \
      --json displayTitle,event \
      --jq '.displayTitle')"
    # Fallback: displayTitle isn't stable input-echo. If displayTitle
    # doesn't match v*, try the workflow's inputs via API.
    if [[ "$RELEASE_TAG" != v* ]]; then
      RELEASE_TAG="$(gh api "repos/$GITHUB_REPOSITORY/actions/runs/$WORKFLOW_RUN_ID" \
        --jq '.display_title // empty' | grep -oE 'v[0-9][^[:space:]]*' | head -n1)"
    fi
  else
    RELEASE_TAG="$WORKFLOW_HEAD_BRANCH"
  fi
  SOURCE_RUN_ID="$WORKFLOW_RUN_ID"
else
  RELEASE_TAG="${DISPATCH_TAG:-}"
  SOURCE_RUN_ID="${DISPATCH_SOURCE_RUN_ID:-}"
fi
```

New env pass-through required: `WORKFLOW_PARENT_EVENT: ${{ github.event.workflow_run.event }}`.

### 2.3 Spike question (resolve at implementation time)

`gh run view --json displayTitle` — for a tag-push parent, `displayTitle` is typically the commit message's first line, not the tag. For a dispatched parent, GitHub sets displayTitle to the dispatched input tag name. If empirical validation shows displayTitle unreliable for dispatched case, fall back to an explicit `gh api` call for the run's input payload — documented as the second branch above.

**Decision criterion**: before landing, invoke the workflow once via workflow_dispatch on a test tag (or reuse a prior dispatched run ID) and confirm extraction logic returns the expected tag.

### 2.4 Tests

**Manual verification only** (no unit test for shell logic). Test plan:

1. After merge, dispatch release.yml with a known test tag (e.g., via `gh workflow run release.yml -f tag_name=v0.4.40-rc.test`).
2. Observe notarize workflow_run auto-triggers and `Resolve release tag` step outputs `RELEASE_TAG=v0.4.40-rc.test`.
3. Cancel the notarize run (we don't actually want to notarize a test tag).

If dispatch-based test isn't feasible pre-merge, land with only the `if:` gate fix (conservative) and defer the tag-resolution change to a separate PR with live validation. **Recorded caveat: if live test is skipped, this becomes Level A (shallow) and a note must be added to STILL OPEN follow-ups.**

### 2.5 PR metadata

- Branch: `fix/notarize-head-branch-dispatched-parent`
- Commit type: `fix(ci)` (cliff-visible)
- Title: `fix(ci): notarize auto-trigger for workflow_dispatch-originated releases`
- CHANGELOG section: "Fixed" (auto via git-cliff; confirm post-squash).

---

## 3. Item #2 — `signature_public_key` default cleanup (I-4)

### 3.1 Current state

`crates/oneshim-core/src/config/sections/storage.rs:355-357`:

```rust
fn default_update_signature_public_key() -> String {
    "GIdf7Wg4kvvvoT7jR0xwKLKna8hUR1kvowONbHbPz1E=".to_string()
}
```

This base64 string equals `TRUSTED_PUBLIC_KEYS[0]` (confirmed at the D9 landing in PR #439). Three tests in `config/mod.rs` (lines 202-234) pass synthetic keys and are unaffected by the default.

### 3.2 Approach

Change default to empty string:

```rust
fn default_update_signature_public_key() -> String {
    String::new()
}
```

Rationale:
- D9 made `TRUSTED_PUBLIC_KEYS` the authoritative trust source.
- `validate_integrity_policy` (storage.rs:247-272) already accepts empty `signature_public_key` when updates are enabled (the `filter(|k| !k.trim().is_empty())` short-circuit).
- `verify_signature` (install.rs) walks `TRUSTED_PUBLIC_KEYS` first; empty fallback is inert.
- Eliminates the "user override" false positive after future `trusted_keys.rs` edits.

### 3.3 Tests

Add in `config/sections/storage.rs` test module (near the existing `default_update_*` helpers):

1. **`default_update_signature_public_key_is_empty`** — assert the helper returns empty.
2. **`validate_integrity_policy_passes_with_default_config`** — construct `UpdateConfig::default()` and call `validate_integrity_policy()`; assert `Ok(())`. (Guards against regression where someone re-adds a hardcoded default that conflicts with validation.)

Existing tests in `config/mod.rs:202-234` (three test cases with synthetic keys `[1u8; 16]`, `[7u8; 32]`) continue to pass — they explicitly set `signature_public_key`.

No regression expected in `updater/install.rs` tests either — they rely on `TRUSTED_PUBLIC_KEYS` directly, not on config defaults.

### 3.4 CHANGELOG

Under `## [Unreleased]` → `### Changed`:

> - `update.signature_public_key` default is now empty string; `TRUSTED_PUBLIC_KEYS` built-in array is the sole authoritative trust source by default. Existing configs with a non-empty value continue to function as an override (unchanged semantics).

### 3.5 PR metadata

- Branch: `refactor/signature-public-key-empty-default`
- Commit type: `refactor(updater)` (cliff-visible)
- Title: `refactor(updater): default signature_public_key to empty (I-4)`

---

## 4. Item #3 — `boot_count` concurrent-process race mitigation

### 4.1 Current state

`src-tauri/src/updater/health_probe.rs` maintains a single marker file `.boot_count_{VERSION}` that stores an integer ASCII count. On startup, `check_startup_state_inner` reads it, parses, compares against threshold, increments, writes back. On healthy-writer timeout (default 30s uptime), the file is deleted alongside `.install_pending_{VERSION}`.

**Race**: two instances of the same binary booting within milliseconds of each other each read the count, each increment locally, each write back — losing one or more increments. In extreme concurrent failure cases, the threshold may never be reached and rollback never triggers.

Probability is low in practice (single-user desktop app) but the scenario exists for:
- auto-start-on-login racing with a leftover session's terminating instance
- multi-account user switching on shared machines
- health_probe spec §4.3 (staleness check) intersecting with rapid restarts

### 4.2 Approach

Replace the single `.boot_count_{VERSION}` file with per-PID marker files `.boot_count_pid_{VERSION}_{PID}`.

**Write path** (`check_startup_state_inner` step 5 — increment):
- Each boot creates exactly one empty file: `.boot_count_pid_{VERSION}_{PID}` via `std::fs::OpenOptions::new().write(true).create_new(true).open(path)`. `create_new` is atomic (fails if file exists — acceptable since per-PID path uniqueness means collision is near-zero but we defensively ignore `AlreadyExists`).
- No read-modify-write. No shared mutable state.

**Read path** (step 3 — read count, step 4 — threshold check):
- Count = number of files matching glob `.boot_count_pid_{VERSION}_*` in `binary_dir`.
- Implementation: `std::fs::read_dir(binary_dir)` + filter by filename prefix. No external glob crate needed.

**Cleanup path** (healthy_writer, post-threshold-timeout):
- Delete all files matching `.boot_count_pid_{VERSION}_*` (replaces the single-file delete).
- Existing deletion of `.install_pending_{VERSION}` + rollback backups is unchanged.

**Staleness step 0** (spec §4.3 preserved):
- Staleness rule currently compares `.boot_count_{VERSION}` mtime to current binary mtime. New behavior: use the **oldest** `.boot_count_pid_{VERSION}_*` file mtime as the staleness anchor (fresh install should have no such files; if the oldest is older than the binary, something phantom happened). Falls back to "no pid files = no boot history = fresh/healthy" per §4.3 semantics.

### 4.3 Scope of code change

**Modified file**: `src-tauri/src/updater/health_probe.rs`

**Helper functions introduced/modified**:
- `boot_count_pid_path(version: &str, pid: u32) -> PathBuf` — new.
- `boot_count_files(&self) -> std::io::Result<Vec<PathBuf>>` — new; enumerates pid sub-files via `read_dir`.
- `boot_count(&self) -> std::io::Result<u32>` — replace read-parse with `boot_count_files().len()`.
- `increment_boot_count(&self) -> std::io::Result<()>` — replace with single `create_new` call against per-PID path.
- `cleanup_boot_count_markers(&self) -> std::io::Result<()>` — new; delete all matching pid files (and the legacy single-file if present).
- `legacy_boot_count_path(version: &str) -> PathBuf` — new helper returning `.boot_count_{VERSION}`; used only during migration (§4.3) and legacy cleanup.

**Migration of pre-existing `.boot_count_{VERSION}` files**:
- On startup, if an old single-file `.boot_count_{VERSION}` exists alongside new pid files, delete it silently. Documented at the top of the function. Users upgrading from v0.4.39-rc.1 won't have this file mid-failure because v0.4.39-rc.1 healthy_writer already cleans it within 30s. Edge case only: a v0.4.39 crashloop (old file present) → user manually installs v0.4.40-rc.1 → startup sees legacy file. Migration step handles it.

### 4.4 Tests

Add to the existing `#[cfg(test)] mod tests` in `health_probe.rs`. Existing 7 tests stay (they use the tempfile + injected threshold pattern).

1. **`concurrent_boot_count_no_undercount`** — simulate two concurrent boots by creating two `HealthProbe` instances with different PIDs against the same tempdir, call increment on each, assert `boot_count() == 2`.
2. **`boot_count_cleanup_removes_all_pid_files`** — create 3 pid marker files, call `cleanup_boot_count_markers`, assert directory has none.
3. **`legacy_single_file_boot_count_migrated`** — manually create `.boot_count_{VERSION}` with content "2", instantiate `HealthProbe`, run `check_startup_state`, assert old file is gone. (Does not assert count == 2 — migration drops the legacy count; acceptable because the next failed boot will create its own pid file and eventually reach threshold again.)
4. **`boot_count_pid_file_is_stable_across_probe_reads`** — create 1 pid file, call `boot_count()` twice, assert 1 both times (read is non-destructive).
5. **Update existing `boot_count_staleness_triggers_phantom_cleanup`** — use oldest pid file mtime for staleness check; test setup creates a pid file with artificially old mtime.

Total delta: ~5 new tests, 1 modified. Net count change logged in PR.

### 4.5 CHANGELOG

Under `## [Unreleased]` → `### Changed`:

> - Updater boot-count tracking now uses per-PID marker files (`.boot_count_pid_{VERSION}_{PID}`) instead of a single `.boot_count_{VERSION}` file, eliminating the read-modify-write race when multiple instances start concurrently. Behavior is identical for single-instance users; threshold and rollback semantics unchanged.

### 4.6 PR metadata

- Branch: `fix/updater-boot-count-per-pid-markers`
- Commit type: `fix(updater)` (cliff-visible)
- Title: `fix(updater): per-PID boot_count markers eliminate concurrent-boot race`

---

## 5. Execution Order & Dependencies

Independent items — order chosen for lowest risk → highest risk (tests breadth):

1. **#1 Notarize head_branch** — workflow YAML only. No Rust compilation needed. Manual dispatch validation post-merge.
2. **#2 signature_public_key default** — pure Rust config change + 2 tests. Fast CI, no cross-crate impact.
3. **#3 boot_count per-PID** — updater module change + ~5 new tests + 1 modified. Most code surface; requires careful handling of the legacy-file migration edge case.

No blocking dependencies between the three. Serial landing simplifies CI and merge-queue coordination.

---

## 6. Risks & Mitigations

| Risk | Likelihood | Mitigation |
|------|------------|-----------|
| Notarize `gh run view` displayTitle unreliable for dispatched parents | Medium | Fallback to `gh api` with regex tag extraction (§2.2). If both fail, drop to Level A (gate-only fix) and flag as deferred. |
| Empty default signature_public_key breaks some edge test or downstream tool | Low | 1,110+ existing tests run in CI. Manual `grep -rn 'signature_public_key'` before merge to audit. |
| Per-PID boot_count migration leaves orphan legacy files | Low | Explicit migration step in `check_startup_state` deletes legacy on first run; tested. |
| PID reuse across boots (PID X terminates, PID X reboots before cleanup) | Very Low | File deletion is idempotent; `create_new` on reboot would fail with AlreadyExists and we defensively accept that path (treat as "already recorded this boot"). Net result: undercount by 1 in extreme PID-reuse edge case — strictly better than current unbounded race. |
| All 3 PRs colliding at the PR merge queue | Low | Serial merging; each PR ~15 min CI. |

---

## 7. Acceptance Criteria

- All 3 PRs merged to `main`.
- No regression in `cargo test --workspace` (+5 net new tests across items #2 and #3; #1 is workflow-only).
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `cargo fmt --check` clean.
- CHANGELOG `[Unreleased]` reflects all 3 entries.
- Manual dispatch test of notarize workflow confirms auto-trigger (or documented as Level A with reason).
- `project_phase4_complete.md` deferred follow-up list updated to reflect shipped items.
- `MEMORY.md` refreshed to drop Wave 1 items from the "remaining" section.

---

## 8. Deferred / Out of Scope (restated)

Reminder for memory reload next session:
- **D13 gRPC server exposure** — moved to Phase 4 remainder (1 week, alongside C5/D7).
- **Windows rollback implementation** — Phase 4 deferred #1; requires Windows CI runner.
- **M-3 simulator deletion** — blocked on v0.4.40 stable promotion.
- **Notarize Level A fallback** — only if live dispatch test fails.

---

## 9. Invoke writing-plans next

After user review approves this design, invoke `superpowers:writing-plans` to produce a per-item implementation plan (one plan doc, three sections). Plans will enumerate exact file edits, test stubs, commit messages, and verification commands.
