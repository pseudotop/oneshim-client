# Phase 4 Updater Hardening — PR body draft

Ready-to-paste PR description for `feat/phase4-updater-hardening-spec` → `main` (after v0.4.39 stable is promoted).

---

## Title

```
feat(updater): Phase 4 Updater Hardening — D9 multi-key trust + D10 rollout defense + D11 auto-rollback
```

## Body

### Summary

Bundles all three Phase 4 Updater Hardening improvements (per `docs/reviews/2026-04-16-feature-gaps-analysis.md`) into one PR, closing three paths for bad-release propagation:

- **D9** Multi-key Ed25519 trust array for day-1 key rotation support (scheduled + compromise procedures documented).
- **D10** Defensive `installation_id: None` handling in the staged rollout gate — eliminates the "config regression → admit every device to first-receive cohort" loophole.
- **D11** Post-install self-healthy probe + automatic rollback — 2 consecutive failed boots without a 30s-uptime self-healthy marker triggers restoration of the previous binary.

Plus **A-4** release metadata enrichment (Release Date + commit-count headers in CHANGELOG / GitHub Release notes / client UI) for operational observability.

### 3-Loop Ralph Loop discipline

| Loop | Iterations | Reviewer EXIT verdict |
|---|---|---|
| L1 Spec | 5 | 0 Critical + 0 Important |
| L2 Plan | 2 | 0 Critical + 0 Important |
| L3 Impl | 3 (1 fixes + 2 review) | 0 Critical + 0 Important (4 independent reviewers total) |

All 4 reviewers independently verified EXIT criteria.

### Files

**Modified (10)**: `src-tauri/src/updater/{mod,install,github}.rs`, `app_runtime_launch.rs`, `scheduler/mod.rs` (via update_coordinator), `update_coordinator.rs`, `crates/oneshim-api-contracts/src/update.rs`, `crates/oneshim-core/src/config/sections/storage.rs`, `crates/oneshim-web/src/update_control.rs`, `crates/oneshim-web/frontend/src/{api/contracts.ts, components/UpdatePanel.tsx, i18n/locales/{en,ko}.json}`, `.github/workflows/release.yml`, `cliff.toml`, `CHANGELOG.md`.

**Created (5)**: `src-tauri/src/updater/health_probe.rs`, `src-tauri/src/updater/trusted_keys.rs`, `docs/guides/{updater-rollout, updater-key-rotation, updater-rollback-windows}.md`.

### Verification

- `cargo test --workspace`: **3,445 passed** / 0 failed / 21 ignored (+27 new tests)
- `cargo clippy --workspace --all-targets -- -D warnings`: clean
- `cargo fmt --check`: clean
- `pnpm lint` + `pnpm test` (frontend): clean, 231 tests passed
- Manual macOS smoke: forcibly crash app twice within 30s of launch → third launch triggers rollback (`UpdatePhase::RolledBack` emitted + notification file consumed + UI renders rolled-back state with from/to versions + dates)

### Breaking vs non-breaking

**Non-breaking** — targeting v0.4.40-rc.1, not v0.5.0. D9 default `require_signature_verification: true` was already live in production (`storage.rs:349`); this PR only ADDS the multi-key array alongside the existing single-key default.

### Deferred follow-ups (tracked)

1. **Windows rollback implementation** — `docs/guides/updater-rollback-windows.md` recommends `cmd.exe` helper (Option A). Requires a dedicated Windows CI runner + signed-helper decision. Separate PR.
2. **Concurrent-process race mitigation** — per-PID `.boot_count_pid_{PID}` sub-files, documented in `health_probe.rs` module header.
3. **`pre-release-check.sh:241` Dependabot JSON guard** — 5-minute bug fix PR.
4. **Notarization workflow `head_branch` condition** — separate infra PR.

### Design docs

- **Spec**: [docs/reviews/2026-04-18-phase4-updater-hardening-design.md](docs/reviews/2026-04-18-phase4-updater-hardening-design.md)
- **Plan**: [docs/reviews/2026-04-18-phase4-updater-hardening-plan.md](docs/reviews/2026-04-18-phase4-updater-hardening-plan.md)

### Commit map (18 commits)

- T0 audit: `1df426c5`
- T1 scaffolding: `2fbb22f7`
- T2 D9 multi-key: `3fa2ef16`
- T3 D10 defensive None: `1a4ead68`
- T4 D10 rollout doc: `1511ffcb`
- T5 D11 probe core: `658c6db1`
- T6 D11 install_pending + orphan cleanup: `67d64abd`
- T7 execute_rollback + integration test: `ee5be33b`
- T8 probe wiring (launch): `8618eda9`
- T9 UpdateControl::set_rolled_back bridge: `480dc57e`
- T10 frontend + 15 i18n keys: `ce7a5127`
- T11 cliff.toml body amendment: `d7829670` + `85440863`
- T12 Windows spike doc: `dd3c3d04`
- T13 key rotation runbook + CHANGELOG: `c9cda02d`
- Loop 3 iter 1 fixes (I-1, I-2, I-5): `45578368`
- Loop 3 iter 2 EXIT: `66c26828`
- Loop 3 iter 3 polish (M-1, M-3): `a15d0237`

🤖 Generated with [Claude Code](https://claude.com/claude-code)
