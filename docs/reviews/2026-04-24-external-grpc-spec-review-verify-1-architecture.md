# Spec Verify Review 1 — Architecture Lens

**Spec version**: rev-2 (commit `eb5e1958`)
**Round**: Loop 1 / Round 2 (verify)
**Prior review**: `2026-04-24-external-grpc-spec-review-1-architecture.md`

---

## Round-1 findings resolution status

### Resolved ✅

- **C1** (`DashboardServiceImpl` dual-mode). Spec §5.8 introduces the explicit `StreamingSource` enum (Fixed | Live) per D24 (table row §12), field swap documented at §4.2 row "`grpc/mod.rs` +50/-15" and §5.8 ("DashboardServiceImpl field change"). Loopback path constructs `Fixed`, external path constructs `Live(Arc<LiveExternalConfig>)`. Handler sites in `subscribe_metrics` / `subscribe_events` call `self.streaming_source.streaming_enabled()` / `.load_policy()` — single snapshot load per call satisfying D21. Type is Cloneable (`Arc<LiveExternalConfig>` is Clone; `Arc<LoadPolicy>` + `bool` in Fixed is Clone), so DashboardServiceImpl Clone bound preserved. ✅

- **C2** (`LoadPolicy::new` panics). §5.10 adds `LoadPolicy::try_new(thresholds) -> Result<Self, LoadPolicyError>` and `try_new_with_started_at(thresholds, started_at)`. Legacy `new` retained as `try_new(...).expect(...)` for boot — reasonable because boot config is already validated by `ConfigManager::update_with`. §5.4 `apply_config` uses `try_new_with_started_at` and partial-applies on `Err`: `streaming_enabled` still updates while `load_policy` is carried forward from `current.load_policy.clone()`. D23 + D27 locked in §12. ✅

- **C3** (two-store tear). §5.1 is fully rewritten — now a single `ArcSwap<LiveSnapshot>` where `LiveSnapshot { streaming_enabled: bool, load_policy: Arc<LoadPolicy> }` is the unit of atomic swap. Readers call `live.snapshot()` which returns `Arc<LiveSnapshot>` via `ArcSwap::load_full` (lock-free). NG7 rewritten at §3.2 to accurately describe the new "per-snapshot atomicity, per-call eventually-consistent" contract — old "NG7 lies" corrected. D21 locked. ✅

- **I1** (`http-body` + `pin-project-lite` not listed). D19 revised (§12 row) explicitly verifies `pin_project_lite` is transitive via `tokio/tower/hyper/http-body-util` — zero new dep. `http-body` itself is a tonic re-export (accessible as `tonic::body::Body`); §5.3 imports `http_body::{Body, Frame}` directly which will require an explicit `http-body = "1"` in `oneshim-web/Cargo.toml`. Spec does not list this crate in §4.3 or §4.4. Minor polish gap, not a blocker.

- **I2** (ConfigReloadTask supervisor tracking claim was false). §5.4 final paragraph ("Spawn site (D23)") explicitly places spawn in `build_external_spawn_config`, not `serve_external`, and documents the fire-and-forget pattern matching `spawn_cert_watcher` + `spawn_expiry_monitor`. D30 locked. §6.5 shows all 4 long-lived tasks responding to shutdown_rx. ✅

- **I3** (Body trait compatibility proof). §5.5 appends a compile-time assertion:
  ```rust
  const _: fn() = || {
      fn assert_body<T: http_body::Body + Send + 'static>() {}
      assert_body::<TrailerCapturingBody<tonic::body::Body>>();
  };
  ```
  Test placement stated in §9.1.3 (`trailer_body::tests`). ✅

- **I4** (deferred spawn regression for unary). §5.5 "Unary vs streaming latency" paragraph explicitly covers three cases: (1) unary `Err(Status)` trailers-only fires oneshot synchronously in `call` via header-first observation; (2) unary `Ok` writes data+trailer inline, `poll_frame` observes trailer on first poll; (3) streaming RPCs are the only long-tail case. D28 locked. ✅

- **I5** (overwrite-vs-append). §5.2 ingress logic now does conditional overwrite: if response already has `x-request-id` matching the validated value, no-op; otherwise insert. Preserves rare proxy-mirror patterns. D31 locked. Test bullet added to §9.5 (+1 for conditional-overwrite). ✅

- **I6** (spawn_config test staleness). §5.6 "Test updates required" explicit bullets:
  - add `Arc::ptr_eq(&cfg.live, &clone.live)` to `spawn_config_clone_is_shallow`
  - replace `streaming_enabled` substring check with `streaming_enabled_live` in `spawn_config_debug_redacts_sensitive_fields`
  ✅ (one nit noted in "New issues" below)

- **I7** (mod.rs declarations). §4.2 "Module declarations" block lists all 6 new `pub(crate) mod` lines (config_reload, live_config, live_config_handler, request_id_layer, streaming_source, trailer_body). ✅

- **Q1** (warmup reset). D27 locked: `LoadPolicy::started_at()` accessor + `try_new_with_started_at`. §5.4 `apply_config` captures `old_started_at = current.load_policy.started_at()` and threads through. §5.10 shows the symmetric pair (store + accessor). ✅

### Incomplete / partial ⚠️

None — all Round-1 C/I items are concretely addressed.

### New issues found in rev-2 edits

#### I8 (new, **Important**): Duplicate conflicting `§5.7` subsection — stale copy of `build_external_spawn_config` still present

**Location**: spec lines 720-762 (new rev-2 version, matches D30) **AND** lines 1028-1052 (stale copy — unchanged from rev-1).

**Issue**: Two `### 5.7 build_external_spawn_config` subsections coexist. The rev-1 stale version (L1028-1052) still stores `config_rx` on `ExternalGrpcSpawnConfig` (contradicting D30 + §5.6 spec body which removes it). It also shows `LiveExternalConfig::new(initial_streaming, initial_policy)` — a 2-arg constructor that no longer exists (new API at §5.1 is `LiveExternalConfig::new(initial: LiveSnapshot)`). An implementer reading top-to-bottom will hit the stale copy last and may take it as the authoritative one.

**Evidence**:
- L720-762: new version (correct — `LiveSnapshot`, D30 spawn site, `config_manager` param, no `config_rx` field)
- L1028-1052: stale version (incorrect — `config_rx` stored, 2-arg `LiveExternalConfig::new`, no spawn in builder)

**Suggested fix**: delete L1028-1052 (or collapse into a `### 5.12 (obsolete — see §5.7)` marker). This is a copy-paste oversight from the rev-2 edit pass.

#### I9 (new, **Minor→Important**): `§5.6` renames Debug field to `streaming_enabled_live`, but the struct no longer carries a bool field

**Location**: spec §5.6 "Debug impl adjustment" bullet (L713), §4.2 row "spawn_config.rs +15/-4" (L149).

**Issue**: §5.6 says *"Replace `.field("streaming_enabled", ...)` with `.field("streaming_enabled_live", &self.live.snapshot().streaming_enabled)`."* The struct field is now `live: Arc<LiveExternalConfig>` (L705). The Debug key `streaming_enabled_live` is fine as a human-readable label for the dynamically-read value, but:
1. The test assertion `spawn_config_debug_redacts_sensitive_fields` replacing `streaming_enabled` substring check with `streaming_enabled_live` will pass — but also the `load_policy` info is no longer in Debug (it was in rev-1 via separate field). Should §5.6 say whether `load_policy_snapshot` also appears in Debug?
2. Semantically, Debug output for a request-reloadable field is racy — two consecutive Debug prints may show different values. Acceptable for debug output but worth a one-line caveat.

**Suggested fix**: add to §5.6 Debug bullet: *"The Debug output includes both `streaming_enabled_live` and `load_policy_snapshot_summary` (just the threshold triplet), read from a single `self.live.snapshot()` call to avoid torn reads within one Debug print. Debug values are inherently racy across prints — documented."*

Reclassified to Important because this interacts with test semantics and impl reads that might vary by run.

#### I10 (new, **Important**): §5.11 `started_at_unix_ms` field in `LoadPolicyView` has placeholder pseudocode — mapping from `Instant` to Unix ms is undefined

**Location**: spec §5.11 line 1010: `started_at_unix_ms: /* elapsed-since-boot equivalent */,`

**Issue**: `std::time::Instant` is opaque — no direct conversion to Unix ms. A real impl needs one of:
1. Capture `SystemTime::now()` alongside `Instant::now()` in `LoadPolicy::try_new_with_started_at` (doubles the `LoadPolicy` field set — invasive but correct).
2. Compute "elapsed since boot" via `Instant::elapsed()` which returns `Duration` — gives seconds-since-policy-created, not a Unix timestamp.
3. Expose `elapsed_ms_since_started` instead of `started_at_unix_ms`.

Spec leaves this unresolved as placeholder pseudocode. An operator looking at `started_at_unix_ms = 0` after impl would be confused.

**Suggested fix**: decide in spec. Option (3) is cheapest: rename field to `elapsed_seconds` (or `elapsed_ms`), compute via `policy.started_at().elapsed().as_millis()`. Update §5.10 `LoadPolicy` struct, §5.11 handler, §9.2 test expectations.

#### I11 (new, **Minor**): §4.2 row "mod.rs" claims "+35/-5" but references 4-6 new mod lines — count collision with I7 module declarations (6 lines)

**Location**: §4.2 row for `grpc/external/mod.rs` (L151) says "+35/-5" with notes "`pub(crate) mod` lines for 4-6 new files (I7)".

**Issue**: I7 listed 6 module lines (§4.2 "Module declarations" block). The +35 LoC delta for `mod.rs` should be roughly: 6 new mod lines + layer stack changes + imports = ~15-20 LoC, not 35. If the intent is that the file gains more than just mod declarations (e.g., `serve_external` changes), those should be broken out — otherwise the LoC estimate looks padded.

**Suggested fix**: either tighten to "+20/-5" (if only mod + minor serve_external insert) or split row into two (one for mod declarations, one for `serve_external` layer-stack changes).

#### I12 (new, **Minor**): `LiveConfigResponse::load_policy_snapshot.in_warmup` calls `policy.is_in_warmup()` — is this method currently exposed?

**Location**: §5.11 L1011.

**Issue**: `LoadPolicy::is_in_warmup() -> bool` is referenced but not listed among the additions in §5.10. Current `load_policy.rs` has `in_warmup()` or similar internal gate — spec should explicitly enumerate the accessor surface added in this PR (started_at, is_in_warmup, thresholds).

**Suggested fix**: §5.10 should list all public accessors added: `started_at() -> Instant`, `thresholds() -> &LoadThresholds`, `is_in_warmup() -> bool` (if new). Avoids an impl-phase "is this method pub already?" question.

#### I13 (new, observational — **Minor**): §13 OQ13 raises handler-panic case but leaves it deferred

**Location**: §13 OQ13.

**Issue**: Spec acknowledges handler-panic path bypasses body-wrap (panic unwinds past `?` in `inner.call(req).await?`). This means panicked handlers *do not get any `Completed/Failed` audit row* — only Started. Deferring this is defensible (rare in practice, tonic has panic handler at outer layer), but the spec §6.1 data-flow diagram doesn't call out this path. At minimum a one-line disclaimer: *"Handler-panic case: panic propagates through `?` at `inner.call` await; Started row persists but Completed never recorded. Tracked as OQ13 / deferred."*

---

## Verdict

**CONDITIONAL-PASS**

Rationale: all 11 Round-1 items (C1-C3, I1-I7, Q1) are genuinely resolved with concrete, traceable fixes that hold up under close inspection. The `StreamingSource` enum + `ArcSwap<LiveSnapshot>` + `try_new_with_started_at` + D28 header-first path + D30 spawn-site move are all architecturally sound.

However, rev-2's +460-line expansion introduced 6 new issues (I8-I13). I8 (duplicate conflicting §5.7) and I10 (unresolved placeholder pseudocode in live-config endpoint) are genuinely Important — an implementer will be confused by I8 and stuck at I10. I9/I11/I12/I13 are polish-level.

None of I8-I13 is Critical. They can be fixed in a fast spec-polish pass without invalidating the plan-phase work. If the fixer addresses at minimum I8 (delete stale §5.7 copy) and I10 (decide `elapsed_ms` vs SystemTime), verdict upgrades to PASS.

Overall: architecture is in good shape; residual nits are cleanliness, not correctness.

---

*End of verify review 1 (architecture lens). ~1850 words.*
