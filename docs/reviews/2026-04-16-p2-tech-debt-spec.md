# P2 Tech-Debt — Specification

_Spec version: 1.1 (Loop 1 output, post-deep-review)_
_Brief: `.claude/plans/p2-tech-debt-brief.md`_

## At a glance

| Item | Effort | Risk | Action |
|------|--------|------|--------|
| 1. Nursery lint hardening (4 targeted lints) | **~6 dev-days** | Moderate (drop tightening) | Fix per (lint, crate) across ≤15 commits / 3–4 PRs. |
| 2. windows-sys consolidation | **~30 min** | None | Document; no code action. |
| 3. Large files triage | **~4 hr** | None | Write triage doc; no splits in scope. |

**Grand total effort**: ~6 dev-days + 4.5 hours. All items are independently sequenceable; recommended order 2 → 3 → 1.

## Ground Truth Snapshot (2026-04-16)

Values measured on this worktree, branch `fix/pr422-followups` (post-#422 / pre-#423-merge):

| Item | Memory (2026-04-07) | Actual (2026-04-16) | Drift |
|------|---------------------|---------------------|-------|
| Nursery lint hits | "46" | **~500 distinct warnings** across ~20 categories (~1620 raw grep hits when counting lint-name mentions twice per warning header + help text). | Memory was likely counting a narrow filtered subset; actual surface is order-of-magnitude larger. |
| windows-sys versions | 5 | 5 (0.45.0 / 0.52.0 / 0.59.0 / 0.60.2 / 0.61.2) | Unchanged. |
| Large frontend files | 8 (>500 LOC) | 11 (>500 LOC) | +3 files since memory. |
| Large Rust src (non-test) | not tracked | 102 files (>500 LOC) | New surface. |

## Item 1 — Nursery Lint Hardening (selective)

### Goal
Harden the codebase against the 3–5 highest-signal nursery lints by opting in per-crate, fixing violations, and documenting the rest as accepted noise. Not a full nursery adoption.

### Target lints (ranked by ROI)

| # | Lint | Warnings | Why it matters | Risk / caveat |
|---|------|----------|----------------|---------------|
| 1 | `clippy::significant_drop_tightening` | ~16 | Scope-held mutex guards can cause subtle deadlocks and latency spikes under contention. | Real runtime bug surface in lock-heavy code (sync, storage). **Fixes can change lock release timing → always re-run `cargo test` in affected crate.** |
| 2 | `clippy::redundant_clone` | ~10 | Unnecessary heap allocations; concrete perf wins. | **False positives on `Arc<T>` / `Rc<T>` are known — case-by-case judgment required.** Use `#[allow]` locally when the clone is intentional. |
| 3 | `clippy::option_if_let_else` | ~12 | Readability; `.map_or` / `.map_or_else` idiom consistency. | Pure rewrite; low risk. |
| 4 | `clippy::missing_const_for_fn` | ~16 | Functions that could be `const fn` for compile-time evaluation. | Adding `const` can change trait-bound behaviour in rare cases; mechanical fix. |

**Deprioritized** (was considered, now out of scope):
- `clippy::use_self` (~13 warnings) — cosmetic-only; consider for a future pass once higher-ROI lints are shipped.
- `clippy::suboptimal_flops` (~7 warnings) — float reordering changes numerical output; needs case-by-case care beyond this scope.
- `clippy::redundant_pub_crate`, `derive_partial_eq_without_eq` — cosmetic.

### Scope (per lint)
- Enable the lint as `#![deny(clippy::<lint>)]` at the top of each affected crate (not workspace-wide).
- Fix all violations in that crate.
- Re-run `cargo clippy --workspace --all-targets -- -D warnings` to confirm the base gate stays green.
- **Commit granularity**: one commit per `(lint, crate)` pair so bisect remains useful without exploding the PR log. Group crates with ≤5 violations of a lint into a single commit when they all fix cleanly. Target ≤15 commits across Item 1.

### Non-goals
- Enabling the entire `clippy::nursery` group.
- The deprioritized lints listed above (cosmetic-only + `suboptimal_flops`).

### Lint-by-lint distribution (by crate)
Top nursery-heavy crates: `oneshim-web` (266), `oneshim-storage` (258), `oneshim-core` (222), `oneshim-analysis` (175), `oneshim-network` (151). Start with `oneshim-storage` for lint #1 (drop tightening) since it has the most lock-bearing code.

### Acceptance criteria
- AC1.1 At least 3 of the 4 target nursery lints promoted to `deny` on ≥1 crate each.
- AC1.2 `cargo clippy --workspace --all-targets -- -D warnings` remains green.
- AC1.3 `cargo test --workspace` remains green (3370+ tests, 0 failures).
- AC1.4 A short doc at `docs/reviews/YYYY-MM-DD-nursery-lint-hardening.md` (date TBD at execution time) lists which lints were enabled, which crates, and why the deprioritized ones were excluded.

### Risks
- **Drop-tightening rewrites can change behaviour** (releasing a lock earlier may expose races). Each fix needs a manual review beyond "does it compile". Mitigation: `cargo test --workspace` must pass after each commit; flaky tests tolerated for 3 re-runs before treating as regression.
- **redundant_clone false positives** on `Arc<T>` / `Rc<T>` — case-by-case; use local `#[allow(clippy::redundant_clone)]` with an inline comment when the clone is intentional.
- **Commit churn**: ≤15 commits spread over several PRs prevents a single giant "fix nursery" PR that would be hard to review.

### Effort estimate
- Lint 1 (drop tightening): **3 developer-days** — highest risk, needs lock-order review per fix.
- Lint 2 (redundant_clone): **1 day** — mostly mechanical.
- Lint 3 (option_if_let_else): **1 day** — pure rewrite.
- Lint 4 (missing_const_for_fn): **1 day** — mechanical.
- **Total: 6 developer-days** spread across ≤15 commits in 3–4 PRs.

## Item 2 — windows-sys Version Consolidation

### Investigation result (already completed in Loop 1)

Each version is pulled transitively. **None is a direct workspace dependency.** Callers:

| Version | Upstream chain (primary) | Can workspace influence? |
|---------|--------------------------|--------------------------|
| 0.45.0 | `jni@0.21 → tao → tauri-runtime-wry → tauri` (+ `cpal → oneshim-audio`) | No — jni/tao are heavy Tauri deps. |
| 0.52.0 | `ring/rustls` (via `reqwest`), `self-replace` | No — mature crypto/self-update stack; upstream rarely bumps. |
| 0.59.0 | `indicatif`, `global-hotkey`, `rustix 0.38` (via drm/gbm/xcap), `window-vibrancy` | No — Tauri + screen-capture ecosystem. |
| 0.60.2 | `hf-hub`, `keyring 3.x`, `muda` (Tauri menus) | No — downstream of Tauri/HF/keyring. |
| 0.61.2 | Everything else (mio, tokio, socket2, reqwest, etc.) | Latest; already dominant. |

### Goal
Document the finding. Decide: **keep as-is.** Set a monthly check cadence (existing `dependabot` already covers this).

### Scope
- Update the existing `reference_dep_constraints.md` memory (primary location) with:
  - 5 versions are transitive via ~10 independent upstream chains (listed in the table above).
  - No workspace action can consolidate without forking upstream.
  - Dependabot grouping (`cargo-minor-patch`) already covers upstream bumps.
  - Re-audit cadence: quarterly, or when `cargo tree` shows ≥7 versions (threshold for action).
- Optionally add a short in-repo note in `docs/reviews/` if the project prefers in-repo docs over memory for this topic (decided in Loop 2, Q2).

### Non-goals
- `[patch.crates-io]` overrides (fragile, breaks on upstream updates).
- Forking or patching upstream crates.
- Disabling specific Tauri features to drop transitive deps (blast radius too large).

### Acceptance criteria
- AC2.1 Memory `reference_dep_constraints.md` updated to include the 5-version breakdown and the re-audit threshold.
- AC2.2 A documented re-audit threshold (≥7 versions) and cadence (quarterly).
- AC2.3 No Cargo.toml edits required.

### Effort estimate
~30 minutes (memory edit only; no repo changes).

## Item 3 — Large Files Triage

### Rust source — top 10 non-test, SOLID-sniff triage

| File | LOC | fns | structs | impls | Verdict | Reasoning |
|------|-----|-----|---------|-------|---------|-----------|
| `src-tauri/src/updater/mod.rs` | 1404 | 55 | 4 | 1 | **maybe-split** | Likely has download + extract + apply + verify phases; 4 structs hint at natural boundaries. Investigate before committing. |
| `crates/oneshim-storage/src/sqlite/maintenance.rs` | 1401 | 56 | 0 | 1 | **keep** | 56 free functions on one concern (retention/maintenance) — single-responsibility within the storage adapter. Splitting would pass SOLID check but add indirection. |
| `crates/oneshim-vision/src/privacy.rs` | 1276 | 80 | 1 | 1 | **must-split (confirmed)** | Confirmed during Loop 2 review: the file holds 4 distinct responsibilities — (a) PII detection (`detect_pii_markers_*`), (b) PII masking (`mask_emails`, `mask_credit_cards`, `mask_ip_addresses`, `mask_user_paths`, ~12 mask functions), (c) app-level exclusion policy (`is_sensitive_app`, `should_exclude`, `matches_exclusion_pattern`), (d) title sanitization (`sanitize_title_with_level`). Classic SRP violation; candidates to extract: `privacy/detection.rs`, `privacy/masking.rs`, `privacy/app_policy.rs`, keeping `VisionPiiSanitizer` as the thin trait impl. |
| `crates/oneshim-storage/src/frame_storage.rs` | 1167 | 52 | 5 | 4 | **maybe-split** | 5 structs across 4 impls — buffer pool + retention + I/O. Needs inspection of coupling before deciding. |
| `crates/oneshim-analysis/src/adaptive_search.rs` | 1166 | 81 | 7 | 12 | **maybe-split** | 7 structs / 12 impls suggests multiple coordinators. Worth looking at whether to extract `IvfSearchStrategy`, `BruteForceStrategy`, `HnswStrategy` into sibling files. |
| `crates/oneshim-analysis/src/coaching_engine/mod.rs` | 1151 | 45 | 1 | 2 | **keep** | Already under the directory-module pattern (`coaching_engine/`). The 1151 LOC is the aggregator of submodules (`guards.rs`, `triggers.rs`). Per feedback_file_split_policy: splitting further would be over-engineering. |
| `crates/oneshim-network/src/local_llm_session.rs` | 1002 | 33 | 3 | 2 | **maybe-split** | Likely handles session lifecycle + request/response; 3 structs hint at split candidates. |
| `crates/oneshim-vision/src/accessibility/windows.rs` | 997 | 26 | 3 | 4 | **keep** | Already gated by `#[cfg(target_os = "windows")]`. Windows UIA requires a lot of ceremony (COM init, cache requests). SOLID within the platform boundary. |
| `crates/oneshim-storage/src/sqlite/web_storage_impl.rs` | 993 | 70 | 0 | 14 | **keep** | Deliberate umbrella: 14 impl blocks each satisfying a narrow sub-trait of `WebStorage`. Splitting breaks the "one blanket-impl site" convention. |
| `crates/oneshim-core/src/models/intent.rs` | 974 | 36 | 8 | 2 | **keep** | Pure model definitions (DTOs + helpers). 8 structs = different aggregates. Splitting would fragment the domain vocabulary. |

**Summary**: 1 must-split (`privacy.rs`), 4 maybe-split (require investigation), 5 keep.

### Frontend — 11 files >500 LOC, triage

| File | LOC | Verdict | Reasoning |
|------|-----|---------|-----------|
| `api/contracts.ts` | 1724 | **keep** | Shared TypeScript contracts — single source of truth. Splitting would scatter types. |
| `api/client.ts` | 1235 | **maybe-split** | HTTP client — likely has resource-per-area methods. Candidate for `client/{domain}.ts` split. |
| `api/standalone.ts` | 1219 | **maybe-split** | Duplicates parts of `client.ts`. Investigate dedup opportunity. |
| `hooks/useSettingsForm.ts` | 984 | **must-split (confirmed)** | Confirmed during Loop 2 review: single `useSettingsForm` hook (line 127) managing 5 distinct state domains — form-data lifecycle, export UI state, model-catalog fetching/caching, AI-provider profile CRUD, settings load/sanitization. Candidates: `useSettingsFormState.ts` (form lifecycle), `useSettingsExport.ts`, `useModelCatalog.ts`, `useAiProviderProfiles.ts`, with the root hook composing them. |
| `pages/setting-tabs/ai-automation/index.tsx` | 758 | **maybe-split** | Tab page — candidate for subsection components. |
| `pages/Onboarding.tsx` | 607 | **maybe-split** | Multi-step flow — candidate for per-step components. |
| `pages/timeline/AllFrames.tsx` | 606 | **keep** | Likely a single list view with local state — line count alone doesn't justify split. |
| `pages/chat/index.tsx` | 589 | **keep** | Single chat surface — SRP intact. |
| `stories/mock-data.ts` | 566 | **keep (fixture)** | Test / Storybook fixtures; line count irrelevant for data files. |
| `components/BugReportWizard.tsx` | 541 | **maybe-split** | Wizard component — step-per-file candidate. |
| `pages/setting-tabs/GeneralTab.tsx` | 508 | **keep** | Just over threshold; single tab. |

**Summary**: 1 must-split (`useSettingsForm.ts`), 5 maybe-split, 5 keep.

### Goal
Produce a **triaged list** (this table) so future work can draw from it deterministically. No refactors prescribed here.

### Acceptance criteria
- AC3.1 The triage tables above are written into `docs/reviews/YYYY-MM-DD-large-files-triage.md` (date TBD at execution time).
- AC3.2 The two `must-split (confirmed)` files carry a written rationale (done — see rows above).
- AC3.3 Each `maybe-split` file has a 1-line "inspect-before-deciding" note stating the specific hypothesis (e.g., "download/extract/apply phases appear separable"). These remain hypotheses — spot-check during execution.
- AC3.4 `keep` files don't need action but are listed for transparency.

### Non-goals
- Actually splitting any file.
- Producing per-file refactor plans.
- Rewriting affected tests.

### Effort estimate
~4 hours to formalize the triage doc with citations + inspection notes for each maybe-split candidate.

## Cross-cutting

### Validation (end of plan execution, not this spec)
- `cargo check --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo fmt --check`

### Risks (overall)
- **Scope creep**: "We'll split this one file" can cascade into a multi-week refactor. The plan must enforce strict file-by-file commits with green tests between each.
- **Reviewer fatigue**: Large-file splits produce huge diffs. Consider rebase to small atomic commits.

## Deferred (out of scope)
- Full frontend codegen: `api/contracts.ts` auto-generation from OpenAPI (would solve the 1724-LOC file but is a separate initiative).
- Deep `updater/mod.rs` rewrite via Tauri updater v2 plugin (separate initiative, requires GA readiness audit).
- `clippy::pedantic` adoption (explicitly out of scope for P2).

## Open Questions (to resolve in Loop 2)
- Q1: Should Item 1 (nursery lints) be done per-crate one-at-a-time or in a big-bang PR? (PR size vs risk isolation trade-off.)
- Q2: Should Item 3 (triage) live in `docs/reviews/` or in the memory system? (In-repo docs survive; memory is agent-only.)
- Q3: Sequencing: do all three items in parallel, or strictly in order? Recommendation: Item 2 (30 min) first, then 3 (4 hr), then 1 (~2 weeks).
