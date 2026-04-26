# Branch Main-Reflection Audit

Date: 2026-04-27

Baseline: `origin/main` at `ed1d81a676ef1762c49a085b4069ddac9e85cd49` (`#523`).

## Method

- Fetched `origin --prune`.
- Compared local and remote refs against `origin/main` with ancestry checks and `git rev-list --cherry-pick`.
- Mapped branch names to the full GitHub PR list (`523` PRs).
- Checked related PR state, merge commits, and nearby commits for stale branches where direct diffs were misleading.

## Recovered

These branches had review/planning artifacts that were not present on `origin/main` and were safe to restore as add-only documentation:

- `origin/feature/d13-v2b-streaming-design`
  - `docs/reviews/2026-04-21-d13-v2b-streaming-design.md`
  - `docs/reviews/2026-04-21-d13-v2b-streaming-plan.md`
- `feature/phase9-autostart-linux-deep`
  - `.claude/pr-b2-loop-state.md`
  - `.claude/pr-b2-review/phase1-iter1-findings.md`
  - `docs/superpowers/plans/2026-04-25-phase9-pr-b2-autostart-linux-deep-plan.md`
  - `docs/superpowers/specs/2026-04-25-phase9-pr-b2-autostart-linux-deep-design.md`

## Already Reflected Or Superseded

The following branches still show unique commits when compared mechanically, but their PRs were merged by squash or the work was superseded by later merged PRs:

- `feature/external-grpc-audit-liveconfig` -> `#491`
- `feat/d13-v2c-external-grpc` -> `#484`
- `feature/phase9-autostart-foundation` -> `#508`
- `feature/sidebar-routing-and-error-middleware` -> `#376`
- `feature/phase9-tracking-schedule` -> `#487`
- `fix/review-polish-v2` -> `#410`, `#412`
- `feature/d13-v2b-pr-b2-subscribe-metrics` -> `#479`
- `feat/d13-task13-full-wiring` -> `#486`
- `feature/grpc-stress-test-suite` -> `#488`
- `fix/phase9-pr-a-followup-cleanup` -> `#489`
- `fix/wal-checkpoint-shutdown` -> `#426`
- `features` -> `#267`
- `fix/overlay-polish` -> `#387`, `#395`
- `fix/dmg-background-clean` -> `#389`
- `refactor/ia-ux-cleanup` -> `#380`
- `chore/dmg-background-remove` -> `#385`
- `feat/d13-v2c-drop-accumulator-reason-split` -> `#483`
- `feat/audit-storage-fall-through` -> `#502`
- `fix/notarize-head-branch-dispatched-parent` -> `#447`
- `release/v0.4.32-rc.4` -> `#386`
- `fix/d13-v2c-polish-followup` -> `#485`
- `test/chat-session-e2e` -> `#383`
- `fix/updater-boot-count-per-pid-markers` -> `#449`
- `refactor/signature-public-key-empty-default` -> `#448`
- `feature/d5-iter16-migrate-gui-pipeline` -> `#460`
- `pr-512` -> `#512` (`refactor/timewindow-primitive`)

## Closed Or Stale Branches

These branches should not be merged directly. They are either closed release candidates, superseded by later merged work, or too divergent from current `main`:

- `feature/d13-v2-config-grpc-port` (`#463`) was closed after `#523` integrated the effective `grpc_port` support.
- `refactor/split-app-runtime-launch` (`#471`) was closed after `#523` integrated the effective health-probe split.
- `fix/review-polish` (`#409`) was replaced by merged `fix/review-polish-v2` (`#410`).
- `feat/analysis-wiring` (`#320`) was closed after the relevant analysis/coaching work landed through earlier merged PRs; its remaining branch-tip doc sync is stale.
- `feat/p4-voice-activity-detection` (`#287`) is a closed release branch; VAD and later AudioTab work are present through merged PRs such as `#286` and follow-up audio work.
- `feature/phase11-platform-parity` (`#366`) is a closed release branch; later autostart/platform work landed through `#508` and related platform fixes.
- `release/v0.4.17-rc.1` (`#302`) is a closed release candidate and should not be replayed.
- `promote-stable` (`#421`) has a small follow-up panic/unwrap cleanup commit, but equivalent cleanup is covered by later merged stability work (`#422`).
- `feature/phase9-quick-wins` has a post-merge docs commit, but those Phase 9 review documents are already present on `origin/main`.

## Needs Separate Rebase Review

- `codex/public-export-audit-20260426` has no PR and contains a broad public-export/Maekon cleanup commit over an older base. It touches docs, Cargo metadata, public export scripts, and runtime files. Direct replay would collide with `#523` and current `main`; this should be rebased and reviewed as a separate PR instead of merged wholesale.
