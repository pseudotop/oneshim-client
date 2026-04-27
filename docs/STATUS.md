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

## Current Snapshot (2026-04-27)

### Version

v0.4.41-rc.1 (Phase 9 PR-B1 cross-platform autostart foundation + PR-B2 Linux deep robustness shipped; v0.4.41-rc.1 candidate)

### Workspace

- **Packages**: 15 per `cargo metadata --no-deps` (14 crates under `crates/` including `oneshim-sandbox-worker` + `src-tauri`; planned `oneshim-ui` and former `crates/oneshim-app/` removed by ADR-004 Tauri migration)
- **SQLite schema**: V31 (V31 added by Phase 3 for `regime_manager_state`)

### Workflow Status (as of 2026-04-27 spot-check)

- **Latest `main` CI run**: Pending after PR-B2 merge. Pre-PR-B2 main CI had mixed status due to [RUSTSEC-2026-0104](https://rustsec.org/advisories/RUSTSEC-2026-0104) (`rustls-webpki 0.103.10` CRL parsing panic) — fixed via PR #526 lockfile-only bump to 0.103.13.
- **Latest Release RC**: Pending after PR-B2 merge (v0.4.41-rc.1 candidate). Previous: `v0.4.40-rc.2` success — [Run 24950722387](https://github.com/pseudotop/oneshim-client/actions/runs/24950722387) (2026-04-26 07:02 UTC).
- **Most recent stable tag**: v0.4.40 (2026-04-27). Promoted from v0.4.40-rc.3 via `promote-stable.sh`; v0.4.40-rc.2 awaiting-promotion clause now resolved.

### Local Verification Baseline

- `cargo check --workspace`: pass
- `cargo clippy --workspace --all-targets -- -D warnings`: pass
- `cargo test --workspace`: pass — **3,906 passed, 0 failed, 24 ignored** (last full re-measurement on v0.4.41-rc.1 candidate). PR-B2 adds: `lifecycle::migration_hashes` 10 unit tests + `lifecycle::sd_notify` 2 smoke tests = +12 over PR-B1 baseline. Linux-only inline tests (`autostart::tests::linux_capability_tests` +3, `linux_autostart_unit` smoke +1) compile cleanly but require Linux CI to execute. Earlier cumulative growth from prior 3,798 baseline (v0.4.39-rc.1) came from Phase 9 PR-B1 cross-platform autostart foundation, TimeWindow primitive consolidation (+37), D5 SanitizedDisplay coaching + gui_pipeline migrations, plus dependency bumps in PRs #494/#497/#514. Earlier growth from prior 3,651 baseline: Phase 9 PR-A Tracking Schedule (+147). ADR-019 Error Code Infrastructure + C5 Bedrock skip + post-merge drift audit iter 87~214 added ~196 tests earlier. Phase 2 telemetry tests run separately via `--features telemetry -- --test-threads=1`. **Not counted in 3,906**: 6 `map_challenge_status_to_error` tests (iter-195 Follow-up #5) are feature-gated behind `lan-sync` — run `cargo test -p oneshim-network --features lan-sync --lib sync::lan_transport::auth` to include them.
- `cargo fmt --check`: pass
- `pnpm lint` (`crates/oneshim-web/frontend`): pass
- `pnpm build-storybook` (`crates/oneshim-web/frontend`): pass
- Frontend Vitest (`pnpm test --run` in `crates/oneshim-web/frontend`): pass — **279 passed across 43 test files** at last re-measurement on PR-B2 baseline (no new test cases added by PR-B2 — `translateError.test.ts` wire-code count assertion bumped 49 → 53 reflecting 4 new `autostart.*` wire codes; PR-B1 baseline of 272/42 files grew with subsequent merges).

### Phase 2 Telemetry Feature (added 2026-04-17)

- `cargo test -p oneshim-app --features telemetry -- --test-threads=1`: pass — **10 passed** (T-X2-1 is default-build-only and runs in the workspace suite above).
- `cargo clippy -p oneshim-app --features telemetry --all-targets -- -D warnings ...`: pass.
- Binary size delta on `cargo build --release -p oneshim-app` (macOS arm64, stripped by default): **default 46.4 MB, `--features telemetry` 47.6 MB → +1.2 MB**. Well under the ≤5 MB target from the spec (§7).

### Known Flaky / Quarantined Tests

- None in the default non-ignored Rust test suite.
- `oneshim-embedding` fastembed constructor/embed smoke tests remain ignored by default because they download model assets and should only run in explicitly network-enabled verification.

### Ignored Tests

24 tests are marked `#[ignore]` because they require external dependencies or long runtime:

| Crate | Count | Reason |
|-------|-------|--------|
| oneshim-vision | 7 | macOS accessibility API (requires live OS permission) — +1 added by Phase 4 Updater Hardening |
| oneshim-embedding | 3 | Model download from Hugging Face |
| oneshim-storage | 3 | Keychain integration (requires macOS keychain access); mutex poison path covered by Phase 5-D8 PR1 dedicated test |
| oneshim-network | 2 | Doc-test examples requiring runtime context |
| src-tauri | 8 | GitHub API e2e (2) + long-running memory profile (3) + Linux systemd live integration T8/T9/T10 (3, requires user systemd — run via workflow_dispatch `linux-systemd-integration.yml`) |
| oneshim-storage (doc) | 1 | Doc-test example requiring runtime context |

Run ignored tests explicitly: `cargo test --workspace -- --ignored`

### Release Hygiene Baseline

- `CHANGELOG.md` must contain exactly one `[Unreleased]` header.
- Release preparation and stable promotion must sync `Cargo.lock` workspace package versions.
- Release workflow verifies `Cargo.toml`, `Cargo.lock`, frontend package version, changelog hygiene, and Tauri metadata before build fan-out.

## Notes

- Historical release notes belong in [`CHANGELOG.md`](../CHANGELOG.md).
- GUI V2 milestone history and deeper implementation context belong in ADR and crate docs, not in this mutable status file.
