# Multi-Agent Audit Remediation Design

**Date**: 2026-03-07
**Scope**: client-rust (pseudotop/oneshim-client)
**Source**: 12-agent comprehensive audit (docs, UX, architecture, privacy, security, performance, OSS, enterprise, product strategy, supply chain, devil's advocate, test strategy)

## Overview

Sequentially remediate all findings from the multi-agent audit using isolated git worktrees per fix group, each resulting in a PR to `main`.

## Phase Structure

### Phase 1 — Parallel (4 worktrees)

| Worktree branch | Scope | Key files |
|----------------|-------|-----------|
| `fix/docs-tauri-migration` | Update CLAUDE.md, README.md, docs/crates/, PHASE-HISTORY.md to reflect Tauri v2 migration; create ADR for Tauri decision; deprecate oneshim-ui docs | `CLAUDE.md`, `README.md`, `docs/crates/oneshim-ui.md`, `docs/PHASE-HISTORY.md`, `docs/architecture/ADR-*.md` |
| `fix/security-tls-config` | Enable TLS by default for gRPC/HTTP; add localhost auth for web dashboard | `crates/oneshim-network/src/`, `src-tauri/` config |
| `fix/storage-encryption` | Add SQLite encryption (sqlcipher or at-rest key); document encryption key management | `crates/oneshim-storage/src/` |
| `fix/frontend-i18n` | Translate ErrorBoundary; add missing English tray menu labels; sync Korean companion docs | `crates/oneshim-web/frontend/src/i18n/`, `src-tauri/src/tray.rs`, `*.ko.md` |

### Phase 2 — Parallel (3 worktrees, after Phase 1 fully merged)

| Worktree branch | Scope | Key files |
|----------------|-------|-----------|
| `fix/frontend-ux-a11y` | Add aria-live to status indicators; add role to EmptyState; add server-down recovery guidance UI; improve API error messages | `crates/oneshim-web/frontend/src/components/` |
| `fix/privacy-consent` | Implement pre-consent data deletion; expand PII filter patterns; add consent audit trail; fix OCR strict-mode gap | `crates/oneshim-monitor/src/`, `crates/oneshim-core/src/` |
| `fix/rust-error-handling` | Remove `unwrap()` occurrences; structure error types per crate; add missing port adapter tests | All `crates/*/src/` |

### Phase 3 — Sequential (1 worktree, after Phase 2 fully merged)

| Worktree branch | Scope | Key files |
|----------------|-------|-----------|
| `fix/enterprise-oss-docs` | Add MDM deployment guide; add Good First Issues guide; improve CI transparency docs; add Tauri governance doc; add IPC command contract doc; add version migration guide (v0.1.4→v0.1.7) | `docs/guides/`, `docs/contracts/`, `.github/` |

## Worktree Workflow (per branch)

```
1. cd client-rust
2. git worktree add .claude/worktrees/<name> -b fix/<name>
3. cd .claude/worktrees/<name>
4. implement changes
5. cargo build / pnpm build (verify)
6. git commit -m "fix(<scope>): <description>"
7. git push origin fix/<name>
8. gh pr create --base main --title "..." --body "..."
9. (review + merge)
10. git worktree remove .claude/worktrees/<name>
```

## PR Strategy

- One PR per worktree branch
- Base: `main` (client-rust)
- Title format: `fix(<scope>): <short description>`
- Body: links to audit findings, test evidence
- After all PRs merged: update parent repo submodule pointer

## Testing Per Phase

| Phase | Verification |
|-------|-------------|
| docs | `markdownlint`, link checker, manual review |
| security-tls | `cargo test -p oneshim-network`, TLS handshake smoke test |
| storage-encryption | `cargo test -p oneshim-storage`, DB open/read/write with key |
| frontend-i18n | `pnpm test`, visual check in both en/ko |
| frontend-ux | `pnpm test`, axe-core a11y check |
| privacy-consent | `cargo test -p oneshim-monitor`, consent flow integration test |
| rust-error-handling | `cargo build --workspace`, `cargo test --workspace` |
| enterprise-oss | Manual review, markdownlint |

## Success Criteria

- All 40+ audit findings addressed or explicitly deferred with rationale
- `cargo build --workspace` passes
- `pnpm build` passes
- All existing tests pass (831 Rust + frontend)
- Each group has its own PR with clear scope
