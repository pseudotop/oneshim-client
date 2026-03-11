# Release Checklist — v{VERSION}

> Complete ALL items before tagging a release. No exceptions.

## Automated Gates (must be green)
- [ ] Quick suite (PR CI) — all green
- [ ] Nightly suite — last run green (within 24h)
- [ ] cargo-mutants score ≥ 70% on oneshim-core
- [ ] Zero P0/P1 flaky tests in quarantine

## Manual Verification
- [ ] `cargo build --release` succeeds on macOS
- [ ] `cargo build --release` succeeds on Windows (or cross-compile)
- [ ] App launches and shows Dashboard with real data
- [ ] Settings save/load round-trip works
- [ ] Auto-updater detects the new version (staging)

## Test Layers Verification
- [ ] Layer 1 (Rust): `cargo test --workspace` — 0 failures
- [ ] Layer 2 (Mock IPC): `pnpm test` — 0 failures
- [ ] Layer 3 (Playwright): `pnpm test:e2e` — 0 failures
- [ ] Layer 4 (Tauri WDIO): `run-e2e-tauri.sh` — 0 failures

## Documentation
- [ ] CHANGELOG.md updated
- [ ] Breaking changes documented (if any)

## Sign-off
- [ ] Maintainer approval
- [ ] Tag created: `git tag -s v{VERSION}`
