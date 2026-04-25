# Spec Verify Review Round 3 — Architecture Lens

**Spec version**: rev-3 (commit `bee9b308`)
**Round**: Loop 1 / Round 3 (verify)
**Prior review**: `2026-04-24-external-grpc-spec-review-verify-1-architecture.md` (Round 2, CONDITIONAL-PASS)

---

## Round-2 findings resolution status

### Resolved ✅

- **I8** (duplicate `### 5.7` — stale copy L1028-1052). ✅ Resolved cleanly.
  Evidence: `grep -n "^### 5\." spec.md` now returns exactly **11 `### 5.x` headings**, one per subsection from §5.1 through §5.11. No duplicates remain. The rev-1 stale block (L1028-1052 with 2-arg `LiveExternalConfig::new` + stored `config_rx`) has been deleted. L1028 is now mid-body of §5.11 (live-config endpoint) — the authoritative content.

- **I9** (Debug impl — single-snapshot + load_policy_snapshot + racy-across-prints caveat). ✅ Resolved.
  §5.6 (L716-723) now explicitly encodes all three fixes:
  1. **Single snapshot**: *"Take a single `live.snapshot()` for all live-config Debug fields within one Debug print — avoids cross-field torn reads within a single `{:?}` output."* (L718)
  2. **Both fields in Debug**: emits `streaming_enabled_live` (from `snap.streaming_enabled`) AND `load_policy_snapshot_summary` with the threshold triplet formatted via `format_args!` (L720-721)
  3. **Racy caveat**: *"Racy across prints: Debug values reflect the snapshot at print-time; consecutive `{:?}` prints during a reload may show different values. Documented; acceptable for diagnostic output (not a correctness surface)."* (L723)
  Test update bullets (L725-727) are consistent with the renamed Debug field name.

- **I10** (`started_at_unix_ms` placeholder → monotonic). ✅ Resolved with the clean option (my suggested fix #3).
  §5.11 L1020-1023 now defines `started_at_elapsed_ms: u64` with clear docstring: *"Milliseconds since this LoadPolicy's warmup anchor (monotonic). Operators compute wall-clock origin as `now - started_at_elapsed_ms`. Monotonic-clock avoids SystemTime drift/suspend-resume weirdness."* Handler body at L1047 computes via `policy.started_at().elapsed().as_millis() as u64` — matches my recommendation verbatim. `grep` confirms **zero remaining references to `started_at_unix_ms`** anywhere in the spec (5 hits, all for the new `_elapsed_ms` name).

### Polish-level (Round 2 Minors)

- **I11** (LoC delta padding in `mod.rs` row): not touched, remains `+35/-5`. The spec author did not re-scope it. Not a correctness issue.
- **I12** (`LoadPolicy::is_in_warmup()` accessor disclosure): §5.10 still does not enumerate `is_in_warmup()` in its explicit accessor list. §5.11 uses `policy.is_in_warmup()` at L1056, and `grep` shows only that one call-site — so the impl-phase needs to confirm or add this accessor. Defer-to-impl. Not a blocker.
- **I13** (handler-panic disclaimer in §6.1): not added. Defer-to-impl (OQ13 tracks).

---

## New issues found in rev-3 edits

### N1 (new, **Minor**): §4.2 row at L154 still claims `config_rx` is stored on `ExternalGrpcSpawnConfig`

**Location**: L154
> *"`grpc/external/spawn_config.rs` +15/-4 | `streaming_enabled` + `load_policy` collapsed into `live: Arc<LiveExternalConfig>`; **new `config_rx: watch::Receiver<Arc<AppConfig>>`**; manual `Debug` impl updated for renamed fields"*

**Issue**: The boldfaced "new `config_rx`" clause directly contradicts:
- §5.6 (L711-712): *"Note: `config_rx` is NOT stored here (D30). The reload task is spawned in build_external_spawn_config and owns its Receiver directly."*
- §5.7 (L759-764): `config_rx` lives as a local binding in `build_external_spawn_config`, moved into `tokio::spawn(run_config_reload(...))`.
- Closed OQ7 (L1560): *"config_rx is no longer stored on ExternalGrpcSpawnConfig (D30); the reload task receives the Receiver directly at spawn time."*

This is a stale fragment carried over from the rev-1 → rev-2 edit pass that I missed calling out in Round 2 because it was consistent with rev-1. The rev-3 edits corrected §5.6 + §5.7 + OQ7 but not this table row. An implementer reading the component map will add a `config_rx` field to the struct only to discover in §5.6 that it shouldn't be there.

**Suggested fix**: delete the `"; new config_rx: watch::Receiver<Arc<AppConfig>>"` clause from L154. Leave the row as: *"…collapsed into `live: Arc<LiveExternalConfig>`; manual `Debug` impl updated."*

### N2 (new, **Minor**): §4.2 row at L154 also still says `+15/-4` which is likely slightly low given D30 spawn changes are in `app_runtime_launch.rs` (L165 row, +30/-10) — this is actually fine, just noting consistency

No change needed. The LoC estimates for `spawn_config.rs` are plausible after N1 fix (+15 could be: `live` field add + Debug impl rewrite + any required `config_rx` removal if there was previously one — though if it was never added, then the delta is just `+15/-4`).

### No other new issues

Cross-checks that passed:
- **Rename consistency**: `started_at_elapsed_ms` appears in struct def (L1023), docstring (L1021), handler computation (L1047), and JSON field in `LiveConfigResponse` (L1055). No dangling `started_at_unix_ms` anywhere.
- **§9.2 test list** (L1411-1413): mentions `live_config_endpoint_returns_current_snapshot` but does not pin the specific `started_at_elapsed_ms` assertion shape. Test-authoring discretion at impl time; spec is complete enough.
- **Debug impl pattern**: §5.6 correctly encodes single-snapshot read (one `live.snapshot()` binding `snap`, all field reads via `snap.*`). The `format_args!` approach for `load_policy_snapshot_summary` is efficient (no allocation on `redact` short-circuit paths).
- **§4.2 LoC total** (~1600, +300 vs rev-1): consistent with rev-2's breakdown. The new `grpc/external/live_config_handler.rs` row (L151, ~50 impl + ~60 test) is properly accounted for.
- **§4.2 Module declarations block** (L169-177): lists all 6 new `pub(crate) mod` entries matching the component map. Consistent.
- **OQ7/OQ10/OQ11 closed-status markers** at L1556-1567: accurately reflect spec body. No lingering "was-resolved-but-body-still-old" gaps.

---

## Verdict

**PASS**

Rationale: all three Important Round-2 findings (I8, I9, I10) are genuinely and cleanly resolved with concrete, implementable specifications. The rev-3 delta is surgical — the stale §5.7 block is deleted, the Debug impl is specified with appropriate torn-read avoidance and a racy-print caveat, and the Unix-ms placeholder is replaced with a monotonic `Instant::elapsed()` approach that matches my explicit Round-2 recommendation.

N1 is a Minor copy-paste residue on a single table row (L154) that does not affect implementation if the author or implementer reads §5.6 + §5.7 normatively (both explicit that `config_rx` is not stored). It will be caught at code-review time if missed during impl. I11-I13 remain deferred polish items that do not gate progression.

The architecture is now in solid shape:
- `ArcSwap<LiveSnapshot>` single-store (C3/I9 resolved, torn-read semantics documented)
- `StreamingSource::{Fixed, Live}` dual-mode (C1 resolved)
- `LoadPolicy::try_new` + `try_new_with_started_at` + `started_at()` accessor (C2, Q1 resolved)
- Header-first gRPC status observation for unary-Err / trailers-only (I4/D28 resolved)
- `ConfigReloadTask` spawned in builder, not stored on spawn_config (I2/D30 resolved)
- Live-config REST endpoint with monotonic `started_at_elapsed_ms` + `config_reload_task_alive` task-liveness surface (I10/D29/NV2 resolved)
- `RequestIdLayer` conditional-overwrite preserving proxy-mirror patterns (I5/D31 resolved)
- `TrailerCapturingBody` with compile-time `Body` trait assertion (I3 resolved)

Ready to proceed to Loop 2 (plan-phase). N1 can be fixed opportunistically in a doc-only commit alongside the plan or as part of impl.

---

*End of verify review round 3 (architecture lens). ~1050 words.*
