# Project Status (Single Source of Truth)

This document is the canonical source for mutable project quality metrics.

## Scope

The following values must be tracked only in this file:

- Rust test totals and pass/fail status
- E2E test totals and pass/fail status
- Lint/build status
- Known flaky test status

Other documents should link to this file instead of hard-coding mutable counts.

## Update Policy

Update this document whenever CI quality metrics change.

Recommended verification commands:

```bash
cargo test --workspace
cd crates/oneshim-web/frontend && pnpm test:e2e
cargo clippy --workspace
cargo fmt --check
```

## Current Snapshot

- Rust tests: See latest CI run artifact/logs for exact count
- E2E tests: See latest Playwright CI report for exact count
- Lint status: Enforced in CI (`cargo clippy --workspace`)
- Format status: Enforced in CI (`cargo fmt --check`)
- Build status: Enforced in CI (`cargo build --workspace`)

## Notes

- Historical numbers may remain in CHANGELOG entries because they describe past releases.
- For current status communication, always link this file.
