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

## Current Snapshot (2026-04-26)

### Version

v0.4.40-rc.2 (Phase 9 PR-A Tracking Schedule + PR-B1 cross-platform autostart foundation + TimeWindow primitive consolidation + D5 SanitizedDisplay coaching/gui_pipeline migrations shipped; v0.4.40-rc.2 released to GitHub)

### Workspace

- **Packages**: 15 per `cargo metadata --no-deps` (14 crates under `crates/` including `oneshim-sandbox-worker` + `src-tauri`; planned `oneshim-ui` and former `crates/oneshim-app/` removed by ADR-004 Tauri migration)
- **SQLite schema**: V31 (V31 added by Phase 3 for `regime_manager_state`)

### Workflow Status (as of 2026-04-26 spot-check)

- **Latest `main` CI run** (`docs: harden public export audit (#525)`): mixed — [Run 24960929828](https://github.com/pseudotop/oneshim-client/actions/runs/24960929828) (2026-04-26 16:02 UTC). `Integrity Gates` + `Security & Compliance` + `Release Smoke` failure; `Config Sync` + `gRPC Governance` green. Root cause: [RUSTSEC-2026-0104](https://rustsec.org/advisories/RUSTSEC-2026-0104) — `rustls-webpki 0.103.10` reachable panic in CRL parsing (advisory published 2026-04-22). **Fix in flight**: PR #526 bumps `rustls-webpki` to 0.103.13 via lockfile-only transitive bump (no `Cargo.toml` change required).
- **Latest fully-green main CI run**: [Run 24959996723](https://github.com/pseudotop/oneshim-client/actions/runs/24959996723) (2026-04-26 15:17 UTC, `docs: recover branch audit artifacts (#524)` push) — `Security & Compliance` + `Integrity Gates` already failing on the same advisory; `CI` workflow itself succeeded. Last fully-green pipeline pre-advisory: needs spot-check on next CI cycle.
- **Latest Release RC**: success (`Release`, `v0.4.40-rc.2`) — [Run 24950722387](https://github.com/pseudotop/oneshim-client/actions/runs/24950722387) (2026-04-26 07:02 UTC). Preceded by `v0.4.40-rc.1` failure (different root cause: yanked transitive `core2 0.4.0` → fixed via PR #519 `bitstream-io 4.10.0` bump).
- **Most recent stable tag**: v0.4.37 (2026-04-12). v0.4.40-rc.2 awaiting promotion via `promote-stable.sh` once main CI returns to green (gated on PR #526 merge).

### Local Verification Baseline

- `cargo check --workspace`: pass
- `cargo clippy --workspace --all-targets -- -D warnings`: pass
- `cargo test --workspace`: pass — **3,798 passed, 0 failed, 21 ignored** (last full re-measurement on v0.4.39-rc.1). Subsequent merges (Phase 9 PR-B1 cross-platform autostart foundation, TimeWindow primitive consolidation, D5 SanitizedDisplay coaching + gui_pipeline migrations, dependency bumps in PRs #494/#497/#514) likely add a small delta but were not full-suite re-measured. Re-measure on next stable promotion. Cumulative growth from prior 3,651 baseline came from Phase 9 PR-A Tracking Schedule (+147: A.2 serde+validation 12, A.4 pure-fn contracts 16, A.6 migration tests 3, A.8 scheduler gating 21, A.10 uploader suppression 3, A.13 IPC contract 5, A.15 REST handler 4, A.17 tray-watch 7, A.18 notifier integration 4, A.19 frontend Vitest 7) plus helper modules. Earlier cumulative growth: ADR-019 Error Code Infrastructure + C5 Bedrock skip + post-merge drift audit iter 87~214 added ~196 tests — 85+ HTTP status-mapping regression tests across 16 dispatchers, ~38 Internal→specific-variant re-route tests, 4 subprocess_kind (iter-149) + 3 LLM envelope-extraction (iter-151) + 10 `IpcError` contract tests (iter-196 Follow-up #1) + other targeted regression guards. Phase 2 telemetry tests run separately via `--features telemetry -- --test-threads=1`. **Not counted in 3,798**: 6 `map_challenge_status_to_error` tests (iter-195 Follow-up #5) are feature-gated behind `lan-sync` — run `cargo test -p oneshim-network --features lan-sync --lib sync::lan_transport::auth` to include them.
- `cargo fmt --check`: pass
- `pnpm lint` (`crates/oneshim-web/frontend`): pass
- `pnpm build-storybook` (`crates/oneshim-web/frontend`): pass
- Frontend Vitest (`pnpm test --run` in `crates/oneshim-web/frontend`): pass — **272 passed across 42 test files** at last re-measurement on Phase 9 PR-B1 baseline (+10 Vitest tests from autostart `GeneralTab Startup` section + `AutostartOnboardingPrompt`; wire-code count assertion bumped 42 → 47). Subsequent merges may shift this count.

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
