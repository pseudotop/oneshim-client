# External gRPC Stress Test Suite — Design Spec

- **Date**: 2026-04-24
- **Status**: DRAFT (brainstorming output — awaiting implementation plan)
- **Author**: richard.kim0828@gmail.com
- **Branch**: `feature/grpc-stress-test-suite` (from `origin/main@5618558c`)
- **Related**:
  - PR #486 (D13 Task 13 external gRPC wiring) — merged at `5618558c`
  - `crates/oneshim-web/tests/external_grpc_integration.rs:1219-1225` — T13 deletion note
  - `crates/oneshim-web/tests/external_grpc_integration.rs:1294-1298` — T15 deletion note
  - `crates/oneshim-web/tests/external_grpc_integration.rs:1300-1307` — T16 deletion note
  - `crates/oneshim-web/src/grpc/external/ip_ban.rs:127-250` — existing IpBan unit tests (`ipv6_64_prefix_shared_ban` covers /64 prefix logic)
  - Sibling branch `feature/external-grpc-audit-liveconfig` (features2) — active parallel work to avoid conflicts with
  - Memory: `project_next_tasks.md`, `reference_ci_tauri_externalbin_stub.md`, `feedback_3loop_quality_gate.md`, `feedback_subagent_driven_catches_stale_plans.md`, `feedback_pipelined_reviews_pattern.md`, `feedback_squash_merge_cliff_skip.md`

---

## 1. Context & Problem

### 1.1 Trigger

PR #486 (merged at `5618558c`) completed D13 Task 13 external gRPC full wiring — real `DashboardServiceImpl`, `AuditLayer`, CLI dispatch, `CountingStream`, port-collision helper, and T14/T17/T18/T19 integration test un-ignore. The merge commit explicitly **deleted** three tests with reference comments in `crates/oneshim-web/tests/external_grpc_integration.rs`:

- **T13** (`external_grpc_ipv6_ban_uses_64_prefix`) — duplicate of unit tests in `ip_ban.rs`, deleted with note "add a scenario to the stress-test workflow instead" if full-stack coverage is desired.
- **T15** (`concurrent_connection_cap_enforced`) — deleted because it "require[s] dedicated CI workflow with elevated fd ulimit + opt-in trigger (`ulimit -n 65536`; separate workflow with manual dispatch)".
- **T16** (`external_grpc_task_panic_respawned`) — redundant with unit test `external::mod::tests::supervisor_respawns_on_injected_panic`.

This spec covers the **T15 rewrite at 1024+ connection scale** plus a **T13-derived IPv6 wiring scenario** placed inside the same stress workflow per the T13 deletion note's recommendation. T16 is **not** in scope (covered by unit test).

### 1.2 User scope decisions (brainstorming 2026-04-24)

Five binary decisions shaped this spec:

1. **Q1 scope** → **B'**: T15 stress + IPv6 full-stack scenario (NOT T13 rewrite per deletion note).
2. **Q2 purpose** → **(iv)**: correctness + fd resilience (excluding sustained-load/benchmark).
3. **Q3 location/gating** → **(A)**: new file `tests/external_grpc_stress.rs` gated by new cargo feature `stress-test`. Regular CI runs without the feature → file compiles to empty binary (zero overhead).
4. **Q4 workflow** → **(2)**: `workflow_dispatch` + weekly schedule `cron '0 3 * * 0'`, `ubuntu-latest` only.
5. **Q5 granularity** → **(B)**: 3 tests, separated for failure isolation:
   - `concurrent_connection_cap_enforced` (cap correctness)
   - `fd_pressure_resilience` (graceful degradation under churn)
   - `ipv6_64_prefix_ban_full_stack` (accept_loop wiring check)

### 1.3 Explicitly out of scope

- **Streaming shutdown observation** — `subscribe_events`/`subscribe_metrics` observing `shutdown_rx` to emit `Unavailable` on graceful shutdown. Overlaps with features2 (`audit_layer.rs` edits in progress). Defer to a separate PR after features2 lands.
- **Task 19 e2e tests** — `real_handler_returns_business_data`, `audit_completed_written`, `streaming_audit_records_count`. Different domain (business verification, not stress).
- **Sustained load / benchmarking** — continuous N conn/sec for M seconds. Genuine performance work; not correctness.
- **T13 regular-integration rewrite** — explicitly rejected by deletion comment ("not portable across CI runner configurations"). Only the full-stack **wiring** goes into the stress workflow as Test 3.
- **T16 rewrite** — already covered by `external::mod::tests::supervisor_respawns_on_injected_panic`.
- **Cross-platform (macOS/Windows) stress** — ubuntu-latest only (ulimit semantics diverge, marginal value).

---

## 2. Goals & Non-goals

### 2.1 Goals

- **G1**: Detect regressions in `max_connections` cap enforcement at the documented scale (1024).
- **G2**: Detect regressions in graceful connection cleanup (fd leak, accept_loop death) after high-load churn.
- **G3**: Detect regressions in `IpBan` accept_loop wiring on the IPv6 path (ban observed before TLS handshake).
- **G4**: Keep regular CI unchanged — stress tests strictly opt-in, isolated from `cargo test --workspace`.
- **G5**: Follow the project's 3-loop workflow (spec → plan → impl) and the PR-A / PR-B / PR-C subagent-driven-development precedent.

### 2.2 Non-goals

- **NG1**: Performance benchmarking. No pass/fail baseline for throughput/latency.
- **NG2**: Production code behavior changes. Test-only + CI infrastructure.
- **NG3**: Cross-platform coverage. Ubuntu-latest only.
- **NG4**: Sustained-load testing. One-shot stress per test.
- **NG5**: Rewriting unit-covered logic. `ip_ban.rs::ipv6_64_prefix_shared_ban` stays as the /64 prefix source of truth.

---

## 3. Architecture Overview

### 3.1 File layout

```
crates/oneshim-web/
├── Cargo.toml                            # +[features] stress-test = ["grpc-dashboard", "grpc-dashboard-external", "test-support"]
└── tests/
    └── external_grpc_stress.rs           # NEW. #![cfg(feature = "stress-test")]

.github/workflows/
└── grpc-stress.yml                       # NEW. workflow_dispatch + weekly schedule

docs/guides/
└── external-grpc.md                      # APPEND "Running stress tests locally" (~15 lines)
```

### 3.2 Cargo feature gating

```toml
# crates/oneshim-web/Cargo.toml
[features]
# existing features …
# stress-test pulls in the gRPC + external + test-support dependency chain
# so users only need to pass `--features stress-test` (not the full chain).
stress-test = ["grpc-dashboard", "grpc-dashboard-external", "test-support"]
```

File-level gate:

```rust
// tests/external_grpc_stress.rs
#![cfg(feature = "stress-test")]
// Without the feature, the file compiles to an empty integration test
// binary — zero overhead for `cargo test --workspace` without the flag.
```

### 3.3 Helpers strategy — local duplication

Rust integration tests (`tests/*.rs`) are separate binaries; helpers in one file are not directly importable by another. Options considered:

1. **Promote helpers to `external/test_support.rs`** (pub under `#[cfg(any(test, feature = "test-support"))]`) — already hosts `install_rustls_crypto_provider`, `test_jwt_keypair`, `test_cert_pair`, `test_ca_and_client_cert`. Adding `spawn_server`, `make_jwt_config` etc. would benefit `external_grpc_integration.rs` too (DRY win).
2. **Local duplication** — copy ~80 LoC of helpers into `external_grpc_stress.rs`.
3. **Shared `tests/support/` module** via `mod support;` pattern — awkward with Rust integration test crate model.

**Chosen: (2) Local duplication.** Rationale:
- Only 3 tests use these helpers. Extraction cost > reuse benefit (YAGNI).
- Keeps stress file self-contained; easy to read/maintain in isolation.
- Smaller blast radius: no touch to `external/test_support.rs` (shared with `external_grpc_integration.rs`).
- Re-evaluate promotion if the stress suite grows to ≥ 10 tests.

Duplicated helpers (shape mirrors `external_grpc_integration.rs`):
- `spawn_stress_server(cfg) -> (JoinHandle<()>, u16)` — bind to `127.0.0.1:0` (IPv4) or `[::1]:0` (IPv6), return port
- `make_jwt_stress_config(jwt_pub_key_path, max_connections, bind_addr)` — JWT-only, configurable cap + bind address
- `make_tls_channel(port, cert_pem, None)` — reuse existing (copy)
- `make_test_shutdown_pair()` — reuse existing (copy)

Helpers use existing `external::test_support` exports (`install_rustls_crypto_provider`, `test_jwt_keypair`, `test_mint_jwt`, `test_cert_pair`, `server_cert_pem`, `load_certified_key`, etc.) — no duplication of those.

---

## 4. Test Designs

### 4.1 Test 1: `concurrent_connection_cap_enforced`

**Invariant**: `max_connections = N` → N concurrent connections succeed; N+1 is rejected at the connection layer.

#### Setup

- `max_connections = 1024` (original T15 target scale)
- JWT-only auth mode
- Server bound to `127.0.0.1:0` (IPv4 loopback, OS-assigned port)

#### Phases

```
Phase 1: Open 1024 concurrent tonic channels, each holding one
         `subscribe_events` stream.
         - tokio::task::JoinSet manages channels + stream handles.
         - Each channel is a distinct tonic::transport::Channel
           (one TCP connection per channel — see §6 V3).
         - Expect: all 1024 streams established + initial frames
           received (or at minimum stream-open confirmation).

Phase 2: Attempt 1025th channel + stream.
         - Expect: rejection at connection layer.
         - Accepted outcomes: `Code::ResourceExhausted`,
           `Code::Unavailable`, OR transport-level connect error.
         - Test asserts the REJECTION (error present), not a
           specific code — robust against server-impl nuance.

Phase 3: Drop one Phase-1 stream + its channel.
         Poll for slot recovery (`active_connection_count <= 1023`
         or a fresh RPC round-trip succeeding), up to 5s.
         - Retry the 1025th channel open.
         - Expect: success (cap enforcement is dynamic).
```

#### Cleanup

- Drop all streams + channels; abort server handle; await join.

#### Runtime estimate

- Phase 1: ~2–5 s (1024 concurrent TLS handshakes + JWT verification + stream open)
- Phase 2: ~0.5–1 s
- Phase 3: ~1–2 s
- **Total: ~5–15 s**

#### fd requirement

- Minimum ~2048 fd (1024 connection fds + tokio + OS overhead). `ulimit -n 65536` provides 32× headroom.

---

### 4.2 Test 2: `fd_pressure_resilience`

**Invariant**: Repeated open/close cycles at maximum capacity do **not** leak fds and do **not** kill the accept loop.

#### Setup

- `max_connections = 1024`
- JWT-only auth
- IPv4 loopback

#### Phases

```
Loop (3 rounds):
  2a: Open 1024 concurrent streams.
  2b: Hold 200 ms.
  2c: Drop all streams + channels.
  2d: Wait up to 5 s for server-side cleanup. Completion signal:
      - `active_connection_count == 0` (preferred), OR
      - fresh unary RPC round-trip succeeds.

Post-loop verification:
  P1: Issue fresh unary RPC (`GetAgentInfo`).
       Expect: 200 OK with populated response (server alive).
  P2: Open 1024 new streams.
       Expect: all 1024 succeed (no residual fd leak).
```

#### Runtime estimate

- 3 × (~5 s max setup + 0.2 s hold + ~5 s max cleanup) + ~5 s verification
- **Total: ~20–35 s**

#### Regression targets

- accept_loop's `Drop` path on connection cleanup (semaphore / channel leaks)
- supervisor respawn fidelity — if accept loop silently dies, post-loop RPC fails
- tokio task leakage (spawned RPC handlers not joined on drop)

---

### 4.3 Test 3: `ipv6_64_prefix_ban_full_stack`

**Invariant**: `IpBan::record_failure` (on auth failure) + `IpBan::is_banned` (at accept_loop) are wired on the IPv6 path. After 5 auth failures from `[::1]`, the 6th TCP accept from `[::1]` is rejected **before** TLS handshake.

#### Setup

- `max_connections = 32` (small — avoid cap interference)
- JWT-only auth (invalid tokens trigger failures)
- Server bound to `[::1]:0` (IPv6 loopback)
- Fresh `IpBan::new()` in spawn config (per-test instance — no global state)
- Workflow runs with `--test-threads=1` (see §5), so serial execution is enforced at the runner level; no `#[serial_test::serial]` attribute needed. Per-instance `IpBan::new()` also makes cross-test pollution impossible.

#### Phases

```
Phase 1: 5 connection attempts with invalid JWT from [::1].
         - Each: TLS handshake succeeds → auth_layer rejects
           (Unauthenticated) → IpBan::record_failure increments.
         - After 5 failures in sliding window: banned_until set
           to +60 s (per THRESHOLDS ladder in ip_ban.rs).
         - Test asserts all 5 rejections with Code::Unauthenticated.

Phase 2: 6th connection attempt from [::1].
         - Expect: accept_loop observes IpBan::is_banned(addr)
           → true → immediate TCP close, no TLS handshake.
         - Client-side observation: connect or first RPC fails
           with transport error / Unavailable / Unknown.
         - Test asserts the FAILURE (any of the above); specific
           gRPC status not pinned (tonic version / transport
           layer surface differences).
```

#### Known limitations (documented, intentional)

1. **Single /128 only**: CI host loopback is a single `::1/128`. All 6 attempts share `::1/128`. The test verifies ban-within-same-/128 — a subset of the `/64 prefix` invariant. Full `/64`-sharing (different /128 within the same /64 both banned) is covered by `ip_ban.rs::ipv6_64_prefix_shared_ban` at lines 188-198. This integration test's contribution is **wiring verification** (ip_ban actually called in IPv6 accept path), not prefix logic.
2. **60s ban duration**: test does not wait for ban expiry — unrealistic for CI time. Expiration path is unit-tested separately.
3. **Shared loopback**: another process on the runner using the same port is prevented by OS-assigned port (0) and short test lifetime.

#### Runtime estimate

- Phase 1: ~1–2 s
- Phase 2: ~0.5–1 s
- **Total: ~2–5 s**

#### Pre-condition

See §6 V1. If accept_loop does not call `IpBan::is_banned` before TLS, Test 3 is either (a) scope-expanded to include the wiring fix, or (b) deferred to a follow-up PR. Decision point is C2 (pre-impl verification commit).

---

## 5. GHA Workflow Design

File: `.github/workflows/grpc-stress.yml`

```yaml
name: gRPC Stress Test

on:
  workflow_dispatch:
  schedule:
    - cron: '0 3 * * 0'  # Every Sunday 03:00 UTC

jobs:
  stress:
    runs-on: ubuntu-latest
    timeout-minutes: 15
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Print fd limits
        run: |
          echo "soft: $(ulimit -Sn)"
          echo "hard: $(ulimit -Hn)"
      - name: Install frontend dist stub (required by rust-embed in oneshim-web)
        run: |
          mkdir -p crates/oneshim-web/frontend/dist
          echo '<!doctype html>' > crates/oneshim-web/frontend/dist/index.html
      # No sandbox-worker stub needed: cargo test -p oneshim-web does not
      # compile src-tauri (dep graph is src-tauri → oneshim-web, not reverse).
      - name: Run stress tests
        run: |
          ulimit -n 65536
          cargo test -p oneshim-web \
            --features "stress-test" \
            --test external_grpc_stress \
            -- --test-threads=1 --nocapture
        env:
          RUST_BACKTRACE: 1
          RUST_LOG: info
```

### 5.1 Rationale

- **`workflow_dispatch`**: manual run for ad-hoc investigation + pre-merge dry run.
- **`schedule: '0 3 * * 0'`**: weekly Sunday 03:00 UTC — catches silent regressions without daily noise.
- **`ubuntu-latest`**: OS-agnostic at tokio/Rust level; ubuntu offers straightforward ulimit + IPv6 loopback.
- **`timeout-minutes: 15`**: ~60s test total + compile + cache-miss overhead. Comfortable buffer.
- **`--test-threads=1`**: prevents fd contention between tests (1 and 2 both consume 1024 fds; parallel run needs 2048+ and racy cleanup).
- **`--nocapture`**: preserve tracing output on failure for diagnostics.
- **Frontend/dist stub**: `crates/oneshim-web/frontend/dist/index.html` is `.gitignore`d (built artifact); without the stub, `rust-embed` fails at compile time. Per `reference_ci_tauri_externalbin_stub.md`. Sandbox-worker stub is NOT needed because `cargo test -p oneshim-web` does not compile `src-tauri`.
- **`Swatinem/rust-cache@v2`**: keep steady-state weekly run under ~5 min.
- **`ulimit -n 65536` inside the test step**: the raise must be in the same shell as `cargo test` (a separate step's shell is a new process without the raised limit).

---

## 6. Pre-implementation Verification (V1–V4)

Before implementing any test, the implementer **must** verify these assumptions against `origin/main` and document findings inline in commit C2 (see §7). The commit body quotes code (`file:line`) for each point.

| # | Assumption | Verification | Decision if false |
|---|-----------|--------------|-------------------|
| V1 | `accept_loop.rs` calls `IpBan::is_banned` **before** TLS handshake | `Grep 'is_banned\|ip_ban' crates/oneshim-web/src/grpc/external/accept_loop.rs` + read the accept path | **Scope expansion**: wiring implementation becomes part of this PR (new commit before C5). **Alternative**: defer Test 3 to a follow-up PR; CHANGELOG notes Test 3 absent. **User approval required before choosing.** |
| V2 | `max_connections` enforcement is in `accept_loop.rs` (not tonic-internal) and rejects at connection layer | `Grep 'max_connections\|semaphore\|connection_count' crates/oneshim-web/src/grpc/external/accept_loop.rs` | If tonic-internal: adjust Test 1 assertion to tonic's error shape. Document in C2 body. |
| V3 | `tonic::transport::Channel::connect` creates a distinct TCP connection per channel (not HTTP/2-multiplexed across channels) | tonic 0.14 docs + precedent: `external_grpc_integration.rs` creates per-channel TCP already (each `make_tls_channel` call → separate connection) | If default multiplexing: helper uses `Endpoint::connect()` explicitly per channel, instantiating N full `Channel`s with distinct underlying transports. Document findings in C2. |
| V4 | A server-side connection count accessor exists (for Phase 3 recovery poll) | `Grep 'active_connection_count\|connection_count\|active_streams' crates/oneshim-web/src/grpc/external/` | If missing: Phase 3 falls back to unary-RPC success polling (`GetAgentInfo` round-trip). Document in C2 + adjust Test 1 Phase 3 helper. |

---

## 7. Commit Plan

**Branch**: `feature/grpc-stress-test-suite` (created 2026-04-24 from `origin/main@5618558c`).

Commits land in order; each is self-contained, passes lefthook pre-commit (fmt + clippy + tests without the `stress-test` feature):

| # | Type | Subject | Content |
|---|------|---------|---------|
| C1 | `feat` | `add stress-test cargo feature + empty test file + GHA workflow` | `Cargo.toml` adds `stress-test = ["grpc-dashboard", "grpc-dashboard-external", "test-support"]`, `tests/external_grpc_stress.rs` with only `#![cfg(feature = "stress-test")]`, `.github/workflows/grpc-stress.yml`. Infrastructure only; no tests. |
| C2 | `docs` | `pre-implementation assumption verification (V1–V4)` | Amends spec with verification evidence: grep output + code quotes for V1–V4. If V1 fails, commit body documents the chosen path (scope expansion vs. Test 3 defer) with user approval thread referenced. |
| C3 | `test` | `concurrent_connection_cap_enforced (Test 1)` | Implements Test 1 + local helper duplicates (`spawn_stress_server`, `make_jwt_stress_config`, etc.). |
| C4 | `test` | `fd_pressure_resilience (Test 2)` | Implements Test 2. Reuses helpers from C3. |
| C5 | `test` | `ipv6_64_prefix_ban_full_stack (Test 3)` | Implements Test 3. Depends on V1 = pass. If V1 = fail (wiring missing) and user chose scope expansion in C2, this commit is preceded by a wiring-fix commit. |
| C6 | `docs` | `document stress test local run instructions` | Appends ~15-line section to `docs/guides/external-grpc.md`: `ulimit -n 65536 && cargo test -p oneshim-web --features "stress-test,..." --test external_grpc_stress`. |

**Pre-merge gate** (not a commit):

- `gh workflow run grpc-stress.yml --ref feature/grpc-stress-test-suite` — manual dispatch against the PR branch; all 3 tests green; run URL attached to PR description.

**Merge method**: **squash** (single-commit history per `feedback_squash_merge_cliff_skip.md`). Squash subject uses `test:` prefix so git-cliff produces a CHANGELOG entry.

---

## 8. Acceptance Criteria

- [ ] `cargo test -p oneshim-web --features "stress-test" --test external_grpc_stress -- --test-threads=1` passes locally.
- [ ] `gh workflow run grpc-stress.yml` (manual dispatch) runs to completion with all 3 tests green, before merge.
- [ ] Regular CI (`Test` workflow on PRs) unchanged — same test counts pre/post merge (diff of `cargo test --workspace` output).
- [ ] `cargo clippy --workspace --all-features -- -D warnings` clean (note: `--all-features` pulls in `stress-test` and compiles the stress file; clippy must pass there too).
- [ ] `cargo fmt --check` clean.
- [ ] `lefthook run pre-commit` passes at each commit.
- [ ] V1–V4 pre-implementation assumptions documented in C2 commit body with grep evidence.
- [ ] Weekly schedule registered (verified post-merge in the GHA "Scheduled" UI on the first Sunday).
- [ ] `docs/guides/external-grpc.md` has a reproducible local-run snippet.

No additional CHANGELOG handling needed beyond the squash subject (`test:` prefix → git-cliff picks up).

---

## 9. Risks & Mitigations

| # | Risk | Probability | Impact | Mitigation |
|---|------|------------|--------|------------|
| R1 | V1 fails — accept_loop doesn't call `IpBan::is_banned` before TLS | Medium | Scope expansion or Test 3 defer | C2 commit body documents finding; user approves scope decision before C5 lands. |
| R2 | tonic channel multiplexing (V3 fails) — 1024 "channels" share 1 TCP connection | Low–medium | Test 1/2 invalid | V3 verification enforces per-channel TCP via explicit `Endpoint::connect()` per channel. |
| R3 | GHA runner hard fd limit < 65536 | Low | Test 1/2 skipped | Workflow prints `ulimit -Hn` early; if < 2048, test `panic!`s with diagnostic "requires `ulimit -n 65536`". Fail-fast, not silent-skip. |
| R4 | features2 lands first and mutates `ExternalGrpcSpawnConfig` | Low | Rebase edits in test config calls | Mechanical rebase; no semantic coupling (§10). |
| R5 | Test 3 flakes from IpBan sliding-window reset | Low | Weekly CI noise | Fresh `IpBan::new()` per test (per-instance LruCache — no global state) + workflow `--test-threads=1` → deterministic state. Sliding window is short (60 s) so a single test run (~2–5 s) stays well inside one window. |
| R6 | Weekly schedule CI cost | Very low | Negligible | ~5 min/run steady-state = ~21 min/month. GHA free tier accommodates. |
| R7 | Tests pass locally but fail in GHA (env drift) | Medium | Delayed feedback | Pre-merge manual `gh workflow run` (§7) catches env drift before merge. |
| R8 | Stress test inadvertently detects unrelated bugs (e.g., pre-existing accept_loop race) | Medium | Scope creep | V1–V4 pre-impl verification surfaces issues early. User decides: fix-in-scope vs. separate PR. |

---

## 10. Coupling with features2 (`feature/external-grpc-audit-liveconfig`)

features2 is an active parallel branch (in plan-rework state per `project_next_tasks.md`) editing:

- `external/audit_layer.rs` — audit completeness
- `live_config.rs` — live config reload
- `request_id_layer.rs` — request ID propagation
- `ExternalGrpcConfig.streaming_enabled` — new field
- `audit` schema migration V32

### 10.1 File-level coupling matrix

| File | This PR | features2 | Conflict? |
|------|---------|-----------|-----------|
| `external/audit_layer.rs` | none | editing | ✅ zero |
| `live_config.rs` | none | editing | ✅ zero |
| `request_id_layer.rs` | none | editing | ✅ zero |
| `ExternalGrpcConfig` struct | none (consumed read-only) | adding field | ✅ zero (additive field; test config literals spread-default or explicit-init) |
| `crates/oneshim-web/tests/external_grpc_stress.rs` | create | unaware | ✅ zero |
| `.github/workflows/grpc-stress.yml` | create | unaware | ✅ zero |
| `crates/oneshim-web/Cargo.toml` `[features]` | add `stress-test = ["grpc-dashboard", "grpc-dashboard-external", "test-support"]` | unknown (likely unchanged) | ⚠️ trivial 1-line conflict possible |
| `docs/guides/external-grpc.md` | append section | unknown (may append) | ⚠️ trivial section-append conflict possible |

### 10.2 Semantic contract

This PR is **test-only**. No assertion on features2-owned semantics:

- No audit content/count assertions (features2 may change audit layer output).
- No live-config reload observation.
- No `streaming_enabled` toggling (test always uses default).

### 10.3 Merge-order contingency

- **This PR first**: features2 rebases on new main; trivial (adds `stress-test` feature entry).
- **features2 first**: this PR rebases on new main; trivial (re-add `stress-test` entry + possibly update test config literals if `ExternalGrpcSpawnConfig` gains non-default fields).

---

## 11. References

- **Code anchors** (verified against `origin/main@5618558c`):
  - `crates/oneshim-web/tests/external_grpc_integration.rs:1219-1225` — T13 deletion note with "add to stress workflow" guidance.
  - `crates/oneshim-web/tests/external_grpc_integration.rs:1294-1298` — T15 deletion note ("dedicated CI workflow with elevated fd ulimit").
  - `crates/oneshim-web/tests/external_grpc_integration.rs:1300-1307` — T16 deletion note (unit-test-covered).
  - `crates/oneshim-web/tests/external_grpc_integration.rs:970-974` — un-ignore summary comment.
  - `crates/oneshim-web/src/grpc/external/ip_ban.rs:13-125` — `BanKey`/`IpBan` implementation (IPv6 /64 prefix derivation at lines 23-29; `is_banned` at 71-83; `record_failure` at 85-109).
  - `crates/oneshim-web/src/grpc/external/ip_ban.rs:189-198` — `ipv6_64_prefix_shared_ban` unit test (the logic T3 full-stack wraps).
  - `crates/oneshim-web/src/grpc/external/` — directory layout (accept_loop, audit_bridge, audit_layer, auth_layer, cert_resolver, conn_info, ip_ban, jwt_verifier, metrics, mod, mtls_verifier, port_collision, spawn_config, test_support, tls_config).

- **Memory**:
  - `project_next_tasks.md` — 2026-04-24 handoff describing this task.
  - `reference_ci_tauri_externalbin_stub.md` — fresh-worktree sandbox-worker + frontend dist stubs (applied in §5 workflow).
  - `reference_parent_submodule_bump.md` — parent oneshim repo bump workflow (post-merge).
  - `feedback_3loop_quality_gate.md` — 3-loop workflow precedent.
  - `feedback_subagent_driven_catches_stale_plans.md` — catches plan-vs-code drift at implementation time.
  - `feedback_pipelined_reviews_pattern.md` — implementer FG + spec/quality BG review pattern for large PRs.
  - `feedback_squash_merge_cliff_skip.md` — squash + prefix rules for git-cliff CHANGELOG.

- **PRs & context**:
  - PR #486 (merged `5618558c`): D13 Task 13 external gRPC full wiring.
  - PR #487 (open, BLOCKED): Phase 9 PR-A tracking schedule (orthogonal; does not block this work).

---

## Appendix A — Command reference

**Local run**:

```bash
ulimit -n 65536
cargo test -p oneshim-web \
  --features "stress-test" \
  --test external_grpc_stress \
  -- --test-threads=1 --nocapture
```

**Pre-merge manual workflow dispatch**:

```bash
gh workflow run grpc-stress.yml --ref feature/grpc-stress-test-suite
gh run list --workflow=grpc-stress.yml --limit 1
```

**Verify regular CI unchanged**:

```bash
# On main:
cargo test --workspace 2>&1 | tail -20 > /tmp/main-tests.txt
# On this PR tip:
cargo test --workspace 2>&1 | tail -20 > /tmp/pr-tests.txt
diff /tmp/main-tests.txt /tmp/pr-tests.txt  # expect: zero diff in test counts
```
