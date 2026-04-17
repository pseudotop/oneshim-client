[English](./ADR-018-regime-manager-persistence.md) | [한국어](./ADR-018-regime-manager-persistence.ko.md)

# ADR-018: RegimeManager Persistence

**Status**: Approved
**Date**: 2026-04-18
**Scope**: `oneshim-core::ports::regime_storage`, `oneshim-storage::regime_manager_state_store`, `oneshim-analysis::RegimeManager::hydrate_from`, `src-tauri::main::RunEvent::Exit`

---

## Context

`RegimeManager` was purely in-memory — every restart lost user-curated regime names, merges, deletes. The existing `regimes` SQL table is touched only by the cross-device sync path (`sync_merger.rs`); it does NOT carry RegimeManager's full state (centroid, RegimeStatus enum, name override).

See the 2026-04-16 gap analysis X6.

## Decision

A new `RegimeStoragePort` in `oneshim-core` and `SqliteRegimeManagerStateStore` in `oneshim-storage`. State is a JSON blob in a new **dedicated** `regime_manager_state` singleton table (v31 migration), not the existing `regimes` table.

On startup the composition root calls `store.load_all()` → `RegimeManager::hydrate_from(regimes)`. On graceful shutdown the `RunEvent::Exit` handler in `main.rs` calls `store.save_all(&regime_manager.all_regimes())` with a 4 s watchdog.

On parse failure, `load_all` quarantines the corrupt payload to `payload_backup` with `payload_backup_at` timestamp, logs `error!`, and returns `Ok(vec![])` so the app starts fresh. User-curated state is preserved for later recovery.

## Consequences

### Positive

- Regimes survive restart; the "new regime discovered" notification stops firing for the same cluster on every cold boot.
- Vector `regime_id` filter (C3a) becomes meaningful across sessions — regime IDs are now stable.
- sync_merger's use of the existing `regimes` table is untouched.

### Negative / Constraints

- JSON blob evolves with `Regime` struct. serde's `#[serde(default)]` handles additive fields. Removed/renamed fields trigger the quarantine path. Schema mismatches are never silent wipes.
- `load_all` is not read-only in the quarantine edge case. Doc warns callers; all call sites are single-shot at startup.
- Shutdown save is best-effort under a 4 s watchdog — matches telemetry's shutdown. Past the deadline we log `warn!` and continue; shutdown MUST NOT be blocked.

### Neutral

- Mid-life periodic save is OUT OF SCOPE for this phase. Shutdown-only is sufficient for routine restart survival; a follow-up phase can add periodic save after `run_maintenance` ticks if cold-kill data loss becomes a concern.

## Alternatives considered

- **Reuse the existing `regimes` table** — rejected. Its schema is partial (no centroid, no RegimeStatus enum, no user-name override) and it is owned by sync_merger. Extending it would require migration + write-path update to keep sync consistent. New dedicated table avoids that blast radius.
- **Per-regime rows instead of JSON blob** — rejected. RegimeManager's regime count is bounded (`max_active + archive_days`), so a single blob is simpler and negligible cost. Diff-API is a backward-compatible follow-up if it ever matters.
- **"Start fresh on parse failure"** — explicitly rejected during spec review. Wiping months of user-curated names silently is a regression. Quarantine preserves recovery path.

## References

- Spec: `docs/reviews/2026-04-18-phase3-regime-feedback-learning-spec.md`
- Gap analysis: `docs/reviews/2026-04-16-feature-gaps-analysis.md` C3 + X6
- ADR-016 ConfigChangeBus (shutdown-watchdog pattern)
