# PR-B2 Spec Review — Iter-1 Aggregate Findings

**Date**: 2026-04-22
**Reviewers**: 5 parallel dimensional subagents
**Spec under review**: `docs/reviews/2026-04-22-d13-v2b-pr-b2-spec.md` (commit 1efca7c3)

## Aggregate counts

| Dimension | Critical | Important | Minor | Verdict |
|---|---|---|---|---|
| 01 API contract | 2 | 7 | 6 | IMPORTANT issues |
| 02 Server state/concurrency | 3 | 7 | 6 | NOT CONVERGED |
| 03 Security/privacy | 4 | 10 | 7 | NEEDS CHANGES |
| 04 Test strategy | 4 | 5 | 4 | NEEDS CHANGES |
| 05 Stale-assumption audit | 2 | 2 | 1 | 5 of 15 stale |
| **Total (raw)** | **15** | **31** | **24** | — |

## Consolidated Critical issues (dedup-d across dimensions)

### CRIT-1. Enum variant names wrong in spec §3 (Dim1 C1)
Spec uses bare `LOW/MEDIUM/HIGH/CRITICAL`; generated proto at `proto/generated/oneshim.dashboard.v1.rs:313-320` uses `LOAD_LEVEL_LOW / LOAD_LEVEL_MEDIUM / LOAD_LEVEL_HIGH / LOAD_LEVEL_CRITICAL / LOAD_LEVEL_UNSPECIFIED`. Implementor writing `Level::Low` would compile-fail.
**Fix**: update §3 wire-contract block to canonical names.

### CRIT-2. Ghost method `LoadPolicy::enforced_frame_rate` (Dim1 C2, Dim5 N1)
§4.2 `build_hint` pseudocode calls `policy.enforced_frame_rate(level)` but §4.1 never defines it. Proto convention is `0 = no suggestion` for `suggested_event_rate_limit`.
**Fix**: emit literal `0` for `suggested_event_rate_limit` in PR-B2. Rate-limit population lands in PR-B3 (where EventRateLimiter exists).

### CRIT-3. Stream generator not Drop-aware → active-stream counter leaks (Dim2 C1)
§4.6 + §6 say counter decrements on generator drop, but `async_stream!` macro doesn't automatically run Drop bodies for captured locals. Plain `fetch_add`/`fetch_sub` pairs leak on: abrupt disconnect, `spawn_blocking` JoinError, any `yield Err(...)`.
**Fix**: define an explicit `StreamCounterGuard(Arc<AtomicUsize>)` struct with `Drop`. Moved into the `async_stream!` closure so counter decrement is guaranteed. Test #7 must assert counter returns to baseline after N drops.

### CRIT-4. 51st-subscriber TOCTOU (Dim2 C2)
Plain `load`-then-`fetch_add` permits 51 concurrent subscribers to all pass. Must use `fetch_add` → if `prev >= cap`, `fetch_sub` revert + `Status::resource_exhausted`. Happens BEFORE `async_stream!` body opens (before `Response` is returned).
**Fix**: spec §6 must pin exact CAS-style sequence. `serial_test` annotation per `reference_serial_test_pattern` for the cap test.

### CRIT-5. Realtime path busy-loops when opt-out + throttled (Dim2 C3)
Loopback opt-out with `interval_secs=0` → `effective_interval=250ms` → inner drain-loop + `last_emit.elapsed() < effective_interval` `continue` jumps to top of outer loop, re-runs `collect_metrics` + `classify` + back to `rx.recv()`. Burns CPU on each throttled iteration.
**Fix**: restructure §4.6 so skip-if-too-soon check gates metrics collection, not the emit step.

### CRIT-6. Loopback-only bind collapses "token branch" (Dim3 C1)
Spec §6 claims "trust = loopback OR token". Actual v2b runtime (grpc/mod.rs:348 binds 127.0.0.1 only) → every request matches `is_loopback()` → `honor_opt_out` returns false (opt-out granted) regardless of token. 3 of 6 `auth_gate_tests` become synthetic branch-coverage without live integration path.
**Fix**: explicitly state v2b trust policy as "every loopback caller is trusted to opt out" (simpler truth). OR tighten to "loopback AND token match" (stronger). Spec review must decide.

### CRIT-7. `tonic::Request::remote_addr()` likely returns `None` (Dim3 C2)
Current `grpc::serve` uses `Server::builder().add_service(...).serve(addr)` — no `ConnectInfo` layer. tonic does not attach `TcpConnectInfo` by default. Then `honor_opt_out(_, None, None, Some(token))` returns `true` (enforce) — loopback branch cannot trigger. Every opt-out attempt becomes warn-log + enforcement.
**Fix**: verify `remote_addr()` behavior at current serve call. If `None`, spec §4.4 must list tower-layer wiring changes. Add `auth_gate_handles_missing_remote_addr` unit test pinning fallback to "enforce".

### CRIT-8. active_stream_counter increment ordering unspecified (Dim3 C3)
Unauth flood protection needs counter increment BEFORE `streaming_enabled` check and BEFORE `honor_opt_out` call.
**Fix**: pin increment to first line of `subscribe_metrics`.

### CRIT-9. `#[tracing::instrument]` auto-logs function args including bearer token (Dim3 C4)
If `subscribe_metrics` / `honor_opt_out` / `GrpcSpawnConfig`-taking function gets `#[instrument]`, args auto-log.
**Fix**: spec §7 must prohibit `#[instrument]` on these functions. Structured log fields with explicit whitelist. CI grep-check.

### CRIT-10. `network.rs` has NO `mod tests` block (Dim4 C1)
Spec claims "+3 new tests to existing file". Actually `crates/oneshim-core/src/config/sections/network.rs` has zero `#[cfg(test)]`.
**Fix**: Replace spec wording to "create `#[cfg(test)] mod tests` block with 3 tests".

### CRIT-11. Token-leakage test underspecified (Dim4 C2)
`tracing-subscriber` is NOT a dev-dep of `oneshim-web`. `serial_test` is NOT in workspace. Spec's 5-LoC implicit budget ignores ~40 LoC of MakeWriter + subscriber setup.
**Fix**: spec pick one — (a) add dev-deps + `MakeWriter` scaffold, OR (b) downgrade to `#[instrument]`-prohibition (per CRIT-9) + hand-audit. **Recommend (b)** — cheaper and addresses more paths.

### CRIT-12. 51-concurrent-stream test flakes on fd pressure (Dim4 C3)
macOS ulimit -n = 256 + parallel cargo-test workers.
**Fix**: introduce `GrpcSpawnConfig.max_concurrent_streams` (default 50), set to 5 in test, open 6 streams, assert 6th fails. Cheap + same code path exercised.

### CRIT-13. `MockSystemMonitor` location ambiguous — PR-B3 reuse debt (Dim4 C4)
§4 says `tests/common/`; §9.8 open-q says "both integration tests importing directly". `tests/common/` doesn't auto-share across separate integration-test files.
**Fix**: Move to `crates/oneshim-web/src/grpc/test_support/` under `#[cfg(any(test, feature = "test-support"))]`. Matches `NoopPiiSanitizer` workspace convention; free PR-B3 reuse.

### CRIT-14. `WebConfig.grpc_port` doesn't exist (Dim5 #2)
Spec §10 claims as verified. Actually ENV-var-only (`ONESHIM_DASHBOARD_GRPC_PORT` at `app_runtime_launch.rs:782`). `CHANGELOG.md:18` explicitly deferred.
**Fix**: clarify sourcing in spec §5.

### CRIT-15. `app_state.diagnostics.pii_sanitizer` is phantom chain (Dim5 #8)
`AppState` at `runtime_state.rs:347-384` has NO `diagnostics` sub-struct. `PiiSanitizer` is wired inline at consumer builders.
**Fix**: PR-B2 passes `None` for `pii_sanitizer`; spec must remove the phantom chain reference. (Also lines up with Dim3 I9 recommending drop entirely from PR-B2.)

## Consolidated Important issues

### IMP-1. Wrong status code for kill-switch (Dim1 I1)
`Status::unimplemented` conflates "not compiled" with "temporarily disabled". Clients reasonably treat UNIMPLEMENTED as permanent.
**Fix**: use `Status::unavailable("grpc streaming temporarily disabled")` or `Status::failed_precondition`.

### IMP-2. `MetricBucket.start` wire relationship from PR-B1 unacknowledged (Dim1 I2)
Spec §3 notes field numbers preserved; does not state explicit migration path for v2a clients already on the generated nested path `productivity_metrics_response::MetricBucket`.
**Fix**: spec §3 adds explicit "v2a Rust clients: import migration note; wire format unchanged" paragraph referencing PR-B1 commit c456367b which did the promotion.

### IMP-3. Active-stream-cap integration point not specified in §4.6 (Dim1 I3)
§6 mentions cap, §4.6 pseudocode omits the increment/decrement. Implementors might skip.
**Fix**: §4.6 pseudocode adds explicit `StreamCounterGuard::try_acquire_or_reject(counter, cap)?` as step 0.

### IMP-4. Clamping table nonsense row (Dim1 I4)
Spec §3 table has row "`interval_secs` in (0, 1) requested as zero-second — not representable (uint32) → N/A — uint32 starts at 0". Superfluous.
**Fix**: delete row.

### IMP-5. Window-timestamp drift on level transition (Dim1 I5)
Changing `effective_interval` mid-stream means bucket following HIGH→LOW still uses the previous interval for window_start computation.
**Fix**: spec §4.6 notes this is accepted 1-tick-smearing; add to §11 Risks.

### IMP-6. DB-error `continue` in interval mode silently doubles interval (Dim1 I6, Dim2 I5)
`spawn_blocking ⇒ Ok(Err(_)) ⇒ continue` skips emit; next tick is after `sleep(effective_interval)` again, user sees 2× interval gap with no log visibility beyond `warn!`.
**Fix**: add `consecutive_failures: u32` counter; after N=5 failures, emit a Hint with `reason="db_error_degraded"`. Optional: close stream after N=10. Decision needed.

### IMP-7. IPv6-mapped-v4 loopback not covered (Dim1 I7, Dim3 I2)
`is_loopback()` on `::ffff:127.0.0.1` — Rust stdlib returns false for mapped v4 loopback unless manually converted.
**Fix**: spec §4.3 add `if let IpAddr::V6(v6) = addr.ip() { if let Some(v4) = v6.to_ipv4_mapped() { return v4.is_loopback(); } }` path. Unit test covers.

### IMP-8. Warmup "warmup" reason tag dropped (Dim2 I1)
Upstream design kept explicit `"warmup"` prefix; PR-B2 spec simplified. Reviewer says keep — 5 LoC cost, preserves semantic distinction.
**Fix**: Keep prefix. Remove open-question #1.

### IMP-9. `chrono::Duration::from_std().unwrap_or(seconds(1))` fallback misleading (Dim2 I3)
If `effective_interval > i64::MAX` nanoseconds the fallback is wrong. But `INTERVAL_CEILING=60s` guarantees it never happens — so `.expect("clamped by INTERVAL_CEILING")` is clearer.
**Fix**: switch to `.expect(...)` pattern.

### IMP-10. `tokio::time::sleep` drift vs `tokio::time::interval` (Dim2 I6)
Upstream design used `tokio::time::interval + MissedTickBehavior::Skip`. Sleep-based drifts across iterations.
**Fix**: restore interval-based approach in §4.6 pseudocode.

### IMP-11. Plain `==` timing-side-channel (Dim3 I1)
DNS-rebind + `performance.now()` browser probes can byte-extract token even locally.
**Fix**: use `subtle::ConstantTimeEq` or `constant_time_eq`. One-line cost. If declined, spec must document accepted attack class.

### IMP-12. DNS rebinding / `:authority` validation missing (Dim3 I2)
Browser `fetch()` to attacker-controlled hostname that rebinds to 127.0.0.1 bypasses IP-level trust.
**Fix**: tower layer validating `:authority ∈ {"localhost", "127.0.0.1", "[::1]"}` before `subscribe_metrics` runs. Integration test.

### IMP-13. Token parsing inconsistent with REST + RFC 7235 (Dim3 I3)
REST at `lib.rs:524` uses `strip_prefix("Bearer ")` + `.trim()`. Spec's exact-match differs on `bearer <t>` (lowercase), double-space, trailing newline.
**Fix**: normalize as REST does. Matrix tests for each edge case.

### IMP-14. Empty/whitespace token silent auth-bypass (Dim3 I4)
`honor_opt_out(false, external, Some("Bearer "), Some(""))` with `format!("Bearer {t}") == h` → matches. Misconfigured empty token allows any `"Bearer "` header.
**Fix**: normalize `configured_token.filter(|t| !t.trim().is_empty())` → `None` before compare. Test.

### IMP-15. Kill-switch `Status::unimplemented` message leak (Dim3 I5)
Spec's message `"dashboard gRPC streaming disabled by config (grpc_streaming_enabled=false)"` leaks config field name.
**Fix**: neutral message like `"streaming disabled"`. Test `status.message()` has no config-field leak.

### IMP-16. `GrpcSpawnConfig` Debug leaks token (Dim3 I8)
Default `#[derive(Debug)]` on a struct containing `integration_auth_token: Option<String>`.
**Fix**: custom Debug impl redacting token as `[REDACTED]`. Test.

### IMP-17. `pii_sanitizer` forward-compat field — drop or guarantee (Dim3 I9)
Risk of PR-B3 shipping SubscribeEvents without actually wiring the sanitizer. Combined with CRIT-15 (phantom chain).
**Fix**: DROP `pii_sanitizer` from `GrpcSpawnConfig` in PR-B2. PR-B3 adds it back with wiring verified in that PR's tests.

### IMP-18. Active-stream cap scope (global vs per-RPC) ambiguous (Dim3 I10)
**Fix**: global `AtomicUsize` counter with internal per-RPC breakdown for observability. Cap check BEFORE auth gate AND before hint emission.

### IMP-19. `SystemMetrics` has no keystroke/click counters (Dim2 V1, existing code)
Current v2a `grpc/mod.rs:258-259` hardcodes `active_keystrokes: 0, active_mouse_clicks: 0`. Spec §3 MetricBucket shows these fields non-zero but doesn't say PR-B2 ships them as 0.
**Fix**: spec §3 + §4.1 + §4.6 explicit: `active_keystrokes` and `active_mouse_clicks` remain `0` in PR-B2 (parity with v2a aggregate path). Tracked as future work — separate task, not in PR-B2 scope.

### IMP-20. `aggregate_metrics_window` trait location (Dim5 #14)
Spec §10 claims `oneshim-web/src/storage_port.rs` (which is a re-export shim). Actual: `crates/oneshim-core/src/ports/web_storage.rs:416-420`. Signature is **synchronous** (no `#[async_trait]`).
**Fix**: spec §1 + §4.6 pseudocode explicit: trait is sync + must wrap in `spawn_blocking`.

### IMP-21. `event_tx` access chain wrong (Dim5 #7)
Spec implies `app_state.core.event_tx`. Actual: `core_resources.background_runtime.event_tx()` (method call at `app_runtime_launch.rs:243`).
**Fix**: spec §4.5 + §5 update to correct chain.

### IMP-22. `tokio::time::pause` + `wait_for_server_ready` ordering (Dim4 I1)
If pause is called before server-ready wait, virtual-time deadline fires instantly → panic.
**Fix**: spec §7 add subsection "virtual-time ordering" with recipe: `pick_free_port → spawn → wait_for_server_ready(real-clock) → pause() → exercise`.

### IMP-23. `MockSystemMonitor` "atomics-backed" — no AtomicF32 (Dim4 I2)
Literal atomic impl is 60+ LoC of `AtomicU32::from_bits/to_bits` noise. `parking_lot::Mutex<SystemMetrics>` is 15 LoC and workspace dep.
**Fix**: spec §4 + §7 switch to "parking_lot::Mutex<SystemMetrics>-backed".

### IMP-24. Back-to-back subscribe/drop cycle untested (Dim4 I3)
Counter-leak manifests only after ~50 reconnects.
**Fix**: Add test `grpc_dashboard_subscribe_metrics_survives_reconnect_cycle` — 10 cycles, assert counter baseline.

### IMP-25. `streaming_enabled` at-spawn-only semantics undocumented (Dim4 I4)
**Fix**: §2.5 or §4.4 note.

### IMP-26. CI estimate understated (Dim4 I5)
1.5s may balloon to 4s if 51-stream test runs on slow CI.
**Fix**: revise to "0.6s → 1.0s (core 15 tests); active-stream-cap test adds up to 0.5s"; ceiling 2.0s.

### IMP-27. SystemMonitor failure — 5s stale-tolerance (Dim2 I5 related)
Reviewer 2 suggests observability-only; Design §5 says 5s tolerance. Decide.
**Fix**: spec §4.6 item 5 decision — accept "yield Status::internal once, exit". Document rationale.

### IMP-28. `RecvError::Lagged` → re-collect metrics + re-query DB on every wake (Dim2 I4)
**Fix**: spec §11 risk note documenting sustained-lag self-paces via DB latency (accepted).

### IMP-29. Level-transition latency 1-tick smearing (Dim2 I7)
**Fix**: document explicitly in §4.6.

### IMP-30. HintEmitter yield-drop ordering safety assumption (Dim2 I2)
**Fix**: one-line comment in spec §4.2.

### IMP-31. `tracing-subscriber` + `serial_test` dev-dep additions (Dim4 V)
Cross-cuts IMP-11 / IMP-14 / CRIT-11.
**Fix**: pick final token-leakage approach; if (b) chosen (hand-audit + `#[instrument]` prohibition), no dev-deps needed.

## Minor findings (24 total — deferred unless cross-cutting)

Documented but not individually resolved in iter-2. Will revisit in iter-3 after Critical+Important are clean:
- M1 "strict protective fallthrough" wording → "disjunctive fallthrough"
- M2 `derive_percents` helper spec
- M3 `saturating_sub` comment wrong (guards wrap not panic)
- M4 `HEARTBEAT` const missing from §4.2 pseudocode
- M5 Rename `honor_opt_out` → `must_enforce`
- M6 `GrpcSpawnConfig` field ordering
- `emitted_at = Utc::now()` clock leak (loopback negligible)
- `debug_assert!` → `assert!` for threshold ordering
- `Status::internal(format!("spawn_blocking join: {e}"))` leaks task-id
- `min_free_mem_gb = 2.0` default degrades low-RAM machines
- hint `reason` formatting audit
- Heartbeat-N-th-fires unit test
- IPv6 loopback unit-only (already addressed by IMP-7)
- Float-comparison pattern (inherit v2a)
- `--no-default-features` feature-gate audit (addressed by CRIT-13 restructure)

## Decision gates — MUST resolve in iter-2

| # | Decision | Options | Recommendation |
|---|---|---|---|
| D1 | Kill-switch status code | `unimplemented` vs `unavailable` vs `failed_precondition` | `unavailable` (temporary disable semantics) |
| D2 | Trust policy | (a) "every loopback trusted" OR (b) "loopback AND token" | (a) simpler truth for v2b; v2c tightens |
| D3 | Token-leakage test | (a) dev-dep scaffold ~40 LoC OR (b) `#[instrument]` prohibition + hand-audit | (b) cheaper, broader coverage |
| D4 | DB failure handling | always-retry vs consecutive-counter | consecutive-counter N=5 emit degraded hint, N=10 close stream |
| D5 | SystemMonitor failure | (a) exit stream OR (b) 5s stale-tolerance | (a) exit — 40 LoC savings vs pathological case |
| D6 | `pii_sanitizer` forward-compat field | keep vs drop from PR-B2 | DROP — PR-B3 adds with wiring |
| D7 | Active-stream cap in PR-B2 vs PR-B3 | keep in PR-B2 | keep; reduce test from 51 to 6 streams |
| D8 | `MockSystemMonitor` location | `tests/common/` vs `src/grpc/test_support/` | `src/grpc/test_support/` under `#[cfg(any(test, feature = "test-support"))]` |
| D9 | Constant-time token compare | `subtle::ConstantTimeEq` vs plain `==` | use `subtle` — workspace dep check, likely already transitive via rcgen/rustls |
| D10 | DNS rebinding `:authority` validation | add tower layer vs defer v2c | add — 20 LoC, canonical attack class |

## Iter-2 plan

1. Apply all 15 CRIT fixes to spec
2. Apply all 31 IMP fixes to spec
3. Update §9 open-questions section — resolve D1-D10 in-line
4. Re-dispatch 5 reviewers for iter-2 convergence check
5. Only if iter-2 returns zero Crit + zero Important → Phase 1 done

Estimated spec edits: ~150 LoC insertions, ~50 LoC revisions.
