# Spec Verify Review 3 — Platform & Risk Lens

**Spec version**: rev-2 (commit `eb5e1958`)
**Round**: Loop 1 / Round 2 (verify)
**Prior review**: `2026-04-24-external-grpc-spec-review-3-platform-risk.md`
**Synthesis applied**: `2026-04-24-external-grpc-spec-review-synthesis.md`

---

## Round-1 findings resolution status

### Resolved ✅

- **C1 (trailers-only response path)** — RESOLVED via **D28** (§12 L1510) + §5.5 pseudocode L614-637. Spec now reads `response.headers().get("grpc-status")` *before* body wrapping. If present → `tx.send(Some(code))` fires synchronously, wraps with `TrailerCapturingBody::new_already_fired`. If absent → normal trailer-observing wrap. I verified the tonic 0.14 source paths:
  - `tonic-0.14.5/src/server/grpc.rs:20` `t!` macro: `Err(status) => return status.into_http()` → trailers-only (grpc-status in initial headers, empty body).
  - `tonic-0.14.5/src/server/grpc.rs:447` `map_response` success path: `EncodeBody` emits body frames + trailer via `Frame::trailers(status.to_header_map())` at `codec/encode.rs:325`. **No grpc-status injected into initial headers in the success path** — so the two paths are cleanly disjoint. The spec's "header-first OR trailer-observing" dichotomy is sound.
  - Integration test `external_grpc_audit_denied_when_handler_returns_permission_denied` added to §9.2 L1364 directly exercises this path.

- **C2 (`grpc_streaming_enabled` shared with loopback)** — RESOLVED via **D22** + §7.1 + **NG1** rewrite (L68). New field `ExternalGrpcConfig.streaming_enabled: Option<bool>` added in `oneshim-core/src/config/sections/external_grpc.rs` (I verified the file exists at the path the spec's §4.2 component map cites). Fallback semantics `unwrap_or(cfg.web.grpc_streaming_enabled)` in `apply_config` L504-507 preserves backward compat. Integration test `loopback_streaming_enabled_is_not_live_reloaded` at §9.2 L1387 pins NG1.

- **C3 (`LoadPolicy::new` panics)** — RESOLVED via **D23** + §5.10. `LoadPolicy::try_new` / `try_new_with_started_at` return `Result<Self, LoadPolicyError>`. `apply_config` (L512-522) catches `Err`, logs, keeps previous policy (partial apply). Existing `LoadPolicy::new` retained as `try_new(...).expect(...)` wrapper — backward compat preserved; I verified the existing `impl` at `load_policy.rs:41-63` uses `assert!` trio that `try_new` replaces with explicit `Err` returns. Integration test `external_grpc_live_reload_rejects_malformed_thresholds_and_continues` at §9.2 L1377 covers.

- **I1 (two-store tear / NG7 false)** — RESOLVED via **D21** + §5.1. Single `ArcSwap<LiveSnapshot>` replaces dual `AtomicBool + ArcSwap<LoadPolicy>`. Readers use `snapshot()` → `Arc<LiveSnapshot>` (one lock-free load, single consistent view across `streaming_enabled` + `load_policy`). NG7 (L74) now correctly claims cross-field atomicity. Long-stream eventual-consistency caveat made explicit + pinned by `live_reload_affects_long_running_stream` test.

- **I2 (`config_rx` Debug impl)** — RESOLVED via **§5.6 L711-714**. `config_rx` is NOT stored on `ExternalGrpcSpawnConfig` at all (OQ7 resolution, D30) — the reload task owns its `Receiver` directly after spawning in `build_external_spawn_config`. The Debug concern vanishes because the field doesn't exist on the struct. Manual Debug impl already exists at `spawn_config.rs:82-103` per the spec; only the `streaming_enabled` field-name swap to `streaming_enabled_live` is needed (§5.6 L714). Test `spawn_config_debug_redacts_sensitive_fields` at §5.6 L718 updated.

- **I3 (unbounded deferred spawn)** — RESOLVED via **D32** (§12 L1514). Promoted from optional to in-scope: `ExternalMetrics.deferred_audit_in_flight: AtomicUsize` gauge + `config_reload_total: AtomicU64` + `config_reload_task_alive: AtomicBool`. §4.2 LoC envelope implicitly absorbs this (minor additions). Not as defensive as the reviewer's bounded-mpsc alternative (b), but the gauge satisfies "at minimum … acknowledge and justify" requirement.

- **I4 (G3 ≤5s)** — RESOLVED via **D33** (§12 L1515). G3 rewritten (§3 L62) — "one tokio scheduler tick, typically <10ms" + CI test asserts ≤1s (`live_reload_streaming_toggle_reflects_within_1s` at §9.2 L1375). Coalescing explicitly documented via `external_grpc_live_reload_coalesces_rapid_updates` test at §9.2 L1378.

- **I5 (UUID validator compat)** — RESOLVED via §5.2 L347. Added explicit note "UUIDv4 output (lowercase hex + hyphens, 36 chars) satisfies `is_valid` by construction". No behavioral change; narrative tightened.

### Incomplete / partial ⚠️

None. All Round-1 findings fully addressed.

### New issues found in rev-2 edits

#### N1 — Minor. `grpc_status_code: Option<u32>` cast path `observed.map(|c| c as i32 as u32)` in §5.5 L643 relies on enum-without-repr numeric cast

`tonic::Code` has no `#[repr(...)]` attr (verified at `tonic-0.14.5/src/status.rs:68`). Rust still permits `Code as i32` because every variant has an explicit discriminant (`Ok = 0`, `Cancelled = 1`, ..., `Unauthenticated = 16`). The `as i32 as u32` double-cast is well-defined since all discriminants are non-negative. However: the wire wants the gRPC status code integer; `c as i32` is already correct and `as u32` is a lossless widening. Not a bug, but the double cast is stylistically awkward. Plan phase should consider `i32::from(c) as u32` if `impl From<Code> for i32` exists (it does not in tonic 0.14 — only `From<i32> for Code`). Keep as `c as i32 as u32`; add a `// SAFETY: Code variants 0..=16, non-negative` comment.

#### N2 — Minor. §5.7 (L1028-1050) is a leftover rev-1 block contradicting rev-2

There are two `### 5.7` sections in the spec: the rev-2 one at L720-765 (correct — 2 new params, `config_manager` + `app_config_snapshot`, reload task spawned in-body) and a stale rev-1 remnant at L1028-1050 that shows `config_rx` being stored on `ExternalGrpcSpawnConfig` (contradicting D30). This is an editorial artifact from the synthesis pass. Plan phase MUST delete the stale L1028-1050 block to avoid reader confusion. Does not affect correctness of the approved design — just a doc-hygiene issue. Classify as **Minor** not Important because the authoritative §5.7 appears first and the stale one appears below the new content.

#### N3 — Minor. `LoadPolicyView::started_at_unix_ms` has no concrete computation path (§5.11 L1010)

The handler pseudocode has `started_at_unix_ms: /* elapsed-since-boot equivalent */,` — unwritten. `LoadPolicy::started_at()` returns `Instant` (monotonic, not wall-clock). Converting to a unix ms is either "wall-clock at boot + elapsed" (requires caching `SystemTime::now()` at boot) or "elapsed since boot, renamed to `started_at_elapsed_ms`". Plan phase must pick one. Not a blocker because the field is informational and the endpoint is not a correctness surface, but without resolution implementation will stall. Nudge: rename to `started_at_elapsed_ms: u64` (simpler, uses `Instant::elapsed().as_millis() as u64`) to avoid bringing `SystemTime` into the hot path.

#### N4 — Minor. `TrailerCapturingBody::new_already_fired` still requires `Drop` correctness proof

§5.3 L393-395: `pub fn new_already_fired(inner, captured) -> Self { Self { inner, signal: None, captured } }`. The `PinnedDrop` impl at L371-379 does `if let Some(tx) = this.signal.take() { let _ = tx.send(*this.captured); }` — since `signal: None` for the already-fired case, the `take()` yields `None`, the `if let` body never runs → no double-send, no panic. Verified the invariant holds: the spec handles the `None` signal case correctly via the `if let Some(tx) = ...` guard. Call out for reviewers who might worry but no change needed.

#### N5 — Minor. `StreamingSource::Clone` needed for `DashboardServiceImpl::Clone` — verify

tonic's generated `DashboardServiceServer<DashboardServiceImpl>` requires the inner service to be `Clone` (tonic per-request service cloning). §5.8 L781 declares `#[derive(Clone)]` on `StreamingSource`. Both variants are cheap to clone:
- `Fixed { streaming_enabled: bool, load_policy: Arc<LoadPolicy> }` — `bool: Copy`, `Arc::clone` cheap.
- `Live(Arc<LiveExternalConfig>)` — `Arc::clone` cheap.

`Arc<LiveExternalConfig>` Clone is trivial; no new synchronization cost. Satisfies tonic's per-request Clone pattern. Good.

#### N6 — Minor. `.context("Invalid LoadThresholds at boot…")?` in `build_external_spawn_config` (§5.7 L741-742) vs `.expect()`

Spec §5.10 L954-960 says `LoadPolicy::new` (retained as `try_new(...).expect(...)`) is "the boot-time entry point". But §5.7 L741-742 uses `LoadPolicy::try_new(...).context(...)?` — which propagates the Err up to the caller. This is actually **better** than `.expect()` because `build_external_spawn_config` returns `anyhow::Result` — the caller in `app_runtime_launch.rs` can degrade gracefully (log + fall back to external-grpc disabled mode) rather than panic the whole app. Small inconsistency with §5.10's "new as expect wrapper" narrative but the actual boot path uses `try_new` directly which is correct and safer. No change needed but plan phase could harmonize the doc.

#### N7 — Nit. `ConfigReloadTask` detached spawn in `build_external_spawn_config` (§5.7 L750-754)

```rust
tokio::spawn(run_config_reload(live.clone(), config_rx, shutdown_rx.clone()));
```

The returned `JoinHandle` is dropped. Task exits on shutdown_rx change or config_rx sender drop (L485-486 returns). **Intentional** per the comment (L546 rev-2 block: "reload_handle is fire-and-forget; task exits on shutdown_rx or config_rx-dropped"). The cert-watcher task in the existing codebase uses the same detachment pattern — matches precedent. D32 `config_reload_task_alive: AtomicBool` flips false on clean exit (§5.4 L494 comment) for observability. Good.

#### N8 — Minor. Layer ordering in §4.1 vs §4.2 LoC map

§4.1 L126 shows: `.layer(request_id_layer).layer(auth_layer).layer(audit_layer)` — 3 layers in this FIFO-on-ingress order. Per memory `reference_tonic_layer_order.md`, first `.layer()` is outermost. RequestIdLayer outermost (U5 ✓), AuthLayer second, AuditLayer innermost. Correct.

But: Round-1 platform Q2 noted `record_completion` signature mismatch. §4.2 L148 now says `audit_bridge.rs +15/-0` with "record/record_completion gain command_id: Option<String> arg (8th) + grpc_status_code: Option<u32> field". §5.5 L647-654 pseudocode shows **both** `command_id` and `grpc_status_code` as distinct params. The signature is expanding from `(ctx, remote, op, status, dur, msg_count, failure_reason)` = 7 args → +`command_id` + `grpc_status_code` = 9 args. Plan phase must pick:
- (a) keep signature linear with 9 positional args — adding 2 more pushes the total above common lint thresholds (most clippy configs warn at 7-8)
- (b) consolidate into a `CompletionRecord` struct to keep callsites readable

Inference: (a) is faster; (b) is cleaner. Not a blocker — either works; flag for plan-phase ergonomics decision.

#### N9 — Minor. `.as_str()` on `ctx.auth_type` (§5.5 L658) — verify existing API

`metrics.request_bump("external", ctx.auth_type.as_str(), audit_status_label(audit_status))` — assumes `AuthContext.auth_type` has an `as_str()` method returning `&'static str` (likely an enum with `const fn` mapping). Not verified but matches the existing pattern in `audit_layer.rs:139`. If the existing impl uses `String` or `&str` by construction, this is fine. Plan phase verify.

---

## New open questions the fixer did not re-flag

- **OQ14 (platform)**: `Arc<LoadPolicy>::started_at()` returns `Instant` by value (`Instant: Copy`). `apply_config` L511 `current.load_policy.started_at()` requires the `started_at` accessor on `LoadPolicy` (added per D27). Verified the accessor exists in the spec at §5.10 L950-952. But `Arc::started_at()` would need auto-deref to work — check that `LoadPolicy` is not wrapped in any decorator between `Arc<_>` and method call. Clean in this spec.

- **OQ15 (platform)**: `D26` claims `grpc_status_code: Option<u32>` with `#[serde(skip_serializing_if = "Option::is_none")]`. ExternalGrpcAuditDetails is serialized into the audit row's `details` JSON column. Backward compat: existing rows have no `grpc_status_code` field; deserializer must default to `None`. Requires `#[serde(default)]` on the struct. Not explicitly called out in the spec — plan phase must ensure `#[serde(default)]` is added alongside `skip_serializing_if`.

---

## Observability contract spot checks

- **D32 metrics**:
  - `deferred_audit_in_flight` — AtomicUsize, incremented before `tokio::spawn` in §5.5 L640 body. Decrement in task end. Not explicitly wired in §5.5 pseudocode — plan must insert `metrics.defer_in_flight_inc()` / `metrics.defer_in_flight_dec()` at spawn + end-of-async-block.
  - `config_reload_total` — AtomicU64, bumped after successful `apply_config`. Spec §5.4 L497-534 has `tracing::info!` at L530 but no counter bump. Plan must add `metrics.config_reload_inc()`.
  - `config_reload_task_alive` — AtomicBool, set true at task start, false on clean exit. Spec §5.4 L476 has `tracing::debug!` but no atomic store. Plan must add.

These are doc omissions that don't block the spec's architectural design but need plan-phase flesh-out. Not counted as "Important" because the metric types + semantic are locked in D32.

---

## Verdict

**PASS**

All 5 Critical and all 5 Important findings from Round 1 are concretely addressed in rev-2 with verifiable spec edits (§5.1, §5.4, §5.5, §5.10, §12 decisions D21-D33). The tonic 0.14 response path disjointness (trailers-only vs normal-trailer) is sound per verified source (`tonic-0.14.5/src/server/grpc.rs:20+447`, `codec/encode.rs:325`). `http::HeaderValue::from_str` acceptance range verified superset of `is_valid` predicate (`http-1.4.0/src/header/value.rs:552-553`). `tonic::Code::from_i32` infallibility verified (`status.rs:823`).

Nine new issues surfaced (N1-N9) are all Minor — doc-hygiene (stale §5.7 block, started_at_unix_ms ambiguity) or plan-phase completion items (metric wiring pseudocode, serde `default` attr, signature arity ergonomics). None require another spec iteration.

Two outstanding open-questions (OQ14, OQ15) are plan-phase concerns, not spec-level design gaps.

The spec is ready for Loop 2 (plan phase). Plan reviewers should focus on N2 (delete stale L1028-1050), N3 (resolve `started_at_unix_ms` computation), N8 (9-arg signature vs CompletionRecord struct), and OQ15 (serde `default` for backward-compat deserialization).

---

*End of verify review. Do not commit — main agent will bundle all 3 verify files.*
