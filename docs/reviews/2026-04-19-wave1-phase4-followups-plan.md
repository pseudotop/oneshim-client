# Wave 1 — Phase 4 Deferred Follow-ups Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship three small Phase 4 deferred follow-ups as separate PRs — notarize workflow dispatch-compatibility fix, `signature_public_key` default cleanup (I-4), and `boot_count` concurrent-process race mitigation.

**Architecture:** Three independent PRs in sequence. PR #1 (notarize) includes this plan + the design doc. PR #2 and #3 reference the merged design doc via URL. Each PR follows TDD where applicable (items #2 and #3; item #1 is workflow YAML with manual verification only).

**Tech Stack:** GitHub Actions YAML, Rust 2021 (oneshim-core config + src-tauri updater), tokio/tempfile for tests.

---

## File Structure

### Modified

- `.github/workflows/notarize-macos-release-assets.yml` — `if:` condition + env var + tag-resolution bash (PR #1).
- `crates/oneshim-core/src/config/sections/storage.rs` — single-line default helper + 2 new tests (PR #2).
- `src-tauri/src/updater/health_probe.rs` — per-PID helpers, increment/read rewrite, healthy-writer cleanup, foreign-version sweep logic, module-doc update, 2 existing tests modified, 3 new tests (PR #3).
- `CHANGELOG.md` — `[Unreleased]` section gains 3 entries (one per PR, landed with each PR).

### Created

- `docs/reviews/2026-04-19-wave1-phase4-followups-design.md` — **already exists** (uncommitted); committed in PR #1.
- `docs/reviews/2026-04-19-wave1-phase4-followups-plan.md` — **this file**; committed in PR #1.

---

## Execution Environment

Starting state at PR #1 creation:
- Working directory: `.claude/worktrees/features` (worktree)
- `git status`: detached HEAD on `origin/main` at `757e3a80`
- Untracked: `docs/reviews/2026-04-19-wave1-phase4-followups-design.md` + `docs/reviews/2026-04-19-wave1-phase4-followups-plan.md` (once this file is saved)

---

# Phase A — PR #1: Notarize `head_branch` dispatched-parent fix

**Branch:** `fix/notarize-head-branch-dispatched-parent`
**Scope:** workflow YAML + design/plan doc commits
**TDD applicable:** no (workflow YAML). Manual dispatch validation post-merge.

---

### Task 1: Create PR #1 branch

**Files:** (none modified in this task — branch creation only)

- [ ] **Step 1: Verify current state**

Run: `git status -sb && git rev-parse HEAD`
Expected: `## HEAD (no branch)` + current HEAD ≈ `757e3a80`, untracked `docs/reviews/2026-04-19-wave1-phase4-followups-{design,plan}.md` only.

- [ ] **Step 2: Create branch from current HEAD**

Run: `git checkout -b fix/notarize-head-branch-dispatched-parent`
Expected: `Switched to a new branch 'fix/notarize-head-branch-dispatched-parent'`

---

### Task 2: Commit design + plan docs

**Files:**
- Modify (track): `docs/reviews/2026-04-19-wave1-phase4-followups-design.md`
- Modify (track): `docs/reviews/2026-04-19-wave1-phase4-followups-plan.md`

- [ ] **Step 1: Stage the two doc files**

Run: `git add docs/reviews/2026-04-19-wave1-phase4-followups-design.md docs/reviews/2026-04-19-wave1-phase4-followups-plan.md`

- [ ] **Step 2: Verify only those two files are staged**

Run: `git diff --cached --name-only`
Expected output (exact):
```
docs/reviews/2026-04-19-wave1-phase4-followups-design.md
docs/reviews/2026-04-19-wave1-phase4-followups-plan.md
```

- [ ] **Step 3: Commit the docs**

Run:
```bash
git commit -m "$(cat <<'EOF'
docs(reviews): Wave 1 — Phase 4 deferred follow-ups design + plan

Covers three items:
- Notarize workflow head_branch dispatched-parent fix (PR #1)
- signature_public_key default cleanup I-4 (PR #2)
- boot_count per-PID markers race mitigation (PR #3)

Each item ships as a separate PR. Design + plan land with PR #1 and
are referenced by PR #2 and #3.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```
Expected: `[fix/notarize-head-branch-dispatched-parent <sha>] docs(reviews): Wave 1 — Phase 4 deferred follow-ups design + plan` + `2 files changed`.

---

### Task 3: Fix notarize workflow `if:` condition

**Files:**
- Modify: `.github/workflows/notarize-macos-release-assets.yml:24-28`

- [ ] **Step 1: Inspect current `if:` block**

Run the Read tool on `.github/workflows/notarize-macos-release-assets.yml` lines 22-32 to confirm the current state:

```yaml
jobs:
  notarize-macos-assets:
    if: |
      github.event_name == 'workflow_dispatch' ||
      (github.event_name == 'workflow_run' &&
       github.event.workflow_run.conclusion == 'success' &&
       startsWith(github.event.workflow_run.head_branch, 'v'))
    runs-on: macos-latest
    timeout-minutes: 180
    env:
```

- [ ] **Step 2: Apply the condition fix**

Use the Edit tool on `.github/workflows/notarize-macos-release-assets.yml`:

`old_string`:
```
    if: |
      github.event_name == 'workflow_dispatch' ||
      (github.event_name == 'workflow_run' &&
       github.event.workflow_run.conclusion == 'success' &&
       startsWith(github.event.workflow_run.head_branch, 'v'))
```

`new_string`:
```
    if: |
      github.event_name == 'workflow_dispatch' ||
      (github.event_name == 'workflow_run' &&
       github.event.workflow_run.conclusion == 'success' &&
       (startsWith(github.event.workflow_run.head_branch, 'v') ||
        github.event.workflow_run.event == 'workflow_dispatch'))
```

- [ ] **Step 3: Validate YAML syntax**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/notarize-macos-release-assets.yml'))" && echo OK`
Expected: `OK`

If `python3` / `yaml` unavailable, alternatively run `yq '.' .github/workflows/notarize-macos-release-assets.yml >/dev/null && echo OK`.

---

### Task 4: Fix notarize tag resolution for dispatched parents

**Files:**
- Modify: `.github/workflows/notarize-macos-release-assets.yml:42-71`

- [ ] **Step 1: Add `WORKFLOW_PARENT_EVENT` env var**

Use the Edit tool on `.github/workflows/notarize-macos-release-assets.yml`:

`old_string`:
```
        env:
          EVENT_NAME: ${{ github.event_name }}
          DISPATCH_TAG: ${{ inputs.tag }}
          DISPATCH_SOURCE_RUN_ID: ${{ inputs.source_run_id }}
          WORKFLOW_HEAD_BRANCH: ${{ github.event.workflow_run.head_branch }}
          WORKFLOW_RUN_ID: ${{ github.event.workflow_run.id }}
```

`new_string`:
```
        env:
          EVENT_NAME: ${{ github.event_name }}
          DISPATCH_TAG: ${{ inputs.tag }}
          DISPATCH_SOURCE_RUN_ID: ${{ inputs.source_run_id }}
          WORKFLOW_HEAD_BRANCH: ${{ github.event.workflow_run.head_branch }}
          WORKFLOW_PARENT_EVENT: ${{ github.event.workflow_run.event }}
          WORKFLOW_RUN_ID: ${{ github.event.workflow_run.id }}
```

- [ ] **Step 2: Update bash tag-resolution block**

Use the Edit tool on the same file:

`old_string`:
```
          if [[ "$EVENT_NAME" == "workflow_run" ]]; then
            RELEASE_TAG="$WORKFLOW_HEAD_BRANCH"
            SOURCE_RUN_ID="$WORKFLOW_RUN_ID"
          else
            RELEASE_TAG="${DISPATCH_TAG:-}"
            SOURCE_RUN_ID="${DISPATCH_SOURCE_RUN_ID:-}"
          fi
```

`new_string`:
```
          if [[ "$EVENT_NAME" == "workflow_run" ]]; then
            if [[ "$WORKFLOW_PARENT_EVENT" == "workflow_dispatch" ]]; then
              # Parent release.yml was manually dispatched; head_branch is the
              # dispatching branch (typically main), not the tag. Extract the
              # tag from the parent workflow run's display title.
              RELEASE_TAG="$(gh run view "$WORKFLOW_RUN_ID" \
                --repo "$GITHUB_REPOSITORY" \
                --json displayTitle \
                --jq '.displayTitle // empty')"
              # Fallback: if displayTitle doesn't parse as a version tag,
              # scan for a v* pattern in the display title string.
              if [[ "$RELEASE_TAG" != v* ]]; then
                RELEASE_TAG="$(gh api "repos/$GITHUB_REPOSITORY/actions/runs/$WORKFLOW_RUN_ID" \
                  --jq '.display_title // empty' \
                  | grep -oE 'v[0-9][^[:space:]]*' \
                  | head -n1)"
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

- [ ] **Step 3: Validate YAML syntax again**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/notarize-macos-release-assets.yml'))" && echo OK`
Expected: `OK`

- [ ] **Step 4: Confirm no other changes leaked into the file**

Run: `git diff .github/workflows/notarize-macos-release-assets.yml | head -60`
Expected: only the `if:` + env + bash block diffs above; no whitespace or other line changes.

---

### Task 5: Update CHANGELOG for PR #1

**Files:**
- Modify: `CHANGELOG.md` — `[Unreleased]` section

- [ ] **Step 1: Find the `### Infrastructure` subsection of `[Unreleased]`**

Run: `grep -n "^### " CHANGELOG.md | head -10`
Expected output starts with: `10:### Added`, `20:### Changed`, `24:### Infrastructure`, `32:### Added` (0.4.39-rc.1 block starts around line 32).

- [ ] **Step 2: Insert a new `### Fixed` subsection under `[Unreleased]` ABOVE `### Infrastructure`**

Use the Edit tool on `CHANGELOG.md`:

`old_string`:
```
### Changed

- `validate_integrity_policy` no longer requires `signature_public_key` to be non-empty; built-in `TRUSTED_PUBLIC_KEYS` array is authoritative. Format validation still applies when a non-empty override is provided.

### Infrastructure
```

`new_string`:
```
### Changed

- `validate_integrity_policy` no longer requires `signature_public_key` to be non-empty; built-in `TRUSTED_PUBLIC_KEYS` array is authoritative. Format validation still applies when a non-empty override is provided.

### Fixed

- `notarize-macos-release-assets` workflow now auto-triggers for `workflow_dispatch`-originated parent release runs. Previous `startsWith(head_branch, 'v')` gate filtered dispatched parents (where `head_branch == main`) out of the notarize path; tag resolution now uses the parent run's `displayTitle` via `gh run view` with a regex fallback against the full display-title payload.

### Infrastructure
```

- [ ] **Step 3: Verify the edit**

Run: `sed -n '19,33p' CHANGELOG.md`
Expected: shows `### Changed` → `- validate_integrity_policy...` → blank → `### Fixed` → new bullet → blank → `### Infrastructure`.

---

### Task 6: Commit PR #1 workflow + CHANGELOG changes

**Files:**
- `.github/workflows/notarize-macos-release-assets.yml`
- `CHANGELOG.md`

- [ ] **Step 1: Stage both files**

Run: `git add .github/workflows/notarize-macos-release-assets.yml CHANGELOG.md`

- [ ] **Step 2: Verify staged diff is clean**

Run: `git diff --cached --stat`
Expected: `.github/workflows/notarize-macos-release-assets.yml | ~20 changes`, `CHANGELOG.md | ~4 insertions`.

- [ ] **Step 3: Commit**

Run:
```bash
git commit -m "$(cat <<'EOF'
fix(ci): notarize auto-trigger for workflow_dispatch-originated releases

The notarize-macos-release-assets workflow's if-condition filtered out
parent release runs that were triggered via workflow_dispatch (where
workflow_run.head_branch is the dispatching branch, typically `main`,
not the tag). Add `workflow_run.event == 'workflow_dispatch'` as an
alternative branch and resolve the release tag from the parent run's
displayTitle via `gh run view` when that branch matches.

Closes Phase 4 deferred follow-up (memory: project_phase4_complete.md).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: Push PR #1 branch and open pull request

**Files:** (none — git operations only)

- [ ] **Step 1: Push with upstream tracking**

Run: `git push -u origin fix/notarize-head-branch-dispatched-parent`
Expected: push succeeds; new branch registered.

- [ ] **Step 2: Open PR via gh**

Run:
```bash
gh pr create --title "fix(ci): notarize auto-trigger for workflow_dispatch-originated releases" --body "$(cat <<'EOF'
## Summary

- **Bug:** `notarize-macos-release-assets.yml` auto-triggers via `workflow_run` on `Release` workflow completion, but the `if:` gate requires `startsWith(workflow_run.head_branch, 'v')`. For `workflow_dispatch`-triggered parent releases, `head_branch` is the dispatching branch (e.g. `main`), not the tag — so dispatched releases never auto-notarize.
- **Fix:** extend the `if:` gate to also accept `workflow_run.event == 'workflow_dispatch'`, and when that branch matches, resolve the release tag from the parent run's `displayTitle` via `gh run view` (with a regex fallback against the full `display_title` payload).
- **Other follow-ups in this wave:** Ships design + plan doc for all three Wave 1 follow-ups (signature_public_key default I-4 → PR #2, boot_count per-PID markers → PR #3). Those are tracked separately.

## Test plan

- [ ] CI `actionlint` / YAML workflow syntax validation green.
- [ ] After merge, dispatch `release.yml` manually with a test tag and confirm `notarize-macos-release-assets` auto-triggers and `Resolve release tag` step logs the expected `RELEASE_TAG`. (Cancel the notarize run before it consumes notary-service quota.)
- [ ] Tag-push path (existing behavior) remains functional — observable on next regular RC push.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 3: Record the PR URL**

The PR URL returned by `gh pr create` — record it for session hand-off. Expected form: `https://github.com/pseudotop/oneshim-client/pull/<N>`.

---

### Task 8: Wait for CI, address any failures, merge PR #1

**Files:** (none — CI/merge operations)

- [ ] **Step 1: Check CI status**

Run: `gh pr checks --watch` (blocks until CI completes).
Expected: all required checks pass. If a check fails, diagnose and fix; do NOT proceed until green.

- [ ] **Step 2: Merge (squash)**

Run: `gh pr merge --squash --delete-branch`
Expected: PR merged, branch deleted remote-side.

- [ ] **Step 3: Sync local main pointer**

Run:
```bash
git fetch origin main
git log --oneline origin/main -1
```
Expected: new commit on `origin/main` with title `fix(ci): notarize auto-trigger for workflow_dispatch-originated releases (#<N>)`.

---

# Phase B — PR #2: `signature_public_key` default cleanup (I-4)

**Branch:** `refactor/signature-public-key-empty-default`
**Scope:** `crates/oneshim-core/src/config/sections/storage.rs` one-line default + 2 tests
**TDD applicable:** yes — write failing test first.

---

### Task 9: Create PR #2 branch from updated main

**Files:** (none)

- [ ] **Step 1: Checkout updated main**

Run:
```bash
git fetch origin main
git checkout origin/main
```
Expected: detached HEAD on the new `origin/main` including PR #1.

- [ ] **Step 2: Create branch**

Run: `git checkout -b refactor/signature-public-key-empty-default`

---

### Task 10: Write failing test — default helper returns empty

**Files:**
- Modify: `crates/oneshim-core/src/config/sections/storage.rs` — add test inside the test module.

- [ ] **Step 1: Check whether a test module already exists in storage.rs**

Run: `grep -n "^#\[cfg(test)\]" crates/oneshim-core/src/config/sections/storage.rs`
Expected: either no match (no test module yet) or the line number of the existing module.

- [ ] **Step 2a (if no test module exists): Append a fresh test module at the end of the file**

Use the Edit tool on `crates/oneshim-core/src/config/sections/storage.rs`:

`old_string`: (end of file — find the last 2-3 lines and copy them verbatim; typically the final `fn default_update_signature_public_key()` block closing `}`)

```
fn default_update_signature_public_key() -> String {
    "GIdf7Wg4kvvvoT7jR0xwKLKna8hUR1kvowONbHbPz1E=".to_string()
}
```

`new_string`:
```
fn default_update_signature_public_key() -> String {
    "GIdf7Wg4kvvvoT7jR0xwKLKna8hUR1kvowONbHbPz1E=".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_update_signature_public_key_is_empty() {
        assert_eq!(default_update_signature_public_key(), "");
    }
}
```

- [ ] **Step 2b (if a test module exists): Insert the test into it**

Use the Edit tool. Locate the existing `mod tests { use super::*; ...` block and append the new test inside it, just before the module's closing `}`.

- [ ] **Step 3: Run the test — expect FAIL**

Run: `cargo test -p oneshim-core --lib config::sections::storage::tests::default_update_signature_public_key_is_empty -- --nocapture 2>&1 | tail -20`
Expected: test FAILS with `assertion `left == right` failed` because the helper currently returns `"GIdf7Wg4..."`.

---

### Task 11: Change default helper to return empty string

**Files:**
- Modify: `crates/oneshim-core/src/config/sections/storage.rs:355-357`

- [ ] **Step 1: Apply the one-line change**

Use the Edit tool:

`old_string`:
```
fn default_update_signature_public_key() -> String {
    "GIdf7Wg4kvvvoT7jR0xwKLKna8hUR1kvowONbHbPz1E=".to_string()
}
```

`new_string`:
```
fn default_update_signature_public_key() -> String {
    // D9 (Phase 4): TRUSTED_PUBLIC_KEYS (src-tauri/src/updater/trusted_keys.rs)
    // is the authoritative trust source. This field is now an optional user
    // override (e.g. dev self-signing); default empty means "no override".
    String::new()
}
```

- [ ] **Step 2: Run the test — expect PASS**

Run: `cargo test -p oneshim-core --lib config::sections::storage::tests::default_update_signature_public_key_is_empty -- --nocapture 2>&1 | tail -20`
Expected: test PASSES.

---

### Task 12: Add guard test — default config passes validate_integrity_policy

**Files:**
- Modify: `crates/oneshim-core/src/config/sections/storage.rs` — second test in the same module.

- [ ] **Step 1: Append the guard test to the test module**

Use the Edit tool:

`old_string`:
```
    #[test]
    fn default_update_signature_public_key_is_empty() {
        assert_eq!(default_update_signature_public_key(), "");
    }
}
```

`new_string`:
```
    #[test]
    fn default_update_signature_public_key_is_empty() {
        assert_eq!(default_update_signature_public_key(), "");
    }

    #[test]
    fn validate_integrity_policy_passes_with_default_config() {
        // Guard: prevents regressing to a hardcoded default that might
        // conflict with validation (e.g., a future non-base64 placeholder).
        let config = UpdateConfig::default();
        assert!(
            config.validate_integrity_policy().is_ok(),
            "default UpdateConfig must validate: {:?}",
            config.validate_integrity_policy()
        );
    }
}
```

- [ ] **Step 2: Run both tests**

Run: `cargo test -p oneshim-core --lib config::sections::storage::tests -- --nocapture 2>&1 | tail -20`
Expected: both tests PASS.

- [ ] **Step 3: Run full config tests to ensure no regression in the 3 existing integrity tests (config/mod.rs:202-234)**

Run: `cargo test -p oneshim-core --lib config -- --nocapture 2>&1 | tail -30`
Expected: all config tests pass, including the 3 synthetic-key tests.

---

### Task 13: Clippy + fmt sweep for PR #2

**Files:** (no direct file changes — verification only)

- [ ] **Step 1: Run clippy on the oneshim-core crate**

Run: `cargo clippy -p oneshim-core --all-targets -- -D warnings 2>&1 | tail -20`
Expected: clean (no warnings). If clippy emits a warning about the new test or the comment, address it inline.

- [ ] **Step 2: Run fmt check**

Run: `cargo fmt --check -p oneshim-core 2>&1 | tail -20`
Expected: no output (fmt clean). If fmt emits diffs, run `cargo fmt -p oneshim-core` and restage.

---

### Task 14: Update CHANGELOG for PR #2

**Files:**
- Modify: `CHANGELOG.md` — `[Unreleased]` → `### Changed`

- [ ] **Step 1: Append the new bullet under `### Changed` (same subsection that already exists for `validate_integrity_policy`)**

Use the Edit tool:

`old_string`:
```
### Changed

- `validate_integrity_policy` no longer requires `signature_public_key` to be non-empty; built-in `TRUSTED_PUBLIC_KEYS` array is authoritative. Format validation still applies when a non-empty override is provided.
```

`new_string`:
```
### Changed

- `validate_integrity_policy` no longer requires `signature_public_key` to be non-empty; built-in `TRUSTED_PUBLIC_KEYS` array is authoritative. Format validation still applies when a non-empty override is provided.
- `update.signature_public_key` default is now empty string (was a hardcoded copy of `TRUSTED_PUBLIC_KEYS[0]`); `TRUSTED_PUBLIC_KEYS` is the sole authoritative trust source by default. Existing configs with a non-empty value continue to function as an override (unchanged semantics). Closes holistic review I-4.
```

- [ ] **Step 2: Verify**

Run: `sed -n '20,27p' CHANGELOG.md`
Expected: shows both bullets under `### Changed`.

---

### Task 15: Commit + push + open PR #2

**Files:**
- `crates/oneshim-core/src/config/sections/storage.rs`
- `CHANGELOG.md`

- [ ] **Step 1: Stage**

Run: `git add crates/oneshim-core/src/config/sections/storage.rs CHANGELOG.md`

- [ ] **Step 2: Commit**

Run:
```bash
git commit -m "$(cat <<'EOF'
refactor(updater): default signature_public_key to empty (I-4)

D9 made TRUSTED_PUBLIC_KEYS (src-tauri/src/updater/trusted_keys.rs)
the authoritative trust source. Keeping the per-config default at a
hardcoded copy of TRUSTED_PUBLIC_KEYS[0] means that after a future
key rotation, old configs would still carry the retired key and
trigger false "user-configured override" warnings during incidents.

Change the default to empty; validate_integrity_policy already accepts
empty values (D9 relaxation). Adds a guard test that
UpdateConfig::default() validates, preventing future regressions.

Closes Phase 4 holistic review I-4 (memory: project_phase4_complete.md).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 3: Push**

Run: `git push -u origin refactor/signature-public-key-empty-default`

- [ ] **Step 4: Open PR**

Run:
```bash
gh pr create --title "refactor(updater): default signature_public_key to empty (I-4)" --body "$(cat <<'EOF'
## Summary

- **Refactor:** `default_update_signature_public_key()` now returns `String::new()` instead of a hardcoded copy of `TRUSTED_PUBLIC_KEYS[0]`. D9 (PR #439) made the built-in array authoritative; the hardcoded default only matters during key rotation, where it would trigger false "user-configured override" warnings on old configs.
- **Behavior change (none externally visible):** existing configs with a non-empty `signature_public_key` continue to act as an override (unchanged). New/default configs validate without the placeholder. Verified via new guard test.
- **References:** design `docs/reviews/2026-04-19-wave1-phase4-followups-design.md` §3, plan `docs/reviews/2026-04-19-wave1-phase4-followups-plan.md` Phase B.

## Test plan

- [x] `cargo test -p oneshim-core --lib config` green (2 new tests + 3 pre-existing integrity tests).
- [x] `cargo clippy -p oneshim-core --all-targets -- -D warnings` clean.
- [x] `cargo fmt --check` clean.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 5: Wait for CI, then merge**

Run: `gh pr checks --watch && gh pr merge --squash --delete-branch`

---

# Phase C — PR #3: `boot_count` per-PID markers race mitigation

**Branch:** `fix/updater-boot-count-per-pid-markers`
**Scope:** `src-tauri/src/updater/health_probe.rs` helpers + increment/read rewrite + healthy-writer cleanup + sweep logic + module doc + 2 existing tests modified + 3 new tests
**TDD applicable:** yes — test-then-implement for each new behavior.

---

### Task 16: Create PR #3 branch from updated main

**Files:** (none)

- [ ] **Step 1: Refresh main pointer**

Run: `git fetch origin main && git checkout origin/main`
Expected: detached HEAD on latest main (includes PR #1 and PR #2).

- [ ] **Step 2: Create branch**

Run: `git checkout -b fix/updater-boot-count-per-pid-markers`

---

### Task 17: Write failing test #1 — concurrent-boot no undercount

**Files:**
- Modify: `src-tauri/src/updater/health_probe.rs` — test module at lines 362-584.

- [ ] **Step 1: Read the test module's helper section**

Read tool on `src-tauri/src/updater/health_probe.rs` lines 362-393 to refresh memory of the existing helpers (`write_pending`, `write_boot_count`, `write_self_healthy`).

- [ ] **Step 2: Insert the new test at the end of the test module (before the closing `}` of `mod tests`)**

Use the Edit tool:

`old_string`:
```
    #[test]
    fn probe_io_error_is_non_fatal() {
        // Point install_dir at a non-existent sub-path. The inner probe will
        // encounter read failures (no install_pending → Normal short-circuit);
        // to actually exercise the error path, create a malformed pending file.
        let dir = tempdir().unwrap();
        let pending_path = dir.path().join(".install_pending_0.5.0");
        std::fs::write(&pending_path, b"NOT VALID JSON {{{").unwrap();

        let probe = HealthProbe::new(dir.path().to_path_buf(), "0.5.0".into());
        // Public wrapper catches InstallPendingParse → returns Normal.
        assert_eq!(probe.check_startup_state(), StartupAction::Normal);
    }
}
```

`new_string`:
```
    #[test]
    fn probe_io_error_is_non_fatal() {
        // Point install_dir at a non-existent sub-path. The inner probe will
        // encounter read failures (no install_pending → Normal short-circuit);
        // to actually exercise the error path, create a malformed pending file.
        let dir = tempdir().unwrap();
        let pending_path = dir.path().join(".install_pending_0.5.0");
        std::fs::write(&pending_path, b"NOT VALID JSON {{{").unwrap();

        let probe = HealthProbe::new(dir.path().to_path_buf(), "0.5.0".into());
        // Public wrapper catches InstallPendingParse → returns Normal.
        assert_eq!(probe.check_startup_state(), StartupAction::Normal);
    }

    #[test]
    fn concurrent_boot_count_no_undercount() {
        // Two instances of the same version boot in rapid succession. With
        // the single-file read-modify-write pattern, each read sees the old
        // count and each writes count+1 — losing one increment. With
        // per-PID marker files, each instance records independently and the
        // aggregate count reflects both boots.
        let dir = tempdir().unwrap();
        let version = "0.5.0";

        // Simulate PID 100 and PID 200 each writing their per-PID marker.
        write_boot_count_pid_marker(dir.path(), version, 100);
        write_boot_count_pid_marker(dir.path(), version, 200);

        let probe = HealthProbe::new(dir.path().to_path_buf(), version.into());
        assert_eq!(probe.boot_count().unwrap(), 2);
    }
}
```

Note: `write_boot_count_pid_marker` is a new test-helper function — added in Task 18.

- [ ] **Step 3: Attempt to compile — expect FAIL (missing helper + missing API)**

Run: `cargo test -p oneshim-app --lib updater::health_probe::tests::concurrent_boot_count_no_undercount --no-run 2>&1 | tail -30`
Expected: FAILS with `cannot find function 'write_boot_count_pid_marker'` and `no method named 'boot_count' found for type '&HealthProbe'`.

---

### Task 18: Add per-PID helpers + `boot_count()` method to HealthProbe

**Files:**
- Modify: `src-tauri/src/updater/health_probe.rs` — add methods in `impl HealthProbe`; add test-helper function inside `mod tests`.

- [ ] **Step 1: Add production helpers inside `impl HealthProbe`**

Use the Edit tool on `src-tauri/src/updater/health_probe.rs`:

`old_string`:
```
    fn boot_count_path(&self) -> PathBuf {
        self.install_dir
            .join(format!(".boot_count_{}", self.current_version))
    }

    fn self_healthy_path(&self) -> PathBuf {
        self.install_dir
            .join(format!(".self_healthy_{}", self.current_version))
    }
```

`new_string`:
```
    /// Legacy single-file path — retained only for migration cleanup.
    fn legacy_boot_count_path(&self) -> PathBuf {
        self.install_dir
            .join(format!(".boot_count_{}", self.current_version))
    }

    /// Prefix used by per-PID boot-count marker files for this version.
    fn boot_count_pid_prefix(&self) -> String {
        format!(".boot_count_pid_{}_", self.current_version)
    }

    /// Path for a specific PID's boot-count marker (current version).
    fn boot_count_pid_path(&self, pid: u32) -> PathBuf {
        self.install_dir
            .join(format!("{}{}", self.boot_count_pid_prefix(), pid))
    }

    /// Count the boot attempts recorded for the current version by summing
    /// `.boot_count_pid_{VERSION}_*` marker files. Returns 0 if the install
    /// directory cannot be read (first boot, missing dir, etc.).
    pub(crate) fn boot_count(&self) -> std::io::Result<u32> {
        let prefix = self.boot_count_pid_prefix();
        let entries = match std::fs::read_dir(&self.install_dir) {
            Ok(e) => e,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(err) => return Err(err),
        };
        let mut count: u32 = 0;
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(&prefix) {
                    count = count.saturating_add(1);
                }
            }
        }
        Ok(count)
    }

    /// Record a boot attempt for this process by creating an empty per-PID
    /// marker file. `create_new` makes the write atomic against concurrent
    /// boots — if two processes happen to share a PID (PID reuse), the
    /// second `create_new` returns AlreadyExists and we silently accept
    /// that path (conservative undercount by 1 in the extreme case, vs.
    /// the unbounded race the single-file approach permitted).
    fn record_boot_attempt(&self) -> std::io::Result<()> {
        let path = self.boot_count_pid_path(std::process::id());
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(err) => Err(err),
        }
    }

    /// Remove all boot-count marker files for the current version (both
    /// the new per-PID format and any legacy single-file). Used by the
    /// healthy-writer path.
    fn cleanup_boot_count_markers(&self) -> std::io::Result<()> {
        let prefix = self.boot_count_pid_prefix();
        if let Ok(entries) = std::fs::read_dir(&self.install_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with(&prefix) {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }
        // Legacy single-file cleanup (idempotent — may not exist).
        let _ = std::fs::remove_file(self.legacy_boot_count_path());
        Ok(())
    }

    fn self_healthy_path(&self) -> PathBuf {
        self.install_dir
            .join(format!(".self_healthy_{}", self.current_version))
    }
```

- [ ] **Step 2: Add the test-helper function inside `mod tests`**

Use the Edit tool:

`old_string`:
```
    fn write_boot_count(dir: &Path, version: &str, count: u32) {
        std::fs::write(
            dir.join(format!(".boot_count_{version}")),
            count.to_string(),
        )
        .unwrap();
    }
```

`new_string`:
```
    fn write_boot_count(dir: &Path, version: &str, count: u32) {
        // Legacy single-file format — used only by tests that exercise
        // migration cleanup. New increment path uses per-PID markers.
        std::fs::write(
            dir.join(format!(".boot_count_{version}")),
            count.to_string(),
        )
        .unwrap();
    }

    fn write_boot_count_pid_marker(dir: &Path, version: &str, pid: u32) {
        // Create a single per-PID boot-count marker (simulates a boot).
        std::fs::write(
            dir.join(format!(".boot_count_pid_{version}_{pid}")),
            b"",
        )
        .unwrap();
    }

    fn write_boot_count_pids(dir: &Path, version: &str, count: u32) {
        // Convenience helper for tests that need N simulated boots with
        // distinct PIDs. Uses predictable PIDs starting at 10000 to avoid
        // collision with any actual test-runner PID.
        for i in 0..count {
            write_boot_count_pid_marker(dir, version, 10000 + i);
        }
    }
```

- [ ] **Step 3: Run Task 17's test — expect PASS**

Run: `cargo test -p oneshim-app --lib updater::health_probe::tests::concurrent_boot_count_no_undercount -- --nocapture 2>&1 | tail -20`
Expected: PASS.

---

### Task 19: Rewrite `check_startup_state_inner` to use per-PID markers

**Files:**
- Modify: `src-tauri/src/updater/health_probe.rs` — `check_startup_state_inner` method.

- [ ] **Step 1: Update the read/increment logic**

Use the Edit tool:

`old_string`:
```
        // Step 0 (staleness): if the pending marker is > 24h old and we still
        // have no healthy marker, treat as abandoned (same-version manual
        // reinstall or a device that was powered off for days between boots).
        let pending = read_install_pending(&install_pending)?;
        if is_stale(&pending.installed_at, STALENESS_CUTOFF) {
            tracing::info!(
                "health probe: stale install_pending ({}h+ old) — cleaning abandoned state",
                STALENESS_CUTOFF.as_secs() / 3600
            );
            let _ = std::fs::remove_file(&install_pending);
            let _ = std::fs::remove_file(&boot_count_path);
            return Ok(StartupAction::Normal);
        }

        // Steps 3-5: read boot count, check threshold, increment atomically.
        let current_count = read_boot_count(&boot_count_path).unwrap_or(0);

        if current_count >= u32::from(self.failed_boot_threshold) {
            tracing::warn!(
                "health probe: boot_count={current_count} >= threshold={}; triggering rollback",
                self.failed_boot_threshold
            );
            return Ok(StartupAction::RollbackRequired {
                from_version: self.current_version.clone(),
                to_version: pending.previous_version.clone(),
                backup_path: pending.backup_path.clone(),
                reason: RollbackReason::RepeatedStartupFailure,
            });
        }

        // Increment the counter AFTER the threshold check so a single bad
        // boot is represented as count=1 next time, not count=2. Use a
        // temp-file + rename for atomicity against abrupt termination.
        write_boot_count_atomic(&boot_count_path, current_count + 1)?;
        Ok(StartupAction::Normal)
    }
```

`new_string`:
```
        // Step 0 (staleness): if the pending marker is > 24h old and we still
        // have no healthy marker, treat as abandoned (same-version manual
        // reinstall or a device that was powered off for days between boots).
        let pending = read_install_pending(&install_pending)?;
        if is_stale(&pending.installed_at, STALENESS_CUTOFF) {
            tracing::info!(
                "health probe: stale install_pending ({}h+ old) — cleaning abandoned state",
                STALENESS_CUTOFF.as_secs() / 3600
            );
            let _ = std::fs::remove_file(&install_pending);
            let _ = self.cleanup_boot_count_markers();
            return Ok(StartupAction::Normal);
        }

        // One-time legacy migration: if a pre-per-PID single-file
        // `.boot_count_{VERSION}` exists from an earlier client build, delete
        // it. The new per-PID format is authoritative; the count is rebuilt
        // from whatever per-PID markers already exist (or starts at 0).
        let _ = std::fs::remove_file(self.legacy_boot_count_path());

        // Steps 3-5: count boot attempts, check threshold, record this boot.
        let current_count = self.boot_count().unwrap_or(0);

        if current_count >= u32::from(self.failed_boot_threshold) {
            tracing::warn!(
                "health probe: boot_count={current_count} >= threshold={}; triggering rollback",
                self.failed_boot_threshold
            );
            return Ok(StartupAction::RollbackRequired {
                from_version: self.current_version.clone(),
                to_version: pending.previous_version.clone(),
                backup_path: pending.backup_path.clone(),
                reason: RollbackReason::RepeatedStartupFailure,
            });
        }

        // Record this boot AFTER the threshold check so a single bad boot
        // is represented as count=1 next time, not count=2. `create_new`
        // is atomic and idempotent against concurrent boots.
        self.record_boot_attempt()?;
        Ok(StartupAction::Normal)
    }
```

- [ ] **Step 2: The `let boot_count_path = self.boot_count_path();` line at the top of `check_startup_state_inner` (line 175) references the now-deleted method.**

Since we removed `boot_count_path()` in Task 18, this binding must also go. Use the Edit tool:

`old_string`:
```
    fn check_startup_state_inner(&self) -> Result<StartupAction, ProbeError> {
        let self_healthy = self.self_healthy_path();
        let install_pending = self.install_pending_path();
        let boot_count_path = self.boot_count_path();
```

`new_string`:
```
    fn check_startup_state_inner(&self) -> Result<StartupAction, ProbeError> {
        let self_healthy = self.self_healthy_path();
        let install_pending = self.install_pending_path();
```

- [ ] **Step 3: Verify compilation succeeds for the non-test code**

Run: `cargo check -p oneshim-app 2>&1 | tail -20`
Expected: success. If there's a compile error about the removed helpers, inspect carefully — the only external consumer is the tests.

- [ ] **Step 4: Remove the now-unused helpers `read_boot_count` and `write_boot_count_atomic`**

These file-level functions at lines 255-273 become dead code once `check_startup_state_inner` no longer calls them. Use the Edit tool:

`old_string`:
```
fn read_boot_count(path: &Path) -> Option<u32> {
    let bytes = std::fs::read(path).ok()?;
    let text = std::str::from_utf8(&bytes).ok()?;
    text.trim().parse::<u32>().ok()
}

/// Atomic write via tempfile + rename (same directory as target).
fn write_boot_count_atomic(path: &Path, value: u32) -> Result<(), ProbeError> {
    let parent = path.parent().ok_or_else(|| {
        ProbeError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "boot_count path has no parent",
        ))
    })?;
    let tmp_path = parent.join(format!(".boot_count.tmp.{}", std::process::id()));
    std::fs::write(&tmp_path, value.to_string().as_bytes())?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

```

`new_string`:
```
```

- [ ] **Step 5: Run cargo check again to confirm no dead-code warning or lingering reference**

Run: `cargo check -p oneshim-app 2>&1 | tail -20`
Expected: success, no warnings about unused imports.

---

### Task 20: Update `write_self_healthy_and_cleanup` for per-PID format

**Files:**
- Modify: `src-tauri/src/updater/health_probe.rs` — `write_self_healthy_and_cleanup` function (around lines 294-358).

- [ ] **Step 1: Update the explicit cleanup block (removes legacy + new per-PID files)**

Use the Edit tool:

`old_string`:
```
    // Remove now-stale pending + boot_count files (ignore failures — cleanup
    // is best-effort).
    let _ = std::fs::remove_file(&install_pending_path);
    let _ = std::fs::remove_file(install_dir.join(format!(".boot_count_{version}")));
```

`new_string`:
```
    // Remove now-stale pending file (ignore failures — cleanup is best-effort).
    let _ = std::fs::remove_file(&install_pending_path);

    // Remove all per-PID boot-count markers for the CURRENT version + the
    // legacy single-file if still present. Foreign-version files are
    // handled by the sweep below.
    let current_prefix = format!(".boot_count_pid_{version}_");
    if let Ok(entries) = std::fs::read_dir(install_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.starts_with(&current_prefix) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
    let _ = std::fs::remove_file(install_dir.join(format!(".boot_count_{version}")));
```

- [ ] **Step 2: Update the foreign-version state-file sweep to handle per-PID format**

Use the Edit tool:

`old_string`:
```
            // (b) Foreign-version state-file sweep. Match
            //     `.install_pending_<VER>`, `.boot_count_<VER>`,
            //     `.self_healthy_<VER>` where VER is non-empty and != the
            //     current version. The `is_empty()` guard is defensive:
            //     no production code path produces an empty-suffix file,
            //     but if one existed it would NOT be swept (safer default).
            for prefix in [".install_pending_", ".boot_count_", ".self_healthy_"] {
                if let Some(ver_suffix) = name.strip_prefix(prefix) {
                    if !ver_suffix.is_empty() && ver_suffix != version {
                        let _ = std::fs::remove_file(&path);
                    }
                    break;
                }
            }
        }
    }
```

`new_string`:
```
            // (b) Foreign-version per-PID boot-count sweep. Format is
            //     `.boot_count_pid_<VER>_<PID>`. Extract VER (the segment
            //     before the final `_` separating version from PID).
            if let Some(suffix) = name.strip_prefix(".boot_count_pid_") {
                if let Some((ver, _pid)) = suffix.rsplit_once('_') {
                    if !ver.is_empty() && ver != version {
                        let _ = std::fs::remove_file(&path);
                    }
                }
                continue;
            }

            // (c) Foreign-version legacy single-file boot-count sweep.
            //     Always deleted when encountered — the per-PID format is
            //     authoritative now, so any `.boot_count_<VER>` residual
            //     (regardless of VER) is stale.
            if let Some(ver_suffix) = name.strip_prefix(".boot_count_") {
                if !ver_suffix.is_empty() {
                    let _ = std::fs::remove_file(&path);
                }
                continue;
            }

            // (d) Foreign-version install-pending / self-healthy sweep.
            for prefix in [".install_pending_", ".self_healthy_"] {
                if let Some(ver_suffix) = name.strip_prefix(prefix) {
                    if !ver_suffix.is_empty() && ver_suffix != version {
                        let _ = std::fs::remove_file(&path);
                    }
                    break;
                }
            }
        }
    }
```

- [ ] **Step 3: Run existing healthy-writer tests to confirm sweep still works**

Run: `cargo test -p oneshim-app --lib updater::health_probe::tests::healthy_writer_cleanup_sweeps_foreign_version_state_files -- --nocapture 2>&1 | tail -30`

Expected: this test currently seeds `.boot_count_0.4.40` (legacy single-file) and asserts it's removed. Our new sweep branch `(c)` handles exactly this path — should still PASS.

- [ ] **Step 4: Run the spawn-healthy-writer integration test**

Run: `cargo test -p oneshim-app --lib updater::health_probe::tests::spawn_healthy_writer_sets_marker_after_injected_short_delay -- --nocapture 2>&1 | tail -30`

This test currently asserts `probe.boot_count_path().exists() == false`. We removed `boot_count_path()`, so this test fails to compile. **We fix it in Task 21.**

---

### Task 21: Update existing tests that referenced the removed `boot_count_path()`

**Files:**
- Modify: `src-tauri/src/updater/health_probe.rs` — test module.

- [ ] **Step 1: Update `check_startup_below_failed_boot_threshold_is_normal`**

This test (around lines 421-440) currently does:
```rust
// Confirm counter was bumped.
let new_count = read_boot_count(&probe.boot_count_path()).unwrap();
assert_eq!(new_count, 1);
```

Both `read_boot_count` and `boot_count_path` are removed. Replace with the public API:

Use the Edit tool:

`old_string`:
```
        let probe = HealthProbe::new(dir.path().to_path_buf(), "0.5.0".into());
        assert_eq!(probe.check_startup_state(), StartupAction::Normal);

        // Confirm counter was bumped.
        let new_count = read_boot_count(&probe.boot_count_path()).unwrap();
        assert_eq!(new_count, 1);
    }
```

`new_string`:
```
        let probe = HealthProbe::new(dir.path().to_path_buf(), "0.5.0".into());
        assert_eq!(probe.check_startup_state(), StartupAction::Normal);

        // Confirm counter was bumped — exactly one per-PID marker exists
        // for the current version.
        assert_eq!(probe.boot_count().unwrap(), 1);
    }
```

- [ ] **Step 2: Update `check_startup_at_failed_boot_threshold_triggers_rollback`**

Same test file, around lines 443-476. Currently seeds `write_boot_count(dir.path(), "0.5.0", 2)` (legacy format) and asserts `read_boot_count(&probe.boot_count_path()).unwrap() == 2` after rollback.

Use the Edit tool:

`old_string`:
```
        write_boot_count(dir.path(), "0.5.0", 2); // at threshold

        let probe = HealthProbe::new(dir.path().to_path_buf(), "0.5.0".into());
        match probe.check_startup_state() {
            StartupAction::RollbackRequired {
                from_version,
                to_version,
                backup_path,
                reason,
            } => {
                assert_eq!(from_version, "0.5.0");
                assert_eq!(to_version, "0.4.39");
                assert_eq!(backup_path, backup);
                assert_eq!(reason, RollbackReason::RepeatedStartupFailure);
            }
            other => panic!("Expected RollbackRequired, got {:?}", other),
        }

        // At-threshold does NOT bump the counter further; the next boot's
        // probe will still see count=2 if rollback somehow fails to execute.
        let count_after = read_boot_count(&probe.boot_count_path()).unwrap();
        assert_eq!(count_after, 2);
    }
```

`new_string`:
```
        write_boot_count_pids(dir.path(), "0.5.0", 2); // at threshold via per-PID markers

        let probe = HealthProbe::new(dir.path().to_path_buf(), "0.5.0".into());
        match probe.check_startup_state() {
            StartupAction::RollbackRequired {
                from_version,
                to_version,
                backup_path,
                reason,
            } => {
                assert_eq!(from_version, "0.5.0");
                assert_eq!(to_version, "0.4.39");
                assert_eq!(backup_path, backup);
                assert_eq!(reason, RollbackReason::RepeatedStartupFailure);
            }
            other => panic!("Expected RollbackRequired, got {:?}", other),
        }

        // At-threshold does NOT record a new boot; the next probe still
        // sees count=2 if rollback somehow fails to execute.
        assert_eq!(probe.boot_count().unwrap(), 2);
    }
```

- [ ] **Step 3: Update `spawn_healthy_writer_sets_marker_after_injected_short_delay`**

Around lines 496-523. Two assertions reference removed methods:
1. `write_boot_count(dir.path(), "0.5.0", 0);` — legacy setup; drop it (0 boots == no markers needed).
2. `assert!(!probe.boot_count_path().exists());` — replace with per-PID count check.

Use the Edit tool:

`old_string`:
```
        write_pending(
            dir.path(),
            "0.5.0",
            &Utc::now().to_rfc3339(),
            "0.4.39",
            &backup,
        );
        write_boot_count(dir.path(), "0.5.0", 0);

        let probe = HealthProbe::new(dir.path().to_path_buf(), "0.5.0".into())
            .with_threshold(Duration::from_millis(50));

        let handle = probe.spawn_healthy_writer();
        handle.await.unwrap();

        let marker = dir.path().join(".self_healthy_0.5.0");
        assert!(marker.exists(), "healthy marker should have been written");
        // Cleanup should have removed install_pending + boot_count.
        assert!(!probe.install_pending_path().exists());
        assert!(!probe.boot_count_path().exists());
        // Backup_path recorded in pending should survive the sweep.
        assert!(backup.exists(), "canonical backup should remain");
    }
```

`new_string`:
```
        write_pending(
            dir.path(),
            "0.5.0",
            &Utc::now().to_rfc3339(),
            "0.4.39",
            &backup,
        );
        // Seed 2 per-PID boot markers to confirm cleanup removes all of them.
        write_boot_count_pids(dir.path(), "0.5.0", 2);

        let probe = HealthProbe::new(dir.path().to_path_buf(), "0.5.0".into())
            .with_threshold(Duration::from_millis(50));

        let handle = probe.spawn_healthy_writer();
        handle.await.unwrap();

        let marker = dir.path().join(".self_healthy_0.5.0");
        assert!(marker.exists(), "healthy marker should have been written");
        // Cleanup should have removed install_pending + all per-PID markers.
        assert!(!probe.install_pending_path().exists());
        assert_eq!(probe.boot_count().unwrap(), 0);
        // Backup_path recorded in pending should survive the sweep.
        assert!(backup.exists(), "canonical backup should remain");
    }
```

- [ ] **Step 4: Run all existing tests to confirm they still pass**

Run: `cargo test -p oneshim-app --lib updater::health_probe -- --nocapture 2>&1 | tail -40`
Expected: all 7 original tests + 1 new concurrent test PASS. (8 total so far.)

---

### Task 22: Add test — cleanup removes all per-PID files + legacy file

**Files:**
- Modify: `src-tauri/src/updater/health_probe.rs` — append test.

- [ ] **Step 1: Add the cleanup test at the end of `mod tests`**

Use the Edit tool. Find the closing brace of the test module (just after `concurrent_boot_count_no_undercount`) and insert before it:

`old_string`:
```
    #[test]
    fn concurrent_boot_count_no_undercount() {
        // Two instances of the same version boot in rapid succession. With
        // the single-file read-modify-write pattern, each read sees the old
        // count and each writes count+1 — losing one increment. With
        // per-PID marker files, each instance records independently and the
        // aggregate count reflects both boots.
        let dir = tempdir().unwrap();
        let version = "0.5.0";

        // Simulate PID 100 and PID 200 each writing their per-PID marker.
        write_boot_count_pid_marker(dir.path(), version, 100);
        write_boot_count_pid_marker(dir.path(), version, 200);

        let probe = HealthProbe::new(dir.path().to_path_buf(), version.into());
        assert_eq!(probe.boot_count().unwrap(), 2);
    }
}
```

`new_string`:
```
    #[test]
    fn concurrent_boot_count_no_undercount() {
        // Two instances of the same version boot in rapid succession. With
        // the single-file read-modify-write pattern, each read sees the old
        // count and each writes count+1 — losing one increment. With
        // per-PID marker files, each instance records independently and the
        // aggregate count reflects both boots.
        let dir = tempdir().unwrap();
        let version = "0.5.0";

        // Simulate PID 100 and PID 200 each writing their per-PID marker.
        write_boot_count_pid_marker(dir.path(), version, 100);
        write_boot_count_pid_marker(dir.path(), version, 200);

        let probe = HealthProbe::new(dir.path().to_path_buf(), version.into());
        assert_eq!(probe.boot_count().unwrap(), 2);
    }

    #[test]
    fn cleanup_boot_count_markers_removes_per_pid_and_legacy_files() {
        let dir = tempdir().unwrap();
        let version = "0.5.0";

        // Seed: 3 per-PID markers + 1 legacy single-file.
        write_boot_count_pids(dir.path(), version, 3);
        write_boot_count(dir.path(), version, 7);

        let probe = HealthProbe::new(dir.path().to_path_buf(), version.into());
        assert_eq!(probe.boot_count().unwrap(), 3);

        probe.cleanup_boot_count_markers().unwrap();

        assert_eq!(probe.boot_count().unwrap(), 0);
        assert!(
            !probe.legacy_boot_count_path().exists(),
            "legacy single-file must be removed"
        );
    }

    #[test]
    fn legacy_single_file_removed_by_startup_migration() {
        // A pre-per-PID build left `.boot_count_0.5.0` behind with the
        // single-file format. The current probe sees the pending-install
        // marker, migrates by deleting the legacy file, and records a fresh
        // per-PID boot attempt.
        let dir = tempdir().unwrap();
        let backup = dir.path().join("oneshim.rollback.1");
        std::fs::write(&backup, b"backup-bytes").unwrap();
        write_pending(
            dir.path(),
            "0.5.0",
            &Utc::now().to_rfc3339(),
            "0.4.39",
            &backup,
        );
        // Legacy single-file with count=1 (below threshold).
        write_boot_count(dir.path(), "0.5.0", 1);

        let probe = HealthProbe::new(dir.path().to_path_buf(), "0.5.0".into());
        assert_eq!(probe.check_startup_state(), StartupAction::Normal);

        // Legacy file removed; migration drops the legacy count.
        assert!(
            !probe.legacy_boot_count_path().exists(),
            "legacy single-file must be removed during migration"
        );
        // New per-PID marker for THIS process is in place.
        assert_eq!(
            probe.boot_count().unwrap(),
            1,
            "this boot is recorded via the new per-PID format"
        );
    }
}
```

- [ ] **Step 2: Run the two new tests**

Run: `cargo test -p oneshim-app --lib updater::health_probe::tests::cleanup_boot_count_markers_removes_per_pid_and_legacy_files updater::health_probe::tests::legacy_single_file_removed_by_startup_migration -- --nocapture 2>&1 | tail -30`
Expected: both PASS.

---

### Task 23: Update module-level doc header to remove "Known limitation I-3"

**Files:**
- Modify: `src-tauri/src/updater/health_probe.rs` — module header (lines 25-44).

- [ ] **Step 1: Replace the `# Known limitations` block**

Use the Edit tool:

`old_string`:
```
//! # Known limitations (Loop 3 iter 1 review)
//!
//! - **Boot-counter ordering** (I-4): `failed_boot_threshold = 2` triggers
//!   rollback on the THIRD boot that fails to reach a self-healthy marker
//!   (boot 1 increments 0→1, boot 2 increments 1→2, boot 3 reads 2 ≥ 2
//!   and rolls back). The "2" in the name refers to the maximum retry
//!   count, not the total boot count. This matches the standard
//!   read-then-increment-after-threshold-check pattern.
//!
//! - **Concurrent-process race** (I-3): the "read count; compare threshold;
//!   write count+1" sequence is NOT atomic against two processes of the
//!   same version starting simultaneously (e.g., desktop shortcut + Tauri
//!   autolaunch firing in the same second on first-install). Both reads
//!   see the old value; both writes race. Mitigation deferred to a
//!   follow-up — low-probability path and the worst-case outcome is an
//!   unnecessary rollback after a single real success, which the user
//!   can work around by upgrading again. Fix candidate:
//!   `OpenOptions::new().create_new(true)` for `.boot_count_pid_{PID}`
//!   sub-files, then sum at read-time.
```

`new_string`:
```
//! # Counter semantics
//!
//! - **Boot-counter ordering**: `failed_boot_threshold = 2` triggers
//!   rollback on the THIRD boot that fails to reach a self-healthy marker
//!   (boot 1 creates marker, count becomes 1; boot 2 creates marker,
//!   count becomes 2; boot 3 reads count=2 ≥ 2 and rolls back). The "2"
//!   refers to the maximum retry count, not the total boot count.
//!
//! - **Concurrent-process safety**: each boot creates one
//!   `.boot_count_pid_{VERSION}_{PID}` marker file via `create_new`
//!   (atomic). The count is derived by listing the directory at
//!   read-time. No read-modify-write sequence exists — concurrent boots
//!   of the same version each record independently. PID reuse across
//!   the lifetime of the install_pending window (< 24h per staleness
//!   rule) is possible but rare; the second `create_new` returns
//!   AlreadyExists and we treat that as "already recorded" (conservative
//!   undercount by 1 in the extreme case).
```

- [ ] **Step 2: Verify the header compiles (doc comments can break if formatting is off)**

Run: `cargo check -p oneshim-app 2>&1 | tail -10`
Expected: no errors.

---

### Task 24: Full workspace test run + clippy + fmt

**Files:** (verification only)

- [ ] **Step 1: Run all health_probe tests**

Run: `cargo test -p oneshim-app --lib updater::health_probe -- --nocapture 2>&1 | tail -30`
Expected: 10 tests pass (7 existing + 3 new; 2 existing were modified in place).

- [ ] **Step 2: Full workspace test run**

Run: `cargo test --workspace 2>&1 | tail -30`
Expected: no regressions across the workspace. Memory's baseline from PR #446 is 3,455 passed / 0 failed / 21 ignored. After PR #3: expect 3,458 passed (baseline + 3 new in health_probe; PR #2 added 2 new, already merged → expected workspace baseline here is 3,457 after PR #2, then 3,460 after PR #3 lands).

- [ ] **Step 3: Clippy**

Run: `cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -30`
Expected: clean.

- [ ] **Step 4: Fmt check**

Run: `cargo fmt --check 2>&1 | tail -20`
Expected: no output (clean). If fmt diffs exist, run `cargo fmt` and restage changes.

---

### Task 25: Update CHANGELOG for PR #3

**Files:**
- Modify: `CHANGELOG.md` — `[Unreleased]` → existing `### Fixed` subsection.

- [ ] **Step 1: Append the bullet under `### Fixed`**

Use the Edit tool:

`old_string`:
```
### Fixed

- `notarize-macos-release-assets` workflow now auto-triggers for `workflow_dispatch`-originated parent release runs. Previous `startsWith(head_branch, 'v')` gate filtered dispatched parents (where `head_branch == main`) out of the notarize path; tag resolution now uses the parent run's `displayTitle` via `gh run view` with a regex fallback against the full display-title payload.
```

`new_string`:
```
### Fixed

- `notarize-macos-release-assets` workflow now auto-triggers for `workflow_dispatch`-originated parent release runs. Previous `startsWith(head_branch, 'v')` gate filtered dispatched parents (where `head_branch == main`) out of the notarize path; tag resolution now uses the parent run's `displayTitle` via `gh run view` with a regex fallback against the full display-title payload.
- Updater `health_probe` boot-count tracking now uses per-PID marker files (`.boot_count_pid_{VERSION}_{PID}`) instead of a single `.boot_count_{VERSION}` file, eliminating the read-modify-write race when multiple instances of the same version start concurrently. Aggregate count is computed by enumerating markers at read-time via `create_new` atomic writes. Threshold and rollback semantics unchanged; legacy single-file from pre-migration installs is deleted on first boot.
```

---

### Task 26: Commit + push + open PR #3

**Files:**
- `src-tauri/src/updater/health_probe.rs`
- `CHANGELOG.md`

- [ ] **Step 1: Stage**

Run: `git add src-tauri/src/updater/health_probe.rs CHANGELOG.md`

- [ ] **Step 2: Commit**

Run:
```bash
git commit -m "$(cat <<'EOF'
fix(updater): per-PID boot_count markers eliminate concurrent-boot race

The previous single-file .boot_count_{VERSION} used a read-modify-write
sequence: two processes of the same version starting simultaneously
(e.g. desktop shortcut + Tauri autolaunch on first-install) each read
the old count and each wrote count+1, losing one increment.

Replace with per-PID marker files .boot_count_pid_{VERSION}_{PID}
created via OpenOptions::create_new (atomic). Read-path counts the
markers via read_dir. Healthy-writer cleanup now deletes all per-PID
markers plus any legacy single-file residual. Foreign-version sweep
extended to handle the new prefix.

Closes Phase 4 deferred follow-up #2 (memory: project_phase4_complete.md).

- 2 existing tests updated for the new API surface.
- 3 new tests: concurrent-boot aggregation, cleanup idempotency, legacy migration.
- Module header's "Known limitation I-3" section replaced with counter-semantics docs.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 3: Push**

Run: `git push -u origin fix/updater-boot-count-per-pid-markers`

- [ ] **Step 4: Open PR**

Run:
```bash
gh pr create --title "fix(updater): per-PID boot_count markers eliminate concurrent-boot race" --body "$(cat <<'EOF'
## Summary

- **Bug:** `.boot_count_{VERSION}` read-modify-write was not atomic against two processes of the same version starting simultaneously. Both reads saw the old value; both writes raced, losing an increment and potentially defeating the `failed_boot_threshold = 2` auto-rollback guard.
- **Fix:** per-PID marker files `.boot_count_pid_{VERSION}_{PID}` written via `OpenOptions::create_new` (atomic). Aggregate count derived from directory enumeration at read-time. No shared mutable state.
- **Semantics preserved:** threshold check, rollback path, staleness rule, healthy-writer cleanup all unchanged from the caller's perspective.
- **Legacy migration:** one-time delete of pre-per-PID single-file on first boot after upgrade. Legacy count is dropped; a failed-boot cycle would have been re-accumulated on the next crash anyway.
- **References:** design `docs/reviews/2026-04-19-wave1-phase4-followups-design.md` §4, plan `docs/reviews/2026-04-19-wave1-phase4-followups-plan.md` Phase C.

## Test plan

- [x] `cargo test -p oneshim-app --lib updater::health_probe` green (10 tests: 7 existing + 3 new; 2 existing modified in place).
- [x] `cargo test --workspace` green, no regressions (expected baseline after PR #2: 3,457 → after PR #3: 3,460).
- [x] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [x] `cargo fmt --check` clean.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 5: Wait for CI and merge**

Run: `gh pr checks --watch && gh pr merge --squash --delete-branch`

---

# Phase D — Post-merge housekeeping

### Task 27: Refresh local main + update memory

**Files:**
- Update memory files to reflect shipped Wave 1 items.

- [ ] **Step 1: Pull latest main**

Run: `git fetch origin main && git checkout origin/main && git log --oneline origin/main -5`
Expected: top 3 commits are the three squash-merge commits for PR #1, #2, #3.

- [ ] **Step 2: Update `project_next_tasks.md`**

Using the Edit tool on `/Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.claude/projects/-Volumes-ext-PCIe4-1TB-bjsmacminim4-ext-Documents-vscode---INDIVISUAL---oneshim--git-modules-oneshim-agent-client-rust/memory/project_next_tasks.md`:

- Move the three shipped items under "Remaining work" from ⏳ to ✅ rows.
- Note the new `origin/main` HEAD and session summary.
- Explicitly note D13 was re-categorized: **Wave 1 dropped D13** → D13 is now part of Phase 4 remainder (alongside C5/D7).

- [ ] **Step 3: Update `project_phase4_complete.md`**

Using the Edit tool: mark items 2 (concurrent-process race) and 4 (notarize head_branch) and 6 (I-4 signature_public_key) in the "Deferred follow-ups" list as ✅ SHIPPED with PR numbers. Remaining deferred: Windows rollback + M-3 simulator deletion.

- [ ] **Step 4: Update `MEMORY.md` index if anything becomes stale**

Using the Edit tool on `MEMORY.md`: if any active-project-context entries became stale (e.g., "no open PRs" changes or PR pointers drift), refresh one-liner descriptions. Keep entries ordered most-recent-first.

---

## Self-Review Checklist (completed inline during writing)

**1. Spec coverage**: Every item in the design doc has a task:
- §2 Notarize fix → Tasks 3, 4 (condition + tag resolution)
- §3 signature_public_key default → Tasks 10, 11, 12 (test, implement, guard test)
- §4 boot_count per-PID → Tasks 17–23 (helpers, rewrite, cleanup, migration, tests, module doc)
- §5 Execution order → Phase A/B/C structure
- §6 Risks → mitigations embedded in tasks (e.g., Task 20 sweep handles legacy collision)
- §7 Acceptance criteria → Task 24 (full test + clippy + fmt) + Task 27 (memory refresh) satisfy all bullets

**2. Placeholder scan**: no "TBD", "TODO", "implement later" in task bodies. Expected command outputs are explicit.

**3. Type consistency**: method names consistent across tasks. `boot_count()`, `record_boot_attempt()`, `cleanup_boot_count_markers()`, `legacy_boot_count_path()`, `boot_count_pid_path(pid)`, `boot_count_pid_prefix()` — each defined once in Task 18 and referenced by the same name in Tasks 19, 20, 21, 22.

**4. Notarize Level B decision**: plan assumes Level B (full fix) per design §2.2 recommendation. If `gh run view --json displayTitle` proves unreliable at manual-dispatch test time (Task 8's follow-up), a dedicated follow-up PR can drop Level A. Plan does not attempt to revert; any revert becomes its own mini-plan.

---

## Execution Choice

Plan complete and saved to `docs/reviews/2026-04-19-wave1-phase4-followups-plan.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task (or per Phase A/B/C). Fresh context per block, structured reviews between merges.

**2. Inline Execution** — Execute tasks in this session using executing-plans. Batch-check in with you at Phase A/B/C boundaries.

Which approach?
