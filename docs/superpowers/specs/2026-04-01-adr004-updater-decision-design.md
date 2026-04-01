# ADR-004 Updater Decision: Retain self_update

**Date**: 2026-04-01
**Status**: Approved
**Scope**: ADR-004 documentation update only — no code changes

## Problem

ADR-004 (Tauri v2 Migration) specifies "Auto-update: `tauri-plugin-updater`" but the current implementation uses `self_update` with extensive custom logic that exceeds `tauri-plugin-updater`'s capabilities.

## Decision

Retain `self_update` and update ADR-004 to reflect this. The current implementation is production-proven and provides features that `tauri-plugin-updater` does not:

- **SHA256 integrity verification** — hard stop on hash mismatch
- **Ed25519 signature verification** — optional, configurable public key
- **Rollback on restart failure** — automatic binary restoration
- **Prerelease filtering** — `include_prerelease` config flag
- **Version floor enforcement** — `min_allowed_version` for forced security updates
- **Custom state machine** — Idle → Checking → PendingApproval → Installing → Updated/Error/Deferred
- **Coordinator pattern** — broadcast channel for Tauri event bridge
- **URL allowlist** — only github.com and githubusercontent.com domains

Migrating to `tauri-plugin-updater` would require rebuilding these features or accepting their loss, with no clear benefit.

## Change

Update `docs/architecture/ADR-004-tauri-v2-migration.md`:
- Change "Auto-update: `tauri-plugin-updater`" to document `self_update` retention
- Add rationale for the deviation
