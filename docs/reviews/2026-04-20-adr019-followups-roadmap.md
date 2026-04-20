# ADR-019 Known Follow-ups — Roadmap

**Date:** 2026-04-20
**Status:** Design-complete, execution pending
**Parent ADR:** [ADR-019](../architecture/ADR-019-error-code-infrastructure.md)

This doc aggregates all ADR-019 §Known follow-ups as executable workstreams. Each has a paired design doc; plans are authored when execution is sequenced.

## Summary Table

| # | Title | Status | Design Doc | Effort | Dependency |
|---|-------|--------|-----------|--------|------------|
| 1 | Tauri IPC typed-code propagation | Designed | [ipc-error-dto-design](./2026-04-20-adr019-followup-ipc-error-dto-design.md) | ~1.5 day | none |
| 2 | Grafana dashboard relabeling | Designed | [grafana-relabeling-design](./2026-04-20-adr019-followup-grafana-relabeling-design.md) | ~1 day (elapsed) | ops coordination |
| 3 | Frontend i18n wiring | Designed | [frontend-i18n-wiring-design](./2026-04-20-adr019-followup-frontend-i18n-wiring-design.md) | ~1 day | Follow-up #1 |
| 4 | InternalCode granularity refinement | Evergreen | (N/A — driven by production telemetry) | ongoing | Follow-up #2 (data) |
| 5 | LAN transport auth regression tests | Designed | [lan-transport-tests-design](./2026-04-20-adr019-followup-lan-transport-tests-design.md) | ~0.5 day | none |

## Recommended Sequencing

```
┌── Follow-up #1 (IPC error DTO, ~1.5 day)
│        │
│        ▼
│   Follow-up #3 (frontend i18n, ~1 day) ─── depends on #1 for IpcError type
│
├── Follow-up #5 (LAN tests, 0.5 day) ─── independent, safe anytime
│
├── Follow-up #2 (Grafana relabeling, ~1 day) ─── ops coordination
│        │
│        ▼
└── Follow-up #4 (Internal granularity) ─── feeds off Grafana telemetry signals
```

**Critical path**: Follow-up #1 blocks #3 (frontend i18n needs the IpcError DTO). Everything else is parallelizable.

## Non-Goals for This Roadmap

Each follow-up design doc enumerates its own out-of-scope items. Cross-cutting non-goals:

- **Server-side error surface changes.** ADR-019 is client-rust scoped; server has its own error taxonomy.
- **Breaking wire-code changes.** The 41 codes in `wire_contract_snapshot.expected.txt` are immutable; new codes append only.
- **`CoreError` structural changes.** The variant set is frozen post-ADR-019; changes require a new ADR.

## Execution Notes

- Each follow-up ships as its own PR (or small PR series per the design doc). Not bundled.
- Per [`feedback_release_process.md`](../../.claude/.../memory) memory: version bumps (if any) use `./scripts/release.sh`.
- CHANGELOG entries for each follow-up append under `[Unreleased]` with a link to the parent ADR-019.
- Each design doc's "Implementation Plan" section should be promoted to a paired plan doc (`*-plan.md`) before execution begins, per the `docs/reviews/` convention.

## Progress Tracking

- [⏳] Follow-up #1 — 🟡 **Infrastructure + 91/114 migrations shipped (~80%)** (iter-196/197/199/201/203). Foundation (iter-196): DTO + 11 From-chain impls + 10 contract tests. Frontend TS (iter-197): IpcError interface + isIpcError + errorMessageFromInvoke + 13 Vitest tests. Migration batches: (iter-197) onboarding/detection/focus 8; (iter-199) coaching/dashboard/capture_status 26; (iter-201) settings/permissions/sync/automation 19; (iter-203) suggestions/capture/error_report/bug_report/analysis/system 35. Remaining: audio (10), ai_session (10), integration (12) = **23 signatures**.
- [ ] Follow-up #2 — design landed 2026-04-20, awaiting ops scheduling
- [ ] Follow-up #3 — design landed 2026-04-20, blocked on #1
- [ ] Follow-up #4 — evergreen, re-evaluate quarterly against Grafana telemetry
- [x] Follow-up #5 — ✅ SHIPPED iter-194 (2026-04-20). Took the simpler pure-function-extraction approach instead of the initially-designed rustls-TlsAcceptor fixture — 6 tests cover the same 5 canonical statuses + a 403 variant.

Update this table whenever a follow-up status changes.
