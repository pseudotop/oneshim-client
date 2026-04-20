[English](./STATUS.md) | [한국어](./STATUS.ko.md)

# Project Status Snapshot

This document is a curated snapshot of mutable quality signals and workflow references.

## Scope

Track the following here and link this file from other docs instead of duplicating mutable values:

- Latest full CI workflow status and link
- Latest release workflow status and link
- Local verification baseline for current branch work
- Known flaky or quarantined tests

## Update Policy

Update this document whenever workflow status, verification baseline, or known flake status changes.
GitHub Actions run pages remain the authoritative live source for workflow state.

Recommended verification commands:

```bash
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd crates/oneshim-web/frontend && pnpm lint && pnpm build-storybook
```

## Current Snapshot (2026-04-20)

### Version

v0.4.39-rc.1 (Phase 5-D8 complete; Phase 4 Updater Hardening shipped)

### Workspace

- **Packages**: 15 per `cargo metadata --no-deps` (14 crates under `crates/` including `oneshim-sandbox-worker` + `src-tauri`; planned `oneshim-ui` and former `crates/oneshim-app/` removed by ADR-004 Tauri migration)
- **SQLite schema**: V31 (V31 added by Phase 3 for `regime_manager_state`)

### Workflow Status (as of 2026-04-20 spot-check)

- **Latest `main` CI run** (`fix(updater): per-PID boot_count markers ...`): failure — [Run 24623392589](https://github.com/pseudotop/oneshim-client/actions/runs/24623392589) (2026-04-19). Root cause: cross-platform `Build` jobs try to download the `frontend-dist-bundle` artifact via `ARTIFACT-DL`, but the `Frontend (build + e2e)` job is `skipped` on main pushes (only runs on frontend-touched PRs). Known CI-config drift; Rust test surface is green. Tracked outside this branch.
- **Latest PR-context CI success**: success — [Run 24622597161](https://github.com/pseudotop/oneshim-client/actions/runs/24622597161) (2026-04-19, branch `fix/updater-boot-count-per-pid-markers`). PR CI runs pass reliably — the artifact gap is main-push-only.
- **Latest Release RC**: success (`Release`, `v0.4.38-rc.4`) — [Run 24570428239](https://github.com/pseudotop/oneshim-client/actions/runs/24570428239) (2026-04-17). Preceded by v0.4.38-rc.3 failure for different root cause.
- **Most recent stable tag**: v0.4.37 (2026-04-12). Current branch targets v0.4.39-rc.1 → stable via `promote-stable.sh` after ADR-019 PR lands.

### Local Verification Baseline

- `cargo check --workspace`: pass
- `cargo clippy --workspace --all-targets -- -D warnings`: pass
- `cargo test --workspace`: pass — **3,651 passed, 0 failed, 21 ignored** (post-ADR-019 + drift-audit + Follow-up baseline. Cumulative growth from prior 3,455: ADR-019 Error Code Infrastructure + C5 Bedrock skip + post-merge drift audit iter 87~214 added ~196 tests — 85+ HTTP status-mapping regression tests across 15 dispatchers, ~38 Internal→specific-variant re-route tests, 4 subprocess_kind (iter-149) + 3 LLM envelope-extraction (iter-151) + 10 `IpcError` contract tests (iter-196 Follow-up #1) + other targeted regression guards. Phase 2 telemetry tests run separately via `--features telemetry -- --test-threads=1`. **Not counted in 3,651**: 6 `map_challenge_status_to_error` tests (iter-195 Follow-up #5) are feature-gated behind `lan-sync` — run `cargo test -p oneshim-network --features lan-sync --lib sync::lan_transport::auth` to include them.)
- `cargo fmt --check`: pass
- `pnpm lint` (`crates/oneshim-web/frontend`): pass
- `pnpm build-storybook` (`crates/oneshim-web/frontend`): pass

### Phase 2 Telemetry Feature (added 2026-04-17)

- `cargo test -p oneshim-app --features telemetry -- --test-threads=1`: pass — **10 passed** (T-X2-1 is default-build-only and runs in the workspace suite above).
- `cargo clippy -p oneshim-app --features telemetry --all-targets -- -D warnings ...`: pass.
- Binary size delta on `cargo build --release -p oneshim-app` (macOS arm64, stripped by default): **default 46.4 MB, `--features telemetry` 47.6 MB → +1.2 MB**. Well under the ≤5 MB target from the spec (§7).

### Known Flaky / Quarantined Tests

- None in the default non-ignored Rust test suite.
- `oneshim-embedding` fastembed constructor/embed smoke tests remain ignored by default because they download model assets and should only run in explicitly network-enabled verification.

### Ignored Tests

21 tests are marked `#[ignore]` because they require external dependencies or long runtime:

| Crate | Count | Reason |
|-------|-------|--------|
| oneshim-vision | 7 | macOS accessibility API (requires live OS permission) — +1 added by Phase 4 Updater Hardening |
| oneshim-embedding | 3 | Model download from Hugging Face |
| oneshim-storage | 3 | Keychain integration (requires macOS keychain access); mutex poison path covered by Phase 5-D8 PR1 dedicated test |
| oneshim-network | 2 | Doc-test examples requiring runtime context |
| src-tauri | 5 | GitHub API e2e (2) + long-running memory profile (3) |
| oneshim-storage (doc) | 1 | Doc-test example requiring runtime context |

Run ignored tests explicitly: `cargo test --workspace -- --ignored`

### Release Hygiene Baseline

- `CHANGELOG.md` must contain exactly one `[Unreleased]` header.
- Release preparation and stable promotion must sync `Cargo.lock` workspace package versions.
- Release workflow verifies `Cargo.toml`, `Cargo.lock`, frontend package version, changelog hygiene, and Tauri metadata before build fan-out.

## Notes

- Historical release notes belong in [`CHANGELOG.md`](../CHANGELOG.md).
- GUI V2 milestone history and deeper implementation context belong in ADR and crate docs, not in this mutable status file.
