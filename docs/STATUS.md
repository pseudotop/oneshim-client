[English](./STATUS.md) | [한국어](./STATUS.ko.md)

# Project Status (Single Source of Truth)

This document is the canonical source for mutable quality signals and workflow references.

## Scope

Track the following here and link this file from other docs instead of duplicating mutable values:

- Latest full CI workflow status and link
- Latest release workflow status and link
- Local verification baseline for current branch work
- Known flaky or quarantined tests

## Update Policy

Update this document whenever workflow status, verification baseline, or known flake status changes.

Recommended verification commands:

```bash
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd crates/oneshim-web/frontend && pnpm lint && pnpm build-storybook
```

## Current Snapshot (2026-03-30)

### Workflow Status

- Latest `main` CI run: failure (`CI`) — [Run 23740715704](https://github.com/pseudotop/oneshim-client/actions/runs/23740715704) (2026-03-30). Root cause: `oneshim-embedding` constructor coverage depended on a live Hugging Face download and failed on HTTP 504.
- Latest successful full CI run: success (`CI`, PR #263) — [Run 23740036667](https://github.com/pseudotop/oneshim-client/actions/runs/23740036667) (2026-03-30).
- Latest RC release run: success (`Release`, tag `v0.4.11-rc.2`) — [Run 23740840957](https://github.com/pseudotop/oneshim-client/actions/runs/23740840957) (2026-03-30).
- Latest successful stable recovery run: success (`Release`, workflow_dispatch for `v0.4.10`) — [Run 23732221718](https://github.com/pseudotop/oneshim-client/actions/runs/23732221718) (2026-03-30).

### Local Verification Baseline

- `cargo check --workspace`: pass
- `cargo clippy --workspace --all-targets -- -D warnings`: pass
- `cargo test --workspace`: pass
- `pnpm lint` (`crates/oneshim-web/frontend`): pass
- `pnpm build-storybook` (`crates/oneshim-web/frontend`): pass

### Known Flaky / Quarantined Tests

- None in the default non-ignored Rust test suite.
- `oneshim-embedding` fastembed constructor/embed smoke tests remain ignored by default because they download model assets and should only run in explicitly network-enabled verification.

### Release Hygiene Baseline

- `CHANGELOG.md` must contain exactly one `[Unreleased]` header.
- Release preparation and stable promotion must sync `Cargo.lock` workspace package versions.
- Release workflow verifies `Cargo.toml`, `Cargo.lock`, frontend package version, changelog hygiene, and Tauri metadata before build fan-out.

## Notes

- Historical release notes belong in [`CHANGELOG.md`](../CHANGELOG.md).
- GUI V2 milestone history and deeper implementation context belong in ADR and crate docs, not in this mutable status file.
