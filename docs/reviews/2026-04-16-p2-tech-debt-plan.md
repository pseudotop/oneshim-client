# P2 Tech-Debt — Implementation Plan

_Plan version: 1.0 (Loop 2 output, based on spec v1.1)_
_Spec: `.claude/plans/p2-tech-debt-spec.md`_
_Brief: `.claude/plans/p2-tech-debt-brief.md`_

## Open Question Resolutions (applied into plan)

- **Q1 — Nursery commit granularity**: Per `(lint, crate)` commit, ≤5-violation groups merged into one commit, ≤15 commits total, split across 3–4 PRs (one PR per lint, or lint-1 solo + lint-2-3-4 bundled). Decided: **3 PRs total**. See Item 1 for the exact split.
- **Q2 — Triage doc location**: **In-repo `docs/reviews/YYYY-MM-DD-large-files-triage.md`** (primary). Engineers who browse the repo should find it. Memory stays for agent reasoning context only.
- **Q3 — Sequencing**: **2 → 3 → 1** (cheapest to most expensive). Sequenced, not parallel, because Item 1 commits can surface regressions that benefit from clean baseline state.

## Global Execution Order

1. **Item 2 — windows-sys documentation** (30 min)
2. **Item 3 — Large-files triage document** (4 hours)
3. **Item 1 — Nursery lint hardening** (~6 dev-days across 3 PRs)

Sequenced so Items 2 and 3 finish same-day, and Item 1 owns its own multi-PR rhythm.

---

## Item 2 — windows-sys Version Consolidation (Documentation Only)

### Files touched
- `/Volumes/ext-PCIe4-1TB/bjsmacminim4_ext/.claude/projects/-Volumes-ext-PCIe4-1TB-bjsmacminim4-ext-Documents-vscode---INDIVISUAL---oneshim--git-modules-oneshim-agent-client-rust/memory/reference_dep_constraints.md` (update existing memory file)
- Optionally: `docs/reviews/YYYY-MM-DD-dep-constraints-windows-sys.md` if the team prefers in-repo (deferred by default — decision by user before execution)

### Steps
1. Open the memory file and extend the existing content:
   ```
   ## windows-sys version spread (as of 2026-04-16)
   5 transitive versions: 0.45.0 / 0.52.0 / 0.59.0 / 0.60.2 / 0.61.2.

   Root causes:
   - 0.45.0: jni → tao → tauri-runtime-wry → tauri; also jni → cpal → oneshim-audio
   - 0.52.0: ring/rustls (via reqwest), self-replace
   - 0.59.0: indicatif/console (fastembed chain), global-hotkey, rustix 0.38 (xcap chain), window-vibrancy
   - 0.60.2: hf-hub, keyring 3.x, muda (Tauri menu)
   - 0.61.2: majority (mio, tokio, socket2, reqwest, etc.)

   Action: NONE. No workspace pin or [patch.crates-io] is viable without forking.
   Dependabot's cargo-minor-patch group already pulls upstream bumps.

   Re-audit triggers:
   - `cargo tree` shows ≥7 versions of windows-sys, OR
   - compile time regresses ≥30s from baseline, OR
   - quarterly (2026-07).
   ```

2. Verify the memory file loads correctly (`head -15 memory/reference_dep_constraints.md`).

3. Commit is NOT required — memory lives outside the repo.

### Validation
- `cargo tree --target all 2>/dev/null | grep -oE "windows-sys v[0-9.]+" | sort -u | wc -l` returns `5` (unchanged sanity check).

### Rollback
- Restore the previous memory file content from git if versioned, or simply re-edit.

### Risk
- Zero code risk. Pure documentation.

### Acceptance verification
- AC2.1 ✅ when memory file contains the above block.
- AC2.2 ✅ when the "Re-audit triggers" section is present.
- AC2.3 ✅ `git diff Cargo.toml Cargo.lock` returns no changes.

---

## Item 3 — Large Files Triage Document (Decision-only)

### Files touched
- `docs/reviews/YYYY-MM-DD-large-files-triage.md` (new)

### Steps

1. **must-split candidates — verdicts already confirmed during Loop 2 spec review**. The triage doc should cite these findings directly (see spec Item 3 table):
   - `crates/oneshim-vision/src/privacy.rs`: 4 responsibilities (detection / masking / app-policy / title-sanitization). Extraction candidates: `privacy/detection.rs`, `privacy/masking.rs`, `privacy/app_policy.rs`.
   - `crates/oneshim-web/frontend/src/pages/hooks/useSettingsForm.ts`: 5 state domains managed in one hook. Extraction candidates: `useSettingsFormState.ts`, `useSettingsExport.ts`, `useModelCatalog.ts`, `useAiProviderProfiles.ts`.

2. **Spot-check the 4 Rust `maybe-split` candidates** (1-line verdict for each, short rationale):
   - `src-tauri/src/updater/mod.rs` (1404 LOC) — test phase-boundary hypothesis by reading fn names.
   - `crates/oneshim-storage/src/frame_storage.rs` (1167 LOC) — test buffer-pool vs retention separation.
   - `crates/oneshim-analysis/src/adaptive_search.rs` (1166 LOC) — test strategy-per-struct hypothesis.
   - `crates/oneshim-network/src/local_llm_session.rs` (1002 LOC) — test session-vs-protocol separation.

3. **Spot-check the 5 frontend `maybe-split` candidates** similarly.

4. **Write `docs/reviews/YYYY-MM-DD-large-files-triage.md`** with:
   - Intro paragraph (why: triage not a split plan).
   - Rust table (10 files, final verdicts after step 1-2).
   - Frontend table (11 files, final verdicts after step 3).
   - "Follow-up tickets" section: for each `must-split`, a paragraph citing the SOLID violation and estimated effort to actually split (not done here).
   - "Explicit non-actions" section: listing `keep` files to stop future "why haven't we split X?" conversations.

5. **No code changes.**

### Validation
- `test -f docs/reviews/*-large-files-triage.md && head -5 docs/reviews/*-large-files-triage.md` returns the first 5 lines.
- `grep -c "^|" docs/reviews/*-large-files-triage.md` ≥ `21` (10 Rust + 11 frontend rows = 21 data rows minimum).

### Rollback
- `git revert` on the doc-only commit.

### Risk
- Very low. Documentation only. Risk of opinion drift — mitigate by citing specific fn names and coupling patterns in the doc.

### Effort estimate
- Step 1 (read 2 must-split files): 1 hour
- Step 2 + 3 (spot-check 9 maybe-split files): 1.5 hours
- Step 4 (write doc): 1 hour
- Step 5 (buffer): 30 min
- **Total: ~4 hours**

### Acceptance verification
- AC3.1 ✅ doc exists at the listed path.
- AC3.2 ✅ every file marked `must-split` has a follow-up-ticket paragraph.
- AC3.3 ✅ every `maybe-split` has a one-line inspect-before-deciding note.
- AC3.4 ✅ `keep` files are listed with one-line reasoning.

---

## Item 1 — Nursery Lint Hardening (Multi-PR)

### PR structure (3 PRs, ≤15 commits total)

| PR | Branch | Lints | Crates touched | Commits | Risk |
|----|--------|-------|----------------|---------|------|
| **PR-A** | `p2-lint-drop-tightening` | `clippy::significant_drop_tightening` | all crates with ≥1 hit (focus on `oneshim-storage`, `oneshim-network`, `oneshim-web`) | ≤8, one per crate with >2 hits; smaller crates grouped into "misc" commits | Moderate — each fix rewrites lock scope. Needs `cargo test -p <crate>` + manual review per commit. |
| **PR-B** | `p2-lint-clones-and-options` | `clippy::redundant_clone` + `clippy::option_if_let_else` | all affected crates | ≤5 combined; lints bundled per crate (one commit fixes both lints in that crate) to avoid 2× commit explosion | Low — mechanical + readability. Some `#[allow]` may be needed for Arc false positives. |
| **PR-C** | `p2-lint-missing-const` | `clippy::missing_const_for_fn` | all affected crates | ≤2, grouped across crates since fixes are trivial and co-located | Low — mechanical. |

**Commit granularity note**: the "per (lint, crate)" rule from the spec is relaxed inside a PR when the fixes are small and mechanical — we bundle to respect the ≤15-commit ceiling. When a single crate has >5 changes on one lint, split it out so review chunks stay readable.

### Steps (executed per PR)

**Setup (once per PR branch; branch names from the table above)**:
```bash
git checkout -b <branch_name> origin/main
```

**Per-lint, per-crate workflow**:
1. Add `#![deny(clippy::<lint_name>)]` to the top of `crates/<crate>/src/lib.rs` (or `main.rs`). Do NOT add to `src-tauri/src/main.rs` for Lint 1 (drop tightening) yet — src-tauri is the binary crate, do it last as a follow-up commit.
2. Run `cargo clippy -p <crate> --all-targets -- -D warnings`.
3. Fix each violation:
   - Lint 1 (drop tightening): introduce `drop(guard)` explicitly or scope the guard with `{ … }`. Verify no lock-order inversion is introduced.
   - Lint 2 (redundant_clone): remove the clone OR annotate `#[allow(clippy::redundant_clone)]` with `// Arc clone is intentional: cheap, avoids lifetime binding` if it's an Arc.
   - Lint 3 (option_if_let_else): `.map_or(default, f)` or `.map_or_else(|| default, f)`.
   - Lint 4 (missing_const_for_fn): prepend `const` to the fn signature.
4. Run `cargo test -p <crate>` — must pass.
5. Run `cargo clippy --workspace --all-targets -- -D warnings` — must pass (base gate).
6. Commit: `lint(<crate>): deny clippy::<lint_name>` with a message listing the specific hotspots fixed.

**PR creation**:
```bash
git push -u origin <branch_name>
gh pr create --base main --title "refactor: deny clippy::<lint_name>" --body "<summary + changelog>"
```

**Merge gate**: each PR must pass CI independently. Do NOT stack PRs; sequential merges only.

### Lint-specific detailed steps

#### Lint 1 — `clippy::significant_drop_tightening` (PR-A)

Expected affected files (from raw grep distribution):
- `crates/oneshim-storage/src/sqlite/**` — likely scope-held rusqlite handles.
- `crates/oneshim-network/src/sync/lan_transport/mod.rs` — RwLock scopes around `verified_peers`.
- `crates/oneshim-network/src/sync/remote_transport.rs` — less likely (reqwest is async).
- `crates/oneshim-web/src/app_state.rs` or handlers — Mutex scopes.

**Per-file procedure**:
1. Read the flagged code region.
2. Identify whether the guard is held across an `.await` (always an issue) or across a computation that doesn't need the lock.
3. Apply the narrower scope — either explicit `drop(guard)` before the non-critical work, or inner `{}` block.
4. Run `cargo test -p <crate> --lib` — must pass.
5. If the code holds multiple locks, verify lock ordering remains consistent.

**Rollback**: per-commit revert; each fix is independent.

#### Lint 2 — `clippy::redundant_clone` (part of PR-B)

**Per-site procedure**:
1. Check if the clone target is `Arc<T>` / `Rc<T>`. If yes → `#[allow]` with inline comment.
2. Otherwise, remove the clone. If compile fails due to move, add `.as_ref()` or restructure the borrow.
3. Run `cargo test -p <crate>`.

#### Lint 3 — `clippy::option_if_let_else` (part of PR-B)

**Per-site procedure**:
1. Replace `if let Some(x) = opt { f(x) } else { default }` with `opt.map_or(default, f)`.
2. If the closure captures mutable state or has side effects, use `map_or_else(|| default_expr, |x| f(x))` to preserve lazy evaluation.

#### Lint 4 — `clippy::missing_const_for_fn` (PR-C)

**Per-site procedure**:
1. Prepend `const` to the fn signature.
2. If compile fails with "const fn cannot use …", the lint was a false positive for this fn — add `#[allow(clippy::missing_const_for_fn)]` with a comment.
3. No test impact expected.

### Validation (per PR + cumulative)

**Per PR**:
```bash
cargo fmt --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
# lan-sync feature matrix
cargo test -p oneshim-network --features lan-sync --lib
cargo clippy -p oneshim-network --features lan-sync --all-targets -- -D warnings
```

**After all 3 PRs merged**:
- `cargo clippy --workspace --all-targets -- -W clippy::nursery 2>&1 | grep -oE "clippy::[a-z_]+" | sort | uniq -c | sort -rn` — confirm target lints are absent from the top.
- Document result in `docs/reviews/YYYY-MM-DD-nursery-lint-hardening.md` as prescribed by AC1.4.

### Rollback

**Per PR**: if CI fails after merge, `git revert <pr_merge_sha>` on main. Each PR is standalone.

**Within a PR**: per-commit revert; each `(lint, crate)` fix is independent.

### Risk mitigations

- **Lock-order inversion**: grep for other `lock()` calls in the same file after each Lint-1 fix; verify the new drop timing doesn't enable a re-entrancy or reorder scenario.
- **Arc FP**: Lint-2 Arc clones should use `#[allow]` with comment rather than rewrites — avoids over-engineering.
- **Const FP**: Lint-4 fns that the compiler rejects as const should stay as-is with `#[allow]`.

### Effort tracking

- PR-A (drop tightening, ~16 warnings): **3 days** (0.5-1 hour per fix × 16, plus review overhead).
- PR-B (redundant_clone + option_if_let_else, ~22 warnings): **2 days** combined.
- PR-C (missing_const_for_fn, ~16 warnings): **1 day**.
- **Total: 6 dev-days spread over 2 weeks (one PR every 3–5 days, allowing review time).**

### Acceptance verification

- AC1.1 ✅ at the end of PR-C — at least 3 of 4 target lints are `deny`d on ≥1 crate.
- AC1.2 ✅ final CI green on main after PR-C.
- AC1.3 ✅ `cargo test --workspace` green on main.
- AC1.4 ✅ `docs/reviews/YYYY-MM-DD-nursery-lint-hardening.md` exists and lists the 4 targeted lints + the 3 deprioritized ones with reasoning.

---

## Cross-cutting

### Validation gates (final, after all 3 items complete)

```bash
cargo fmt --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p oneshim-network --features lan-sync --lib
```

### Rollback strategy (cross-item)

Each item is self-contained:
- Item 2: memory edit, trivially reversible.
- Item 3: single doc commit; revert if needed.
- Item 1: 3 independent PRs; revert any subset.

### Sequencing dependencies

- Items 2 and 3 are fully independent (both documentation).
- Item 1 (PR-A / PR-B / PR-C) must be sequential — PR-A's lock changes can affect how Lint 2/3 diagnostics appear on the same lines.

### Commit message convention (git-cliff-visible)

- Item 1: `refactor(<crate>): deny clippy::<lint>` (per commit).
- Item 1 doc: `docs(reviews): add nursery lint hardening report`.
- Item 3 doc: `docs(reviews): add large-files triage`.
- Item 2 (memory edit): no commit — memory file is outside the repo.

---

## Definition of Done (all 3 items)

- [ ] Item 2: `reference_dep_constraints.md` memory updated per AC2.1-2.3.
- [ ] Item 3: `docs/reviews/YYYY-MM-DD-large-files-triage.md` merged; all 21+ files triaged.
- [ ] Item 1: PR-A, PR-B, PR-C all merged to main.
- [ ] Final validation gates all green.
- [ ] `docs/reviews/YYYY-MM-DD-nursery-lint-hardening.md` exists.

## Out of scope (recap)

- Actually splitting any large file (Item 3 is triage only).
- Enabling the full `clippy::nursery` group (Item 1 is selective).
- Removing transitive `windows-sys` versions (Item 2 is document-only).
- `clippy::pedantic`, `suboptimal_flops`, `use_self`, `redundant_pub_crate`, `derive_partial_eq_without_eq` (explicitly deprioritized).

## Completion promise (for Ralph loop)

Output `<promise>RALPH_P2_SPEC_PLAN_DONE</promise>` only after BOTH spec and this plan have passed deep review with zero minor+ issues.
