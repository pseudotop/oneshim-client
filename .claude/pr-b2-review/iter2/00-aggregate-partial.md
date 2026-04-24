# PR-B2 Iter-2 Review — Partial Aggregate (4 of 5 reviewers in)

**Date**: 2026-04-22
**Spec commit**: 41252400 (rev 2)
**Status**: 4 of 5 dimensions reported. Dim 02 (server state/concurrency) still pending.

## Verdicts so far

| # | Dim | Verdict | New Crit | New Imp | New Minor |
|---|---|---|---|---|---|
| 01 | API contract | CONVERGED | 0 | 0 | 0 |
| 02 | Server state | _pending_ | — | — | — |
| 03 | Security | converged-with-issues | 0 | 2 | 0 |
| 04 | Test strategy | NOT CONVERGED | 0 | 4 | 0 |
| 05 | Stale audit | issues | 1 (impl-blocking) | 2 | — |

**Partial counts**: **1 Critical-ish**, **8 Important** new issues across 4 reviewers.

## Consolidated iter-2 findings

### CRIT-V2-A. §4.2 Rust-variant naming regression (Dim 05 finding #2)

Iter-1 CRIT-1 fixed the wire contract (§3) to use canonical `LOAD_LEVEL_*` proto names. But **§4.2 pseudocode still reads `ProtoLevel::{Low|Medium|High|Critical}`** — prost generates Rust variants as `Level::LoadLevelLow / LoadLevelMedium / LoadLevelHigh / LoadLevelCritical` (camelCased from SCREAMING_SNAKE). Implementor writing `Level::Low` compile-fails.

**Fix**: update §4.2 `build_hint` pseudocode to use `Level::LoadLevelLow`, `Level::LoadLevelMedium`, etc. Same for §4.3 and any other §4 Rust pseudocode.

### IMP-V2-A. `:authority` IPv6 bracket parsing bug (Dim 03 IMP-SEC-A)

Spec §4.3 validator: `authority.split(':').next()` on `"[::1]:10091"` returns `"["`, not `"[::1]"`. Breaks IPv6 loopback clients.

**Fix**: use `url::Host::parse` OR manually `if host.starts_with('[') { host.rfind(']').map(|i| &host[..=i]) } else { host.split(':').next() }`. Also add `"[::ffff:127.0.0.1]"` to allowlist for symmetry with IPv6-mapped v4.

### IMP-V2-B. `#[instrument]` grep enforcement fragility (Dim 03 IMP-SEC-B)

Multi-line attrs, renamed re-exports (`use tracing::instrument as trace`), or new sensitive fn not in allowlist all bypass. Can't elevate grep to "enforcement"; it's first-line heuristic only.

**Fix**: §7.4 downgrade language from "CI grep enforces" to "grep is first-line heuristic; actual invariant guard is `GrpcSpawnConfig::Debug` redaction test (IMP-16) + code-review checklist entry". Consider `trybuild`-based assertion or custom clippy lint as v3 follow-up.

### IMP-V2-C. `test-support` feature acceptance criterion missing (Dim 04 #1)

§8 acceptance #4 invokes `cargo test ... --features grpc-dashboard` but `MockSystemMonitor` is gated behind `feature = "test-support"`. Integration tests won't compile.

**Fix**: §8 #4 update to `--features grpc-dashboard,test-support`.

### IMP-V2-D. `test-support` feature definition guard missing (Dim 04 #2)

Spec §7.5 doesn't explicitly declare `test-support = []` + "excluded from default / grpc-dashboard featureset". Risk: if `test-support` auto-enables through `grpc-dashboard`, mock types ship in release.

**Fix**: §7.5 pins `test-support = []`; add guard: "test-support MUST NEVER be enabled by default or transitively via grpc-dashboard".

### IMP-V2-E. Virtual-time ordering ambiguity (Dim 04 #3)

§7.1 recipe ends at "→ exercise". But tonic server's `tokio::time::interval` registered BEFORE `pause()` fires on real-clock. The `subscribe_metrics` RPC call (which creates the handler's interval) must come AFTER `pause()`, not just connect.

**Fix**: §7.1 clarify sequence: `pick_free_port → spawn server → wait_for_server_ready (real clock) → connect client → pause() → subscribe_metrics call → advance()`.

### IMP-V2-F. REST parity claim false (Dim 05 finding #3)

§6 claims case-insensitive `strip_prefix_ignore_ascii_case("Bearer ")` "matches REST at lib.rs:524". Actual REST: `strip_prefix("Bearer ")` (case-sensitive).

**Fix**: §6 reword from "matches REST" to "stricter than REST — case-insensitive per RFC 7235; REST parity is a separate v3 cleanup item".

### IMP-V2-G. Test count header off-by-one (Dim 04 #4, minor)

Header says 40-41 new tests. Actual: 41-42. Tighten header.

### IMP-V2-H. §10 Row 3 off-by-one (Dim 05 finding #1, minor)

`SystemMonitor::collect_metrics` at `ports/monitor.rs:20` — actual line 21.

## Dim 02 expected scope

Server state/concurrency reviewer is checking:
- CRIT-3 `StreamCounterGuard` Drop correctness inside `async_stream!`
- CRIT-4 CAS correctness (ordering choices)
- CRIT-5 restructured rate-limit gate
- IMP-6 consecutive counter × HintEmitter interaction
- Concurrency primitives overall

## Next actions (once Dim 02 reports)

1. Finalize iter-2 aggregate
2. Apply 1 Crit + 8 Imp fixes (~60 LoC spec edits)
3. Commit spec rev 3
4. Dispatch iter-3 reviewers (goal: zero C/I both rounds — convergence gate)
