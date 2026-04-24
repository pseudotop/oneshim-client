# PR-B2 Spec Iter-3 Verdict

**Date**: 2026-04-22
**Spec commit**: `abc16ca7` (rev 3)
**Reviewer**: Single consolidated 5-dimension convergence pass

## Per-dimension verdict

| # | Dim | Verdict | Crit | Important |
|---|---|---|---|---|
| 01 | API contract | CONVERGED | 0 | 0 |
| 02 | Server state/concurrency | CONVERGED | 0 | 0 |
| 03 | Security | CONVERGED | 0 | 0 |
| 04 | Test strategy | CONVERGED | 0 | 0 |
| 05 | Stale audit | CONVERGED | 0 | 0 |

## Overall: **SPEC CONVERGED — proceed to Phase 2 Plan**

All 12 iter-2 findings verified applied correctly:
- CRIT-V2-A §4.2 prost naming → `Level::LoadLevel*`
- IMP-V2-A IPv6 bracket-aware validator (traces confirmed for `[::1]:port`, `127.0.0.1:port`, `localhost:port`, `::ffff:127.0.0.1`)
- IMP-V2-B grep heuristic (language downgraded)
- IMP-V2-C test-support in integration-test command
- IMP-V2-D test-support = [] pinned + no-default-features smoke
- IMP-V2-E subscribe_metrics AFTER pause() in virtual-time recipe
- IMP-V2-F "stricter than REST" wording
- IMP-V2-G test count 41-42
- IMP-V2-H §10 row 3 line :21
- IMP-B2 force_emit_degraded routes through HintEmitter
- MIN-B1 effective_interval_cache seeded with Medium
- MIN-B2 ticker_period sibling tracking

## Non-blocking nits (plan-phase items)

- **NIT-R3-A**: `force_emit_degraded` returns `Option<ServerLoadHint>` always `Some` — consider `ServerLoadHint` direct in plan impl
- **NIT-R3-B**: `cargo build -p oneshim-web --no-default-features --features grpc-dashboard` — plan must dump `oneshim-web/Cargo.toml [features]` to confirm `grpc-dashboard` has no hard dep on other default features
- **NIT-R3-C**: `force_emit_degraded` takes `reason_tag: &str` but passes `Option<&str>` to `build_hint` — internal API mismatch; clear enough but tighten during impl

## Still plan-phase contingent (from rev 2/3 spec §10 rows 18/19)

- `tonic::Request::remote_addr()` behavior under current `grpc::serve()` wiring — plan-phase first task is a smoke test. If `None`, plan adds `TcpConnectInfo` tower layer.

Phase 2 plan can proceed.
