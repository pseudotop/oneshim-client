[English](./2026-05-04-phase-6-migration-plan.md) | [한국어](./2026-05-04-phase-6-migration-plan.ko.md)

---
status: Draft
target_phase: 6
companion_strategy: docs/plan/2026-04-30-maekon-client-public-oss-strategy.md
---

# Phase 6 — Monorepo Migration Plan

## 1. Purpose

Concrete, ordered migration steps for moving `client-rust` from its current state
(separate `pseudotop/oneshim-client` repository, included in the parent
`pseudotop/oneshim` monorepo as a Git submodule) into a parent-internal
single-source-of-truth at `clients/maekon-client/`, with public export driven
from `tools/public-export/maekon-client/` per the strategy doc.

Companion: [`2026-04-30-maekon-client-public-oss-strategy.md`](./2026-04-30-maekon-client-public-oss-strategy.md).
This document narrows the strategy decisions into stepwise actions, rollback
points, and validation gates.

## 2. Scope

In scope:
- Submodule removal in parent.
- Snapshot/import of `client-rust` into `clients/maekon-client/`.
- Construction of `tools/public-export/maekon-client/` (export tool Option B per
  the 2026-05-04 decision).
- Path-reference sweep across the 90 parent files that mention `client-rust`.
- Cut-over to the new export tool with a parallel-run validation period.

Out of scope (deferred to Phase 7 or later):
- Public release tagging and notarization of `maekon-client`.
- Final `oneshim-client` archive notice.
- `maekon.dev` / `docs.maekon.dev` deploy units.
- Maekon hard-reset on the public repository.

## 3. Pre-conditions (Entry Gates)

| # | Gate (strategy §260) | Status (2026-05-04) | Verification needed |
|---|------------------------|---------------------|---------------------|
| 1 | Parent main is stable; latest `origin/main` reviewed | 🟡 Partial — 0 open PRs, ADR-070 R-FU-5 + ADR-067 #6 series settled at PR #1025; cadence remains active (DPoP + UX work) | User-side judgment |
| 2 | Existing parent `client-rust` submodule workflow docs identified | ✅ 90 parent file references catalogued; 0 in `.github/workflows/` | — |
| 3 | `clients/maekon-client/` does not disturb parent `server`/`backoffice`/`docs` structure | 🟡 Pre-flight — `clients/` and `tools/` directories don't exist yet | Step 2 dry-run |
| 4 | Export tooling can accept a parent source path | ✅ Option B confirmed (build new tool at `tools/public-export/maekon-client/`); current `client-rust/scripts/export-public-repo.sh` keeps working in parallel until cut-over | Step 5 cut-over |
| 5 | Public export gates block parent-only path leakage | ✅ Existing `forbidden_paths` already covers `server`, `backoffice`, `terraform`, `tests/private`; will be ported to `tools/public-export/maekon-client/exclude.txt` | Step 2 smoke compare |
| 6 | Maekon/ONESHIM relationship copy applicable to README, install docs, login/OAuth, update docs | ⚠️ Audit pending | Step 6 |
| 7 | Public contribution handling copy ready | ⚠️ `SECURITY.md` already on maekon-client; `CONTRIBUTING.md` gap to be audited (Phase 6 prep follow-up #4) | Step 7 |
| (operations) | Parent local debug + release modes verified | ❌ User-side verification still pending | User signal |

## 4. Migration Steps

Steps are ordered. Step N+1 should not start until Step N's validation gate
passes.

### Step 1 — Pre-flight in `client-rust` SSOT

Goal: capture a known-good baseline so Step 3 can be reproduced or rolled back.

Actions:
- Run a final sync round (`oneshim-client` → `maekon-client`) so the public
  mirror is pinned to the same commit baseline. (At entry to Phase 6 this may
  already be true if no upstream PRs landed since the last sync.)
- Tag `client-rust` with `phase-6-baseline-YYYYMMDD` at the chosen migration
  commit. The parent submodule pointer must reference this commit.
- Capture the baseline:
  - `git -C client-rust rev-parse HEAD`
  - `git -C oneshim ls-tree HEAD client-rust | awk '{print $3}'`
  - `cargo deny check`, `cargo clippy`, `cargo test` results recorded.
- Run a dry-run export from the baseline and store the output tree (~30 MB,
  ~3,600 files) for Step 2 comparison.

Gate: baseline tag pushed, validation results recorded, dry-run output stored.

### Step 2 — Build `tools/public-export/maekon-client/` (parent)

Goal: stand up Option B alongside the existing client-side tool, validated
against the same input.

Directory structure to create in parent:

```
tools/public-export/maekon-client/
├── README.md           # operations guide (sync round procedure)
├── export.sh           # entry point; mirrors client-rust/scripts/export-public-repo.sh logic
├── exclude.txt         # ported from client-rust/scripts/public-repo-exclude.txt
└── overlays/
    └── .github/
        └── ISSUE_TEMPLATE/
            ├── bug_report.yml
            ├── config.yml
            ├── feature_request.yml
            └── install_release_issue.yml
```

Key adjustments from the existing tool:
- `REPO_ROOT` now points to the parent monorepo root.
- Source archival uses `git archive HEAD:clients/maekon-client/ | tar -x …` to
  scope to the subtree (Step 3 must complete before this fully resolves; until
  then `export.sh` operates from the existing submodule path with a
  `--source-path` override flag for Step 2 smoke testing).
- `validate_public_export()` ports verbatim — required and forbidden path
  lists, internal-reference scans, stale public-repo-ref scans.
- ISSUE_TEMPLATE overlay automation: after `rsync`, copy
  `tools/public-export/maekon-client/overlays/.github/` into the destination,
  removing the manual workaround currently documented in
  `project_maekon_sync_workflow` memory.

Validation:
- Run Step 2 export against the Step 1 baseline.
- `diff -ru` the output against Step 1's stored tree. Expected: identical
  except for the overlays directory automation (which we're adding here).
- If non-trivial differences, debug before proceeding to Step 3.

Gate: identical or strictly-additive output vs. Step 1 dry-run.

### Step 3 — Import `client-rust` into `clients/maekon-client/`

Goal: convert the submodule into a parent-internal directory.

Three import strategies to choose from (decision required, see §7):

#### 3a — Subtree merge (preserves history, recommended default)

```bash
# in parent oneshim/
git remote add client-rust-import https://github.com/pseudotop/oneshim-client.git
git fetch client-rust-import main
git merge --allow-unrelated-histories -s ours --no-commit client-rust-import/main
git read-tree --prefix=clients/maekon-client/ -u client-rust-import/main
git submodule deinit -- client-rust
git rm -f client-rust
# remove client-rust entry from .gitmodules (or remove file if empty)
git commit -m "feat(monorepo): import client-rust as clients/maekon-client (subtree)"
```

Pros: full history reachable via `git log clients/maekon-client/`.
Cons: large initial commit; mixes histories at the merge base.

#### 3b — Snapshot copy (no history)

```bash
# in parent oneshim/
git submodule deinit -- client-rust
rsync -a --exclude='.git/' client-rust/ clients/maekon-client/
git rm -f client-rust
# remove client-rust entry from .gitmodules
git add clients/maekon-client/ .gitmodules
git commit -m "feat(monorepo): import client-rust as clients/maekon-client (snapshot)"
```

Pros: clean diff, easy review.
Cons: no `git log`/`git blame` continuity for code that came from `oneshim-client`.

#### 3c — `git filter-branch` / `git-filter-repo` (full history, full prefix rewrite)

Heaviest option. Useful only if blame/log continuity is mandatory.

Validation per option:
- `cargo check --workspace` from parent root succeeds.
- `clients/maekon-client/Cargo.toml` is valid.
- Submodule pointer is removed (no `[submodule "client-rust"]` in `.gitmodules`).
- Submodule directory is removed (`client-rust/` no longer present in tree).

Gate: chosen import strategy applied, parent compiles from `clients/maekon-client/`.

### Step 4 — Path reference sweep across parent

Goal: update the 90 parent file references identified in the 2026-05-04
submodule scan.

Categories and approach:
- ADR plan/spec docs (`server/docs/plans/*.md`, `server/docs/domains/.../README.md`,
  etc.): bulk `sed` rewrite of `client-rust/` → `clients/maekon-client/`.
- `.claude/agents/{rust-core-owner,rust-runtime-owner,qa-gatekeeper}.md`:
  hand-edit because the agent text often references the workflow, not just the
  path.
- Top-level `CLAUDE.md`, `README.md`, `SECURITY.md`, `tests/CLAUDE.md`,
  `server/CLAUDE.md`: hand-edit; these are user-facing.
- `tests/private/client-rust/`:
  - Decision (see §7): move to `clients/maekon-client/tests/private/` or
    keep at `tests/private/client-rust/` and rename to
    `tests/private/maekon-client/`.
  - Run scripts (`run.sh`, `run-frontend.sh`, `run-e2e-tauri.sh`,
    `run-e2e-live.sh`) reference the submodule path directly — must be
    updated.
- `.gitmodules`: remove `[submodule "client-rust"]` block (already removed in
  Step 3 if 3a/3b followed).

Validation:
- `grep -rn "client-rust" --include="*.md" --include="*.sh" --include="*.yml"
  --include="*.yaml" --include="*.json" --include="*.toml"` across parent
  shows only intentional historical mentions (CHANGELOG, archived notes).
- All `tests/private` run scripts execute successfully against the new path.

Gate: zero unintentional `client-rust` references remain; tests/private scripts
pass smoke run.

### Step 5 — Cut over public export

Goal: switch the sync round procedure from
`client-rust/scripts/export-public-repo.sh` to
`tools/public-export/maekon-client/export.sh`.

Procedure:
- Run two sync rounds in parallel (one with each tool) and confirm identical
  PR diffs on `maekon-client`. (If Step 2's smoke compare was clean, this is a
  formality — but the parallel run is the cut-over forcing function.)
- Update `project_maekon_sync_workflow` memory to reference the new tool path
  and remove the manual ISSUE_TEMPLATE overlay step.
- Update `docs/guides/public-repo-launch-playbook.md` (parent and any
  client-rust copy) to reference the new tool location.
- In the (now relocated) `clients/maekon-client/scripts/export-public-repo.sh`,
  add a deprecation banner echoing the new tool path and `exit 1`. Remove the
  script outright in Phase 7.

Validation:
- Sync round N+1 (post-cut-over) produces a diff signature equivalent to the
  parallel run.
- Memory and playbook updates merged.

Gate: cut-over commit landed; deprecation notice live; no consumers of the
client-side script remain.

### Step 6 — Maekon language sweep

Goal: apply the Maekon/ONESHIM relationship copy from strategy §90 across
user-facing surfaces.

Surfaces to audit:
- `clients/maekon-client/README.md`
- `clients/maekon-client/docs/guides/install*.md`
- Login/OAuth UI strings (frontend i18n)
- Update flow docs and prompts
- Public-facing release notes templates

Validation:
- Spot-check by user; copy review against strategy §90.

Gate: user sign-off on copy.

### Step 7 — Public contribution templates

Goal: close the gap on Follow-up #4 (templates).

Actions:
- Audit current `maekon-client` contents: `SECURITY.md` ✅, ISSUE_TEMPLATE ✅
  (via Step 2 overlays).
- Create `CONTRIBUTING.md` skeleton if missing on the public side.
- Verify `dependabot.yml`, `CODE_OF_CONDUCT.md`, license files all present and
  current on the public mirror after the next sync.

Validation:
- Public repo diff after Step 5 cut-over shows the new templates landing
  cleanly.

Gate: maekon-client renders contribution-ready (SECURITY, CONTRIBUTING,
ISSUE_TEMPLATE, CODE_OF_CONDUCT, LICENSE all present and consistent).

## 5. Rollback Scenarios

### Rollback at Step 2 (export tool build)
- Discard the `tools/public-export/maekon-client/` work-in-progress branch.
- Existing `client-rust/scripts/export-public-repo.sh` continues serving sync
  rounds. No user-visible impact.

### Rollback at Step 3 (mid-import)
- `git reset --hard <pre-import-commit>` on parent.
- Restore submodule: `git submodule add` + sync `client-rust` to baseline tag.
- Verify parent `cargo check --workspace` succeeds with submodule active.

### Rollback at Step 4 (path sweep)
- Revert the path-update commits.
- Submodule still removed; clients/maekon-client/ still present; only
  references roll back. Parent CI keeps working because the actual code path
  is correct — the rollback is purely for downstream doc/reference cleanup
  if the sweep introduced regressions.

### Rollback at Step 5 (cut-over)
- Re-enable `client-rust/scripts/export-public-repo.sh` (now relocated to
  `clients/maekon-client/scripts/`).
- Update memory + playbook back to old tool path.
- Sync rounds resume on the legacy tool until the parallel-run regression is
  diagnosed.

### Hard rollback (post-Step 3, full)
- Treated as exceptional: requires re-creating the submodule pointer at the
  baseline tag, cherry-picking any post-import commits made on
  `clients/maekon-client/` back into `oneshim-client`, then deleting
  `clients/maekon-client/` and `tools/public-export/maekon-client/`.
- This path is expensive; the parallel-run validation in Steps 2 and 5 exists
  to make it unnecessary.

## 6. Risk Register

| Risk | Severity | Likelihood | Mitigation |
|------|----------|------------|------------|
| History loss in 3b | Medium | High if 3b chosen | Default to 3a unless user explicitly prefers clean diff |
| Stale `client-rust/` refs left in parent docs | Low | Medium | grep sweep + CI guard (e.g., a parent CI job that fails on `client-rust/` outside CHANGELOG) |
| Parent test regression after Step 3 | Medium | Low | Run `cargo check --workspace` and full test suite immediately after import |
| `tests/private/client-rust/` script breakage | Low | Medium | Update scripts in same PR as Step 4 |
| External tool refs break (Doppler, GitHub Actions) | Low | Very Low | `.github/workflows/` has 0 client-rust refs; verified |
| Public sync round regression at cut-over | Medium | Low | Parallel run for 1-2 rounds before deprecating old tool |
| User-flow disruption (developer git clone) | Low | Low | Submodule transition is internal; downstream consumers see only the new path |
| Lost work if rollback during Step 3 | Medium | Low | Step 1 baseline tag + dry-run export storage are explicit recovery points |
| GitHub Releases continuity for users on auto-update | Low | Very Low | maekon-client releases tracked separately; oneshim-client archive deferred to Phase 7 |

## 7. Open Decisions (User Required)

These decisions block specific steps. Decision points should be resolved
before starting the listed step.

1. **Step 3 import strategy** — 3a (subtree, history preserved), 3b
   (snapshot, clean), or 3c (filter-repo, full prefix rewrite). Default
   recommendation: **3a**.
2. **`tests/private/client-rust/` destination** — move to
   `clients/maekon-client/tests/private/` (co-locate with code) or rename to
   `tests/private/maekon-client/` (parent owns private QA). Default
   recommendation: **co-locate** under `clients/maekon-client/`.
3. **Maekon landing/docs location** (strategy Follow-up #5) — inside
   maekon-client repo, separate `pseudotop/maekon-landing` repo, or another
   deploy unit. Affects Step 6 surfaces.
4. **CONTRIBUTING.md content depth** — minimal (link to security policy + PR
   etiquette) vs. detailed (build instructions, testing, code style).
5. **Submodule `.gitmodules` cleanup** — remove `client-rust` entry entirely
   vs. leave a `# Removed in Phase 6 (YYYY-MM-DD)` historic comment.

## 8. Dependencies and Parallelism

```
Step 1 (pre-flight)
  └─ Step 2 (build tools/public-export/maekon-client) ────┐
  └─ Step 3 (import to clients/maekon-client) ────────────┤
       └─ Step 4 (path sweep) ─── can overlap with Step 6 │
            └─ Step 5 (cut over export tool) ←────────────┘
                 └─ Step 6 (Maekon language sweep)
                      └─ Step 7 (CONTRIBUTING.md + templates)
```

Step 2 can begin once Step 1's baseline is captured; it does not require
Step 3. Steps 6 and 7 can begin in parallel with Step 4 since they touch
different surfaces.

## 9. Estimated Effort

| Step | Estimate | Notes |
|------|----------|-------|
| 1 — Pre-flight | 30 min | Tag, capture baseline, dry-run export |
| 2 — Export tool port | 4–6 h | Includes smoke compare against baseline |
| 3 — Import | 1–2 h | Depends on chosen strategy; 3a is the longest |
| 4 — Path sweep | 2–4 h | 90 files, mostly mechanical |
| 5 — Cut-over | 1 h | Procedural after parallel-run verification |
| 6 — Maekon copy | 4–8 h | Content audit + writing |
| 7 — Templates | 2–3 h | CONTRIBUTING.md + check |
| **Total** | **~14–24 h** | Spread across multiple sessions |

## 10. Success Criteria

- Parent `oneshim` main ships `clients/maekon-client/` directory with no
  regression in CI or local builds.
- A sync round driven from `tools/public-export/maekon-client/export.sh`
  produces a `maekon-client` PR with the same shape as round 6 baseline
  (modulo intended overlay automation).
- All 90 parent `client-rust` references are either updated to
  `clients/maekon-client/` or intentionally archived in CHANGELOG / historic
  notes.
- `pseudotop/maekon-client` mirror continues to receive sync rounds with no
  manual ISSUE_TEMPLATE overlay step.
- `cargo check --workspace`, `cargo clippy --workspace`, full test suite
  green on parent main.
- User can `git clone pseudotop/oneshim` and run `cargo run -p oneshim-app`
  from `clients/maekon-client/` cleanly.
- Memory, playbook, and CLAUDE.md docs reflect the new structure.

## 11. References

- [`docs/plan/2026-04-30-maekon-client-public-oss-strategy.md`](./2026-04-30-maekon-client-public-oss-strategy.md) — strategy document this plan implements.
- [`docs/guides/public-repo-launch-playbook.md`](../guides/public-repo-launch-playbook.md) — current sync round procedure (will be updated in Step 5).
- `scripts/export-public-repo.sh` + `scripts/public-repo-exclude.txt` — current
  client-rust-side tooling that Option B replicates.
- 2026-05-04 sync round 5/6 PRs (`pseudotop/maekon-client#29`, `#30`) — last
  rounds run with the legacy tool; baseline for Step 2 smoke compare.
