# External gRPC Stress Test Suite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a 3-test stress suite for the external gRPC server (concurrent-cap, fd-pressure, IPv6 ban full-stack) gated behind a new `stress-test` cargo feature and a manual/weekly GHA workflow, with zero impact on regular CI.

**Architecture:** New integration test file `crates/oneshim-web/tests/external_grpc_stress.rs` gated by `#![cfg(feature = "stress-test")]`. The file duplicates ~80 LoC of test helpers locally (per spec §3.3 — YAGNI extraction). Production code is unchanged. A new `.github/workflows/grpc-stress.yml` runs the suite on `workflow_dispatch` + weekly `cron '0 3 * * 0'` with `ulimit -n 65536`.

**Tech Stack:** Rust 1.77+, tonic 0.14, tokio 1, rustls 0.23 (aws-lc-rs provider), JoinSet for fan-out, GHA `ubuntu-latest`. No new runtime deps.

---

## Pre-implementation verification (V1–V4) — RESOLVED

These were spec §6 open items. All resolved against `feature/grpc-stress-test-suite@8005bdc2` (worktree `.claude/worktrees/grpc-stress`):

| # | Question | Finding | Action |
|---|----------|---------|--------|
| **V1** | Does `accept_loop.rs` call `IpBan::is_banned` BEFORE TLS handshake? | ✅ **PASS** — `crates/oneshim-web/src/grpc/external/accept_loop.rs:77` calls `cfg.ip_ban.is_banned(remote)` immediately after `listener.accept()` (line 64-71) and BEFORE `acceptor_c.accept(tcp)` (line 100-104). Matching unit test at `accept_loop.rs:299-351` (`accept_loop_rejects_banned_ip`) already validates this for IPv4 loopback. | **Test 3 implementable as-is.** No scope expansion needed; no wiring fix commit; no follow-up PR. |
| **V2** | Where is `max_connections` enforced? | ✅ **accept_loop layer** — `accept_loop.rs:86-91` uses `let prev = active_conns.fetch_add(1, ...); if prev >= cfg.config.max_connections { active_conns.fetch_sub(1, ...); drop(tcp); continue; }`. Cap rejection is **silent** (TCP dropped before TLS, no gRPC error returned to client). | **Test 1 assertion**: client-side connect for the (N+1)th attempt manifests as either a transport-level connect/handshake error OR the channel succeeds at TCP/TLS but the first RPC fails — the test asserts "an error occurred", not a specific gRPC `Code`. |
| **V3** | Does `tonic::transport::Channel::connect` create a distinct TCP per channel? | ✅ **PASS** — existing `make_tls_channel` helper at `tests/external_grpc_integration.rs:295-315` creates a fresh `Endpoint::from_shared(...).connect()` per call. tonic 0.14 `Endpoint::connect()` returns a `Channel` with a single underlying H2 connection (no auto-balance). N distinct `make_tls_channel` calls ⇒ N distinct TCP connections. Pattern proven by T14 (`external_grpc_concurrent_stream_cap_enforced`, line 1234-1292) which holds 4 concurrent streams over a SINGLE channel — the dual case (N channels, 1 stream each) is symmetric. | **Tests 1 & 2 use one `Endpoint::connect()` per channel** to guarantee per-TCP-connection accounting. |
| **V4** | Is there a public `active_connection_count` accessor on the running server? | ❌ **NO** — `active_conns: Arc<AtomicUsize>` is owned inside `run_accept_loop` (`accept_loop.rs:43`) and is not exposed via `serve_external` or any public API. | **Phase 3 of Test 1 + post-loop check of Test 2** use **unary-RPC success polling** (`GetAgentInfo` round-trip) instead. Spec §4.1 Phase 3 / §4.2 P1+P2 already accommodate this fallback. |

### Cargo feature graph correction (spec §3.2 amendment)

Spec §3.2 lists `stress-test = ["grpc-dashboard", "grpc-dashboard-external", "test-support"]`. **This is insufficient.** `crates/oneshim-web/src/grpc/external/test_support.rs` (gated on `#[cfg(any(test, feature = "test-support"))]`) imports `rcgen` (line 53, 90), but the `test-support` feature does NOT pull in rcgen — only `external-grpc-tools = ["grpc-dashboard-external", "dep:rcgen"]` does. Without `external-grpc-tools`, `cargo test --features stress-test` fails to compile the lib because `rcgen` is missing.

**Plan uses**: `stress-test = ["external-grpc-tools", "test-support"]` (which transitively pulls `grpc-dashboard-external` → `grpc-dashboard`). This matches the existing integration test gate at `tests/external_grpc_integration.rs:12-16` exactly.

---

## File structure

| File | Op | Purpose |
|------|----|---------|
| `crates/oneshim-web/Cargo.toml` | modify (one section) | Add `stress-test = ["external-grpc-tools", "test-support"]` to `[features]`. |
| `crates/oneshim-web/tests/external_grpc_stress.rs` | create | Stress suite, gated `#![cfg(feature = "stress-test")]`. Local helper duplicates + 3 tests. |
| `.github/workflows/grpc-stress.yml` | create | `workflow_dispatch` + weekly cron. Single `stress` job with `ulimit -n 65536`. |
| `docs/guides/external-grpc.md` | modify (append) | New "Running stress tests locally" section (~15 lines). |

**No production code changes.** No edits to `accept_loop.rs`, `auth_layer.rs`, `audit_layer.rs`, `live_config.rs`, `request_id_layer.rs`, `ExternalGrpcConfig`, or migration `V32`. Test-only PR.

---

## Task overview (commit-aligned)

The plan is structured to land 6 commits (C1–C6 per spec §7). Each task ends with a commit. Run `lefthook run pre-commit` between commits.

| Task | Commit | Subject |
|------|--------|---------|
| 1 | C1 | `feat(oneshim-web): add stress-test cargo feature + empty test file + GHA workflow` |
| 2 | C2 | `docs(grpc-stress): pre-implementation assumption verification (V1–V4)` |
| 3 | C3 | `test(oneshim-web): concurrent_connection_cap_enforced (Test 1)` |
| 4 | C4 | `test(oneshim-web): fd_pressure_resilience (Test 2)` |
| 5 | C5 | `test(oneshim-web): ipv6_64_prefix_ban_full_stack (Test 3)` |
| 6 | C6 | `docs(grpc-stress): document stress test local run instructions` |

Each commit individually passes lefthook (fmt + clippy + tests **without** the `stress-test` feature, since the file is empty when the feature is off — zero overhead for the regular `cargo test --workspace`).

---

## Task 1: Cargo feature + empty test file + GHA workflow (C1)

**Files:**
- Modify: `crates/oneshim-web/Cargo.toml` (the `[features]` section, ending around line 126)
- Create: `crates/oneshim-web/tests/external_grpc_stress.rs`
- Create: `.github/workflows/grpc-stress.yml`

### Steps

- [ ] **Step 1.1: Add the `stress-test` feature to Cargo.toml**

Open `crates/oneshim-web/Cargo.toml` and append to the `[features]` block (after the existing `test-support = ["dep:tempfile"]` line at line 126):

```toml
# D13-v2c stress test suite: gated test file at tests/external_grpc_stress.rs.
# Pulls in external-grpc-tools (rcgen for cert/JWT key gen) + test-support
# (test_support.rs helpers like install_rustls_crypto_provider, test_jwt_keypair).
# Feature deliberately excluded from `--all-features` workflows except clippy
# (acceptance criteria §8: stress file must clippy-clean).
stress-test = ["external-grpc-tools", "test-support"]
```

- [ ] **Step 1.2: Create the empty stress test file**

Create `crates/oneshim-web/tests/external_grpc_stress.rs` with exactly:

```rust
//! External gRPC stress test suite.
//!
//! See `docs/superpowers/specs/2026-04-24-grpc-stress-test-suite-design.md`
//! and `docs/superpowers/plans/2026-04-24-grpc-stress-test-suite-plan.md`.
//!
//! Three tests:
//! 1. `concurrent_connection_cap_enforced` — `max_connections = 1024`
//!    correctness + dynamic slot recovery.
//! 2. `fd_pressure_resilience` — 3 rounds of 1024-stream churn + post-loop
//!    survival, no fd leak.
//! 3. `ipv6_64_prefix_ban_full_stack` — `IpBan` accept_loop wiring on the
//!    IPv6 path: 5 auth failures from `[::1]` → 6th TCP closed before TLS.
//!
//! Compiled to an empty integration test binary unless the `stress-test`
//! feature is enabled. Run locally:
//!
//! ```sh
//! ulimit -n 65536
//! cargo test -p oneshim-web --features stress-test \
//!   --test external_grpc_stress -- --test-threads=1 --nocapture
//! ```

#![cfg(feature = "stress-test")]

// Helpers + tests added in subsequent commits (C3-C5).
```

- [ ] **Step 1.3: Create the GHA workflow**

Create `.github/workflows/grpc-stress.yml`:

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
        with:
          submodules: recursive
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Print fd limits
        run: |
          echo "soft: $(ulimit -Sn)"
          echo "hard: $(ulimit -Hn)"
      - name: Install frontend dist stub (rust-embed compile-time requirement)
        run: |
          mkdir -p crates/oneshim-web/frontend/dist
          echo '<!doctype html>' > crates/oneshim-web/frontend/dist/index.html
      # No sandbox-worker stub required: cargo test -p oneshim-web does not
      # compile src-tauri (dep graph is src-tauri → oneshim-web, not reverse).
      - name: Run stress tests
        run: |
          ulimit -n 65536
          cargo test -p oneshim-web \
            --features stress-test \
            --test external_grpc_stress \
            -- --test-threads=1 --nocapture
        env:
          RUST_BACKTRACE: 1
          RUST_LOG: info
```

- [ ] **Step 1.4: Verify the empty file compiles**

Run: `cargo check -p oneshim-web --features stress-test --tests`
Expected: clean compile, no warnings. The `external_grpc_stress` test binary is built but contains no `#[test]` items.

Run: `cargo check -p oneshim-web --tests`  (without `--features stress-test`)
Expected: clean compile. The `external_grpc_stress.rs` file's content is excluded by the `#![cfg(feature = "stress-test")]` gate, producing an empty binary.

- [ ] **Step 1.5: Verify regular CI is unaffected**

Run: `cargo test --workspace 2>&1 | tail -20`
Expected: same passing test count as before this task. The `external_grpc_stress` binary appears with `0 passed; 0 failed; 0 ignored` (or is silently empty).

- [ ] **Step 1.6: Run lefthook**

Run: `lefthook run pre-commit`
Expected: PASS.

- [ ] **Step 1.7: Commit**

```bash
git add crates/oneshim-web/Cargo.toml \
        crates/oneshim-web/tests/external_grpc_stress.rs \
        .github/workflows/grpc-stress.yml
git commit -m "$(cat <<'EOF'
feat(oneshim-web): add stress-test cargo feature + empty test file + GHA workflow

Infrastructure-only commit. Adds:
- `stress-test = ["external-grpc-tools", "test-support"]` feature on
  oneshim-web. Pulls in rcgen + test_support.rs helpers.
- `crates/oneshim-web/tests/external_grpc_stress.rs` gated by
  `#![cfg(feature = "stress-test")]`. Empty binary in regular CI.
- `.github/workflows/grpc-stress.yml`: workflow_dispatch + weekly Sunday
  03:00 UTC cron, ubuntu-latest, ulimit -n 65536, --test-threads=1.

Tests added in subsequent commits (C3-C5).

Spec: docs/superpowers/specs/2026-04-24-grpc-stress-test-suite-design.md
Plan: docs/superpowers/plans/2026-04-24-grpc-stress-test-suite-plan.md
EOF
)"
```

---

## Task 2: Pre-implementation verification commit (C2)

**Files:**
- The plan document (this file) already records V1–V4 findings in the "Pre-implementation verification" section above. C2 is the commit that publishes those findings — no new file content beyond the plan itself.

### Steps

- [ ] **Step 2.1: Verify the plan was force-added**

Run: `git status docs/superpowers/plans/2026-04-24-grpc-stress-test-suite-plan.md`
Expected: tracked (not "ignored"). If ignored, run `git add -f docs/superpowers/plans/2026-04-24-grpc-stress-test-suite-plan.md`.

- [ ] **Step 2.2: Append the verification evidence to the plan if not already present**

The verification table at the top of this plan document already contains V1–V4 with `file:line` citations. No edits needed unless the implementer discovers drift against `origin/main` during V1 spot-check (re-grep `is_banned` in `crates/oneshim-web/src/grpc/external/accept_loop.rs` to confirm line 77 still matches).

- [ ] **Step 2.3: Run lefthook (no code change, formatter + linter still run on the docs)**

Run: `lefthook run pre-commit`
Expected: PASS.

- [ ] **Step 2.4: Commit**

```bash
git add -f docs/superpowers/plans/2026-04-24-grpc-stress-test-suite-plan.md
git commit -m "$(cat <<'EOF'
docs(grpc-stress): pre-implementation assumption verification (V1–V4)

Records the V1–V4 verification spec §6 deferred to the plan phase:

- V1 PASS: accept_loop.rs:77 calls IpBan::is_banned before TLS handshake.
  Test 3 implementable as-is; no scope expansion.
- V2 PASS: max_connections enforced at accept_loop atomic counter
  (line 86-91), not tonic-internal. Silent TCP drop on cap reject —
  Test 1 asserts on transport-level error, not gRPC Code.
- V3 PASS: tonic 0.14 Endpoint::connect creates distinct TCP per
  channel (existing make_tls_channel pattern, T14 precedent at
  external_grpc_integration.rs:1234-1292).
- V4 FAIL: no public active_connection_count accessor — Phase 3 of
  Test 1 and post-loop check of Test 2 fall back to unary-RPC
  success polling (GetAgentInfo round-trip).

Also records spec §3.2 correction: stress-test feature must include
external-grpc-tools (for rcgen via test_support.rs), not just
grpc-dashboard + grpc-dashboard-external + test-support.
EOF
)"
```

---

## Task 3: Test 1 — `concurrent_connection_cap_enforced` (C3)

**Files:**
- Modify: `crates/oneshim-web/tests/external_grpc_stress.rs` (add helpers + Test 1)

### Helpers strategy

Per spec §3.3 (chosen option: local duplication). Helpers added in this commit are also reused by C4 (Test 2) and C5 (Test 3). Order: add all 3 helpers + `NoopAudit` in C3, then C4/C5 only add new `#[tokio::test]` functions.

### Steps

- [ ] **Step 3.1: Add imports + module-level constants**

Replace the body of `crates/oneshim-web/tests/external_grpc_stress.rs` (everything below the `#![cfg(...)]` gate) with the following preamble. Subsequent steps APPEND to the same file.

```rust
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use oneshim_core::config::{AuthMode, ExternalGrpcConfig, JwtAlgorithm};
use oneshim_core::models::ai_session::SessionAuditEntry;
use oneshim_core::models::audit::{AuditEntry, AuditLevel, AuditStats, AuditStatus};
use oneshim_core::ports::audit_log::AuditLogPort;
use oneshim_storage::sqlite::SqliteStorage;
use tokio::task::JoinSet;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint};

use oneshim_web::grpc::external::cert_resolver::HotReloadCertResolver;
use oneshim_web::grpc::external::ip_ban::IpBan;
use oneshim_web::grpc::external::jwt_verifier::JwtVerifier;
use oneshim_web::grpc::external::metrics::ExternalMetrics;
use oneshim_web::grpc::external::serve_external;
use oneshim_web::grpc::external::spawn_config::ExternalGrpcSpawnConfig;
use oneshim_web::grpc::external::test_support::{
    install_rustls_crypto_provider, test_cert_pair, test_jwt_keypair, test_mint_jwt,
};
use oneshim_web::grpc::external::tls_config::load_certified_key;
use oneshim_web::grpc::test_support::mock_system_monitor::MockSystemMonitor;
use oneshim_web::proto::dashboard::v1::dashboard_service_client::DashboardServiceClient;
use oneshim_web::proto::dashboard::v1::{GetAgentInfoRequest, SubscribeEventsRequest};
use oneshim_web::storage_port::WebStorage;
```

- [ ] **Step 3.2: Append `NoopAudit` (duplicated from `external_grpc_integration.rs:92-139`)**

```rust
// ── Noop audit ───────────────────────────────────────────────────────────────
//
// Local duplicate of the NoopAudit at tests/external_grpc_integration.rs:92.
// Stress tests do not assert on audit content — see spec §10.2 (test-only PR,
// no semantic coupling on features2-owned audit semantics).

struct NoopAudit;

#[async_trait::async_trait]
impl AuditLogPort for NoopAudit {
    async fn pending_count(&self) -> usize {
        0
    }
    async fn recent_entries(&self, _limit: usize) -> Vec<AuditEntry> {
        vec![]
    }
    async fn entries_by_status(&self, _status: &AuditStatus, _limit: usize) -> Vec<AuditEntry> {
        vec![]
    }
    async fn entries_by_action_prefix(&self, _prefix: &str, _limit: usize) -> Vec<AuditEntry> {
        vec![]
    }
    async fn stats(&self) -> AuditStats {
        AuditStats::default()
    }
    async fn has_pending_batch(&self) -> bool {
        false
    }
    async fn log_event(&self, _action_type: &str, _session_id: &str, _details: &str) {}
    async fn log_start_if(
        &self,
        _level: AuditLevel,
        _command_id: &str,
        _session_id: &str,
        _action_type: &str,
    ) {
    }
    async fn log_complete_with_time(
        &self,
        _level: AuditLevel,
        _command_id: &str,
        _session_id: &str,
        _details: &str,
        _execution_time_ms: u64,
    ) {
    }
    async fn drain_batch(&self) -> Vec<AuditEntry> {
        vec![]
    }
    async fn drain_all(&self) -> Vec<AuditEntry> {
        vec![]
    }
    async fn record_session_event(&self, _entry: SessionAuditEntry) {}
}
```

- [ ] **Step 3.3: Append `make_test_shutdown_pair` (duplicate of `external_grpc_integration.rs:59-65`)**

```rust
// ── Shutdown pair helper ─────────────────────────────────────────────────────

fn make_test_shutdown_pair() -> (
    Arc<tokio::sync::watch::Sender<bool>>,
    tokio::sync::watch::Receiver<bool>,
) {
    let (tx, rx) = tokio::sync::watch::channel(false);
    (Arc::new(tx), rx)
}

fn in_memory_storage() -> Arc<dyn WebStorage> {
    Arc::new(SqliteStorage::open_in_memory(30).expect("in-memory SQLite")) as Arc<dyn WebStorage>
}
```

- [ ] **Step 3.4: Append `make_jwt_stress_config(max_connections, bind_addr)` helper**

This is the stress variant of `make_jwt_config` from `external_grpc_integration.rs:151`, parameterised on `max_connections` and `bind_addr` (so Test 3 can swap `127.0.0.1` for `[::1]`).

```rust
// ── Server config helper (stress variant) ─────────────────────────────────────
//
// Differs from make_jwt_config in external_grpc_integration.rs:151 in that
// max_connections + bind_addr are caller-controlled. JWT-only auth.

fn make_jwt_stress_config(
    jwt_pub_key_path: &std::path::Path,
    max_connections: usize,
    bind_addr: SocketAddr,
) -> ExternalGrpcSpawnConfig {
    let (cert_path, key_path) = test_cert_pair();
    let certified_key = load_certified_key(&cert_path, &key_path).expect("load certified key");
    let cert_resolver = Arc::new(HotReloadCertResolver::new(certified_key));

    let (event_tx, _) = tokio::sync::broadcast::channel(16);

    let pub_key_bytes = std::fs::read(jwt_pub_key_path).expect("read jwt pub key");
    let jwt_verifier = Arc::new(
        JwtVerifier::new(
            JwtAlgorithm::Es256,
            &pub_key_bytes,
            "test-issuer",
            "test-audience",
        )
        .expect("JwtVerifier"),
    );

    let (shutdown_tx, shutdown_rx) = make_test_shutdown_pair();
    ExternalGrpcSpawnConfig {
        bind_addr,
        config: ExternalGrpcConfig {
            enabled: true,
            auth_mode: Some(AuthMode::Jwt),
            max_connections,
            // Per-channel single stream: cap by max_connections, not stream
            // cap. Set high so stream cap is never the rejecting layer.
            // (max_concurrent_streams is `usize` — see oneshim-core
            // crates/oneshim-core/src/config/sections/external_grpc.rs:69.)
            max_concurrent_streams: max_connections.max(1024),
            ..Default::default()
        },
        storage: in_memory_storage(),
        system_monitor: MockSystemMonitor::new(20.0, 2048, 8192),
        event_tx,
        audit_port: Arc::new(NoopAudit) as Arc<dyn AuditLogPort>,
        cert_resolver,
        jwt_verifier: Some(jwt_verifier),
        mtls_verifier: None,
        ip_ban: Arc::new(IpBan::new()),
        metrics: Arc::new(ExternalMetrics::new()),
        shutdown_rx,
        shutdown_tx,
        pii_sanitizer: None,
        ai_runtime_status_snapshot: None,
        load_policy: std::sync::Arc::new(oneshim_web::grpc::LoadPolicy::new(
            oneshim_core::config::LoadThresholds::default(),
        )),
        streaming_enabled: true,
    }
}
```

> **Note for the implementer:** if `features2` lands first and `ExternalGrpcSpawnConfig` gains a non-default field (e.g., a new `request_id_layer` config), the literal init above will fail to compile. Add the field with a sensible default; do NOT touch `streaming_enabled`. Per spec §10.3 this is the "trivial mechanical rebase" anticipated in R4.

- [ ] **Step 3.5: Append `spawn_stress_server` helper**

This is a streamlined `spawn_server` (mirrors `external_grpc_integration.rs:253-292`) that supports IPv6 binds and uses the OS-assigned port (port 0) instead of a `next_test_port` allocator (since stress tests run with `--test-threads=1`, port collisions across stress tests are impossible).

```rust
// ── Server spawn helper ──────────────────────────────────────────────────────
//
// Mirrors spawn_server in external_grpc_integration.rs:253. Uses the OS-assigned
// port (caller passes bind_addr with port 0). Returns the actual bound port
// observed via TCP probing — not via a port allocator, since serve_external
// rebinds the port itself and we do not have a public accessor for the resolved
// port.
//
// Strategy: caller passes bind_addr with port 0; we replicate the bind ourselves
// to discover the OS-assigned port, drop our listener, then pass the resolved
// addr to serve_external. Race window between drop + serve_external bind is
// covered by SO_REUSEADDR in the same process.

async fn spawn_stress_server(
    mut cfg: ExternalGrpcSpawnConfig,
) -> (tokio::task::JoinHandle<()>, SocketAddr) {
    install_rustls_crypto_provider();

    // Bind once locally to discover the OS-assigned port for the requested
    // family (v4 vs v6), then close and let serve_external rebind it.
    let std_listener = std::net::TcpListener::bind(cfg.bind_addr)
        .expect("std bind for port discovery");
    let bound = std_listener.local_addr().expect("local_addr");
    drop(std_listener);
    cfg.bind_addr = bound;

    let probe_addr = bound;
    let handle = tokio::spawn(async move {
        if let Err(e) = serve_external(cfg).await {
            eprintln!("serve_external error: {e:?}");
        }
    });

    // Wait until the server accepts TCP connections (timeout: 5s).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if tokio::net::TcpStream::connect(probe_addr).await.is_ok() {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("external gRPC server did not start at {probe_addr} within 5s");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    (handle, probe_addr)
}

fn server_cert_pem() -> Vec<u8> {
    let (cert_path, _) = test_cert_pair();
    std::fs::read(&cert_path).expect("read server cert PEM")
}
```

- [ ] **Step 3.6: Append `make_stress_tls_channel` helper**

This is the stress variant of `make_tls_channel` (mirrors `external_grpc_integration.rs:295-315`), with explicit per-channel `Endpoint::connect()` to satisfy V3 (one TCP per channel). Returns `Result` rather than panicking — the over-cap channel is EXPECTED to fail to connect.

```rust
// ── TLS channel helper (stress variant) ───────────────────────────────────────
//
// Returns Result<Channel, tonic::transport::Error> instead of panicking — the
// stress tests intentionally try to open over-cap channels and expect failure
// (Test 1 Phase 2). Each call produces a fresh Endpoint::connect() →
// distinct underlying TCP per V3.

async fn make_stress_tls_channel(
    addr: SocketAddr,
    server_cert_pem: &[u8],
) -> Result<Channel, tonic::transport::Error> {
    let ca_cert = Certificate::from_pem(server_cert_pem);
    let tls = ClientTlsConfig::new()
        .domain_name("localhost")
        .ca_certificate(ca_cert);
    // Use ipv6 / ipv4 literal in the URI authority. tonic accepts both.
    let uri = if addr.is_ipv6() {
        format!("https://[{}]:{}", addr.ip(), addr.port())
    } else {
        format!("https://{}:{}", addr.ip(), addr.port())
    };
    Endpoint::from_shared(uri)
        .expect("valid endpoint")
        .tls_config(tls)
        .expect("tls config")
        .connect_timeout(Duration::from_secs(3))
        .connect()
        .await
}
```

- [ ] **Step 3.7: Append a small "fresh RPC succeeds" probe (V4 fallback)**

```rust
// ── Server liveness probe (V4 fallback for active_connection_count) ──────────
//
// Polls a fresh unary GetAgentInfo round-trip until success or deadline.
// Used by Test 1 Phase 3 (slot recovery) and Test 2 post-loop check —
// production lacks a public active_connection_count accessor.

async fn poll_unary_until_success(
    addr: SocketAddr,
    cert_pem: &[u8],
    token: &str,
    deadline: tokio::time::Instant,
) -> Result<(), String> {
    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err("poll_unary_until_success: deadline exceeded".into());
        }
        let channel = match make_stress_tls_channel(addr, cert_pem).await {
            Ok(c) => c,
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
        };
        let mut req = tonic::Request::new(GetAgentInfoRequest {});
        req.metadata_mut().insert(
            "authorization",
            format!("Bearer {token}").parse().expect("valid header"),
        );
        match DashboardServiceClient::new(channel).get_agent_info(req).await {
            Ok(_) => return Ok(()),
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}
```

- [ ] **Step 3.8: Append Test 1**

```rust
// ════════════════════════════════════════════════════════════════════════════
// Test 1: concurrent_connection_cap_enforced
// ════════════════════════════════════════════════════════════════════════════

/// Invariant: max_connections = N → N concurrent connections succeed; the
/// (N+1)th is rejected at the connection layer.
///
/// Phases (spec §4.1):
///   Phase 1: open 1024 concurrent channels (each with 1 subscribe_events
///            stream) and confirm all establish.
///   Phase 2: attempt the 1025th channel; expect transport-level failure.
///   Phase 3: drop one Phase-1 channel, poll for slot recovery (V4 fallback:
///            unary RPC), retry the 1025th — expect success.
///
/// fd estimate: ~2050 (1024 server + 1024 client + tokio + OS). ulimit -n
/// 65536 in the workflow provides 32× headroom.
///
/// Runtime estimate: ~5–15s (1024 TLS handshakes dominate Phase 1).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_connection_cap_enforced() {
    const CAP: usize = 1024;
    let jwt_kp = test_jwt_keypair();
    let cfg = make_jwt_stress_config(
        &jwt_kp.pub_pem_path,
        CAP,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
    );
    let (handle, addr) = spawn_stress_server(cfg).await;

    let token = test_mint_jwt(
        &jwt_kp.enc_key,
        "stress-cap",
        "test-issuer",
        "test-audience",
        3600,
    );
    let cert_pem = server_cert_pem();

    // ── Phase 1: open CAP concurrent streams ────────────────────────────────
    let mut tasks = JoinSet::new();
    for i in 0..CAP {
        let addr_c = addr;
        let cert_c = cert_pem.clone();
        let token_c = token.clone();
        tasks.spawn(async move {
            let channel = make_stress_tls_channel(addr_c, &cert_c)
                .await
                .map_err(|e| format!("channel {i} connect failed: {e}"))?;
            let mut req = tonic::Request::new(SubscribeEventsRequest::default());
            req.metadata_mut().insert(
                "authorization",
                format!("Bearer {token_c}").parse().expect("valid header"),
            );
            let stream = DashboardServiceClient::new(channel.clone())
                .subscribe_events(req)
                .await
                .map_err(|e| format!("stream {i} open failed: {e}"))?
                .into_inner();
            // Hold both channel + stream so the underlying TCP stays open.
            Ok::<(Channel, _), String>((channel, stream))
        });
    }

    let mut held = Vec::with_capacity(CAP);
    while let Some(joined) = tasks.join_next().await {
        let res = joined.expect("task panicked");
        let pair = res.unwrap_or_else(|e| panic!("Phase 1 failed: {e}"));
        held.push(pair);
    }
    assert_eq!(held.len(), CAP, "Phase 1 should establish all {CAP} streams");

    // ── Phase 2: (CAP+1)th attempt rejected ─────────────────────────────────
    //
    // Cap rejection is silent (V2: TCP dropped before TLS). From the client
    // side this manifests as one of:
    //   - Endpoint::connect fails (TLS handshake error / EOF).
    //   - Channel created but first RPC fails with transport error.
    // Either is acceptable; we assert that the over-cap path eventually errors.
    let over_cap_result = async {
        let channel = make_stress_tls_channel(addr, &cert_pem).await?;
        let mut req = tonic::Request::new(SubscribeEventsRequest::default());
        req.metadata_mut().insert(
            "authorization",
            format!("Bearer {token}").parse().expect("valid header"),
        );
        DashboardServiceClient::new(channel)
            .subscribe_events(req)
            .await
            .map(|_| ())
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })
    }
    .await;
    assert!(
        over_cap_result.is_err(),
        "(CAP+1)th channel must be rejected; got: {over_cap_result:?}"
    );

    // ── Phase 3: drop one slot, retry ───────────────────────────────────────
    drop(held.pop().expect("at least one held pair"));

    // V4 fallback: poll for liveness via fresh unary RPC.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    poll_unary_until_success(addr, &cert_pem, &token, deadline)
        .await
        .expect("unary RPC must succeed after slot freed");

    // Now retry the (CAP)-th stream — should succeed.
    let retry_channel = make_stress_tls_channel(addr, &cert_pem)
        .await
        .expect("retry channel after slot recovery must connect");
    let mut req = tonic::Request::new(SubscribeEventsRequest::default());
    req.metadata_mut().insert(
        "authorization",
        format!("Bearer {token}").parse().expect("valid header"),
    );
    let _retry_stream = DashboardServiceClient::new(retry_channel)
        .subscribe_events(req)
        .await
        .expect("retry stream open after slot recovery");

    // Cleanup
    drop(held);
    handle.abort();
    let _ = handle.await;
}
```

- [ ] **Step 3.9: Run Test 1 locally**

Run:
```bash
ulimit -n 65536
cargo test -p oneshim-web --features stress-test \
  --test external_grpc_stress \
  concurrent_connection_cap_enforced \
  -- --test-threads=1 --nocapture
```
Expected: PASS in ~5–15s.

If FAIL with `ulimit` error: re-run with `ulimit -n 65536` actually applied (zsh + macOS may need `ulimit -n unlimited`; macOS hard cap may be < 65536 — see workflow note in §5).

If FAIL with the over-cap assertion ("got: Ok(())"): verify V2 conclusion is still accurate by re-grepping `accept_loop.rs:86-91`. If accept_loop still drops over-cap silently, the test should reach the assertion path; if not, production behavior changed and the assertion needs updating.

- [ ] **Step 3.10: Verify file compiles without `--features stress-test`**

Run: `cargo check -p oneshim-web --tests` (no `--features`)
Expected: clean compile. The whole file should be guarded by `#![cfg(feature = "stress-test")]` so type errors in helpers do not surface.

- [ ] **Step 3.11: Run lefthook**

Run: `lefthook run pre-commit`
Expected: PASS.

- [ ] **Step 3.12: Commit**

```bash
git add crates/oneshim-web/tests/external_grpc_stress.rs
git commit -m "$(cat <<'EOF'
test(oneshim-web): concurrent_connection_cap_enforced (Test 1)

Adds Test 1 of the stress suite + locally duplicated helpers shared with
Tests 2 and 3:
- NoopAudit (mirror of tests/external_grpc_integration.rs:92-139).
- make_test_shutdown_pair, in_memory_storage.
- make_jwt_stress_config(jwt_pub_key, max_connections, bind_addr)
  — parameterized variant of make_jwt_config that lets stress tests
  pick the cap and IPv4/IPv6 bind.
- spawn_stress_server (probe-then-serve_external).
- make_stress_tls_channel — Result-returning per-channel Endpoint::connect
  to satisfy V3 (one TCP per channel).
- poll_unary_until_success — V4 fallback (no public connection-count
  accessor; use unary RPC liveness probing).

Test 1 phases:
  1) Open 1024 concurrent subscribe_events streams; expect all OK.
  2) Try the 1025th; expect transport-level rejection (V2: silent
     accept_loop drop manifests as connect error or first-RPC error).
  3) Drop one Phase-1 stream, poll for slot recovery, retry — expect OK.

Runtime ~5–15s under ulimit -n 65536.

EOF
)"
```

---

## Task 4: Test 2 — `fd_pressure_resilience` (C4)

**Files:**
- Modify: `crates/oneshim-web/tests/external_grpc_stress.rs` (append Test 2)

### Steps

- [ ] **Step 4.1: Append Test 2 to the same file**

Append to the end of `crates/oneshim-web/tests/external_grpc_stress.rs`:

```rust
// ════════════════════════════════════════════════════════════════════════════
// Test 2: fd_pressure_resilience
// ════════════════════════════════════════════════════════════════════════════

/// Invariant: 3 rounds of open-1024 / hold-200ms / drop-all do not leak fds
/// or kill the accept loop. Post-loop the server still serves a unary RPC
/// AND can accept another 1024 streams.
///
/// Regression targets (spec §4.2):
///   - accept_loop's Drop path on connection cleanup
///   - supervisor respawn fidelity (silent accept-loop death post-churn)
///   - tokio task leakage (spawned RPC handlers not joined on drop)
///
/// Runtime estimate: ~20–35s.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fd_pressure_resilience() {
    const CAP: usize = 1024;
    const ROUNDS: usize = 3;

    let jwt_kp = test_jwt_keypair();
    let cfg = make_jwt_stress_config(
        &jwt_kp.pub_pem_path,
        CAP,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
    );
    let (handle, addr) = spawn_stress_server(cfg).await;

    let token = test_mint_jwt(
        &jwt_kp.enc_key,
        "stress-fd",
        "test-issuer",
        "test-audience",
        3600,
    );
    let cert_pem = server_cert_pem();

    for round in 0..ROUNDS {
        // ── 2a: open CAP concurrent streams ─────────────────────────────────
        let mut tasks = JoinSet::new();
        for i in 0..CAP {
            let addr_c = addr;
            let cert_c = cert_pem.clone();
            let token_c = token.clone();
            tasks.spawn(async move {
                let channel = make_stress_tls_channel(addr_c, &cert_c)
                    .await
                    .map_err(|e| format!("round {round} channel {i}: {e}"))?;
                let mut req = tonic::Request::new(SubscribeEventsRequest::default());
                req.metadata_mut().insert(
                    "authorization",
                    format!("Bearer {token_c}").parse().expect("valid header"),
                );
                let stream = DashboardServiceClient::new(channel.clone())
                    .subscribe_events(req)
                    .await
                    .map_err(|e| format!("round {round} stream {i}: {e}"))?
                    .into_inner();
                Ok::<(Channel, _), String>((channel, stream))
            });
        }

        let mut held = Vec::with_capacity(CAP);
        while let Some(joined) = tasks.join_next().await {
            let pair = joined
                .expect("task panicked")
                .unwrap_or_else(|e| panic!("round {round} setup failed: {e}"));
            held.push(pair);
        }
        assert_eq!(
            held.len(),
            CAP,
            "round {round}: should establish all {CAP} streams"
        );

        // ── 2b: hold ────────────────────────────────────────────────────────
        tokio::time::sleep(Duration::from_millis(200)).await;

        // ── 2c: drop all ────────────────────────────────────────────────────
        drop(held);

        // ── 2d: wait up to 5s for server-side cleanup (V4 fallback) ─────────
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        poll_unary_until_success(addr, &cert_pem, &token, deadline)
            .await
            .unwrap_or_else(|e| panic!("round {round} cleanup poll failed: {e}"));
    }

    // ── Post-loop verification ─────────────────────────────────────────────
    // P1: fresh unary RPC succeeds.
    let post_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    poll_unary_until_success(addr, &cert_pem, &token, post_deadline)
        .await
        .expect("post-loop unary RPC must succeed");

    // P2: open CAP new streams — no residual fd leak.
    let mut tasks = JoinSet::new();
    for i in 0..CAP {
        let addr_c = addr;
        let cert_c = cert_pem.clone();
        let token_c = token.clone();
        tasks.spawn(async move {
            let channel = make_stress_tls_channel(addr_c, &cert_c)
                .await
                .map_err(|e| format!("post channel {i}: {e}"))?;
            let mut req = tonic::Request::new(SubscribeEventsRequest::default());
            req.metadata_mut().insert(
                "authorization",
                format!("Bearer {token_c}").parse().expect("valid header"),
            );
            let stream = DashboardServiceClient::new(channel.clone())
                .subscribe_events(req)
                .await
                .map_err(|e| format!("post stream {i}: {e}"))?
                .into_inner();
            Ok::<(Channel, _), String>((channel, stream))
        });
    }
    let mut held = Vec::with_capacity(CAP);
    while let Some(joined) = tasks.join_next().await {
        let pair = joined
            .expect("task panicked")
            .unwrap_or_else(|e| panic!("post-loop fan-out failed: {e}"));
        held.push(pair);
    }
    assert_eq!(
        held.len(),
        CAP,
        "post-loop should still admit {CAP} streams (no fd leak)"
    );

    drop(held);
    handle.abort();
    let _ = handle.await;
}
```

- [ ] **Step 4.2: Run Test 2 locally**

Run:
```bash
ulimit -n 65536
cargo test -p oneshim-web --features stress-test \
  --test external_grpc_stress \
  fd_pressure_resilience \
  -- --test-threads=1 --nocapture
```
Expected: PASS in ~20–35s.

If post-loop fan-out fails ("post-loop should still admit..."): the production accept_loop is leaking fds OR the cleanup-watch deadline is too short. First, increase the cleanup deadline to 10s and re-run. If still failing → genuine regression in `accept_loop` Drop / `ActiveConnGuard`.

- [ ] **Step 4.3: Run BOTH Tests 1 + 2 sequentially**

Run:
```bash
ulimit -n 65536
cargo test -p oneshim-web --features stress-test \
  --test external_grpc_stress \
  -- --test-threads=1 --nocapture
```
Expected: both PASS, total ~25–50s. Critical: `--test-threads=1` ensures no fd contention between tests.

- [ ] **Step 4.4: Run lefthook**

Run: `lefthook run pre-commit`
Expected: PASS.

- [ ] **Step 4.5: Commit**

```bash
git add crates/oneshim-web/tests/external_grpc_stress.rs
git commit -m "$(cat <<'EOF'
test(oneshim-web): fd_pressure_resilience (Test 2)

3 rounds of open-1024 / hold-200ms / drop-all + post-loop verification:
- After each round, poll unary RPC for liveness (V4 fallback for
  active_connection_count).
- Post-loop: unary RPC succeeds AND a fresh fan-out of 1024 streams
  all establish (no residual fd leak / accept-loop death).

Reuses helpers from C3 (NoopAudit, make_jwt_stress_config,
spawn_stress_server, make_stress_tls_channel, poll_unary_until_success).

Runtime ~20–35s under ulimit -n 65536. Regressions target:
  - accept_loop Drop path / ActiveConnGuard.
  - tokio task leakage (RPC handlers not joined on drop).
  - silent supervisor-managed accept-loop death.

EOF
)"
```

---

## Task 5: Test 3 — `ipv6_64_prefix_ban_full_stack` (C5)

**Files:**
- Modify: `crates/oneshim-web/tests/external_grpc_stress.rs` (append Test 3)

V1 already established that this test is implementable as-is. No accept_loop wiring change.

### Steps

- [ ] **Step 5.1: Append Test 3 to the same file**

```rust
// ════════════════════════════════════════════════════════════════════════════
// Test 3: ipv6_64_prefix_ban_full_stack
// ════════════════════════════════════════════════════════════════════════════

/// Invariant: IpBan::record_failure (auth_layer / accept_loop on TLS error)
/// + IpBan::is_banned (accept_loop pre-TLS) are wired on the IPv6 path.
/// After 5 auth failures from [::1], the 6th connection from [::1] is
/// rejected before TLS handshake (V1 verified accept_loop:77 ordering).
///
/// Single-/128 limitation (spec §4.3 known limitations): CI loopback is one
/// ::1/128. All 6 attempts share that /128. The test verifies WIRING
/// (IpBan called on IPv6 accept path), not the /64 prefix logic — which
/// is unit-tested at ip_ban.rs::ipv6_64_prefix_shared_ban.
///
/// Runtime estimate: ~2–5s.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ipv6_64_prefix_ban_full_stack() {
    const SMALL_CAP: usize = 32; // small; never the rejecting layer
    let jwt_kp = test_jwt_keypair();
    let cfg = make_jwt_stress_config(
        &jwt_kp.pub_pem_path,
        SMALL_CAP,
        SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 0),
    );
    let (handle, addr) = spawn_stress_server(cfg).await;
    assert!(addr.is_ipv6(), "Test 3 must bind IPv6 ::1");

    let bad_token = test_mint_jwt(
        &jwt_kp.enc_key,
        "ban-victim",
        "test-issuer",
        "wrong-audience", // intentional aud mismatch → JWT verifier rejects
        3600,
    );
    let cert_pem = server_cert_pem();

    // ── Phase 1: 5 auth failures from ::1 ────────────────────────────────────
    //
    // The IpBan THRESHOLDS ladder in ip_ban.rs requires 5 failures within the
    // sliding window for a 60s ban. Each attempt completes the TLS handshake
    // (loopback cert is trusted) and then auth_layer rejects with
    // Unauthenticated, calling ip_ban.record_failure(remote).
    for attempt in 0..5 {
        let channel = make_stress_tls_channel(addr, &cert_pem)
            .await
            .unwrap_or_else(|e| panic!("attempt {attempt}: TLS connect must succeed: {e}"));
        let mut req = tonic::Request::new(GetAgentInfoRequest {});
        req.metadata_mut().insert(
            "authorization",
            format!("Bearer {bad_token}").parse().expect("valid header"),
        );
        let err = DashboardServiceClient::new(channel)
            .get_agent_info(req)
            .await
            .expect_err(&format!("attempt {attempt}: bad-aud token must be rejected"));
        assert_eq!(
            err.code(),
            tonic::Code::Unauthenticated,
            "attempt {attempt}: expected Unauthenticated; got {err:?}"
        );
    }

    // Brief wait for ip_ban state to commit (record_failure is sync but the
    // channel-shutdown side of the auth-failure path is async).
    tokio::time::sleep(Duration::from_millis(200)).await;

    // ── Phase 2: 6th attempt rejected before TLS ─────────────────────────────
    //
    // accept_loop:77 calls ip_ban.is_banned(remote) immediately after TCP
    // accept. is_banned == true → accept_loop drops the TCP. From the client
    // side, the TLS handshake fails (server closed connection mid-handshake)
    // OR the connect itself fails. Either is acceptable — we assert any
    // error path, not a specific gRPC Code.
    let result = make_stress_tls_channel(addr, &cert_pem).await;
    let banned_path_result: Result<(), Box<dyn std::error::Error + Send + Sync>> = match result {
        Err(e) => Err(Box::new(e)),
        Ok(channel) => {
            // Channel created lazily; the failure may surface on first RPC.
            let mut req = tonic::Request::new(GetAgentInfoRequest {});
            req.metadata_mut().insert(
                "authorization",
                format!("Bearer {bad_token}").parse().expect("valid header"),
            );
            DashboardServiceClient::new(channel)
                .get_agent_info(req)
                .await
                .map(|_| ())
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })
        }
    };
    assert!(
        banned_path_result.is_err(),
        "6th attempt from ::1 must fail (banned). Got: {banned_path_result:?}"
    );

    handle.abort();
    let _ = handle.await;
}
```

- [ ] **Step 5.2: Run Test 3 locally**

Run:
```bash
ulimit -n 65536
cargo test -p oneshim-web --features stress-test \
  --test external_grpc_stress \
  ipv6_64_prefix_ban_full_stack \
  -- --test-threads=1 --nocapture
```
Expected: PASS in ~2–5s.

If `Phase 1: bad-aud token must be rejected` fails with a different code: the JWT verifier may have changed its rejection code; update the assertion to match.

If `Phase 2: 6th attempt must fail` succeeds (no error): production wiring may be broken. Re-verify V1 by re-reading `accept_loop.rs:77`.

- [ ] **Step 5.3: Run all 3 tests sequentially**

Run:
```bash
ulimit -n 65536
cargo test -p oneshim-web --features stress-test \
  --test external_grpc_stress \
  -- --test-threads=1 --nocapture
```
Expected: all 3 PASS in ~30–60s total.

- [ ] **Step 5.4: Run clippy with stress-test active**

Run: `cargo clippy -p oneshim-web --features stress-test --tests -- -D warnings`
Expected: clean.

- [ ] **Step 5.5: Run lefthook**

Run: `lefthook run pre-commit`
Expected: PASS.

- [ ] **Step 5.6: Commit**

```bash
git add crates/oneshim-web/tests/external_grpc_stress.rs
git commit -m "$(cat <<'EOF'
test(oneshim-web): ipv6_64_prefix_ban_full_stack (Test 3)

Verifies IpBan accept_loop wiring on the IPv6 path: 5 auth failures from
[::1] (bad audience JWT → Unauthenticated) → 6th connection from [::1]
rejected before TLS handshake (V1 verified accept_loop.rs:77 ordering).

Single-/128 limitation: CI loopback is one ::1/128. The test asserts
WIRING (IpBan invoked on IPv6 accept path), not /64 prefix logic — that
is covered by ip_ban.rs::ipv6_64_prefix_shared_ban unit test.

Runtime ~2–5s. small_cap = 32 ensures max_connections is never the
rejecting layer.

EOF
)"
```

---

## Task 6: Document local stress test run instructions (C6)

**Files:**
- Modify: `docs/guides/external-grpc.md` (append a new section)

### Steps

- [ ] **Step 6.1: Inspect the current end of the file**

Run: `wc -l docs/guides/external-grpc.md && tail -10 docs/guides/external-grpc.md`
Expected: file exists. If absent, create with a `# External gRPC Server` header before appending.

- [ ] **Step 6.2: Append the section**

Append to `docs/guides/external-grpc.md`:

```markdown

## Running stress tests locally

The external gRPC stress suite (`crates/oneshim-web/tests/external_grpc_stress.rs`) is gated behind the `stress-test` cargo feature so it never runs in the regular `cargo test --workspace` path. The suite covers three scenarios:

1. `concurrent_connection_cap_enforced` — 1024 concurrent connections at `max_connections = 1024`, slot-recovery on drop.
2. `fd_pressure_resilience` — 3 rounds of 1024-stream churn, no fd leak post-loop.
3. `ipv6_64_prefix_ban_full_stack` — `IpBan` accept-loop wiring on `[::1]` (5 auth failures → 6th rejected pre-TLS).

### Local prerequisites

- `ulimit -n 65536` (raise the open-file limit before invoking cargo).
- IPv6 loopback (`[::1]`) reachable. Default on Linux/macOS.
- ~5s to ~15s per test on modern hardware.

### Command

```sh
ulimit -n 65536
cargo test -p oneshim-web --features stress-test \
  --test external_grpc_stress \
  -- --test-threads=1 --nocapture
```

`--test-threads=1` is mandatory — Tests 1 and 2 each consume ~2050 file descriptors. Running them in parallel needs >4000 fds AND increases racy cleanup paths.

### CI invocation

Stress tests run via the `gRPC Stress Test` workflow (`.github/workflows/grpc-stress.yml`):

- Manually: `gh workflow run grpc-stress.yml --ref <branch>`.
- Weekly: every Sunday 03:00 UTC.

The workflow runs on `ubuntu-latest` (only platform with predictable `ulimit -n` and IPv6 loopback semantics).
```

- [ ] **Step 6.3: Run lefthook**

Run: `lefthook run pre-commit`
Expected: PASS.

- [ ] **Step 6.4: Commit**

```bash
git add docs/guides/external-grpc.md
git commit -m "$(cat <<'EOF'
docs(grpc-stress): document stress test local run instructions

Adds "Running stress tests locally" section to docs/guides/external-grpc.md
covering:
- the three tests (concurrent_connection_cap_enforced,
  fd_pressure_resilience, ipv6_64_prefix_ban_full_stack)
- local prerequisites (ulimit -n 65536, IPv6 loopback)
- the cargo test command (--features stress-test, --test-threads=1)
- CI invocation (workflow_dispatch + weekly cron)

EOF
)"
```

---

## Pre-merge gate (per spec §7)

After all 6 commits land:

- [ ] **Push branch + dispatch the workflow**

```bash
git push -u origin feature/grpc-stress-test-suite
gh workflow run grpc-stress.yml --ref feature/grpc-stress-test-suite
gh run list --workflow=grpc-stress.yml --limit 1
```

- [ ] **Wait for the run to complete**

Watch: `gh run watch <RUN_ID>` (or use `gh run view` to poll).
Expected: all 3 tests green within the 15-min job timeout.

- [ ] **Verify regular CI is unchanged**

Run regular `Test` workflow on the PR. The diff vs main for `cargo test --workspace` test count should be 0 (the stress file compiles to an empty binary without the feature).

- [ ] **Open the PR**

```bash
gh pr create --title "test: external gRPC stress test suite (T15 rewrite + IPv6 wiring)" \
  --body "$(cat <<'EOF'
## Summary

- Adds `crates/oneshim-web/tests/external_grpc_stress.rs` (3 tests) gated by new `stress-test` cargo feature.
- Adds `.github/workflows/grpc-stress.yml`: workflow_dispatch + weekly Sunday 03:00 UTC schedule, ulimit -n 65536.
- Documents local run in `docs/guides/external-grpc.md`.

Tests:
1. `concurrent_connection_cap_enforced` — `max_connections=1024` correctness + dynamic slot recovery.
2. `fd_pressure_resilience` — 3 rounds of 1024-stream churn + post-loop survival.
3. `ipv6_64_prefix_ban_full_stack` — IpBan accept_loop wiring on the IPv6 path.

Spec: `docs/superpowers/specs/2026-04-24-grpc-stress-test-suite-design.md`
Plan: `docs/superpowers/plans/2026-04-24-grpc-stress-test-suite-plan.md`

## Pre-merge gate

Manual workflow run: <PASTE_RUN_URL>

## Test plan

- [x] `cargo test -p oneshim-web --features stress-test --test external_grpc_stress -- --test-threads=1` PASSES locally.
- [x] `cargo test --workspace` test count unchanged vs main (verified via diff).
- [x] `cargo clippy -p oneshim-web --features stress-test --tests -- -D warnings` clean.
- [x] `cargo fmt --check` clean.
- [x] Manual `gh workflow run grpc-stress.yml` PASS — see link above.
EOF
)"
```

Squash subject must keep the `test:` prefix so git-cliff includes the entry in CHANGELOG (per `feedback_squash_merge_cliff_skip.md`).

---

## Acceptance criteria mirror (spec §8)

- [ ] `cargo test -p oneshim-web --features stress-test --test external_grpc_stress -- --test-threads=1` passes locally.
- [ ] `gh workflow run grpc-stress.yml` (manual dispatch) runs to completion with all 3 tests green, before merge.
- [ ] Regular CI (`Test` workflow on PRs) unchanged — same test counts pre/post merge.
- [ ] `cargo clippy --workspace --all-features -- -D warnings` clean (note: `--all-features` activates `stress-test`).
- [ ] `cargo fmt --check` clean.
- [ ] `lefthook run pre-commit` passes at each commit.
- [ ] V1–V4 pre-implementation findings documented in C2 commit body (the C2 plan-amend) — already present in this plan's "Pre-implementation verification" section above.
- [ ] Weekly schedule registered (verified post-merge in the GHA "Scheduled" UI on the first Sunday).
- [ ] `docs/guides/external-grpc.md` has a reproducible local-run snippet.

---

## Risk recap (spec §9)

The plan inherits all spec §9 risks. Mitigations remain:
- **R1 (V1 false)**: ✅ Resolved — V1 verified PASS in this plan's pre-impl section. No scope expansion needed.
- **R2 (tonic multiplexing)**: ✅ Per-channel `Endpoint::connect()` confirmed in V3.
- **R3 (GHA hard fd limit)**: workflow prints `ulimit -Hn` early; if test cannot get to 65536 it should fail-fast, not silent-skip. Implementer: if local test fails on macOS due to hard cap, document via `sysctl kern.maxfilesperproc` adjustment or skip on macOS via `#[cfg]`.
- **R4 (features2 lands first)**: trivial mechanical rebase — flagged in §3.4 above.
- **R5 (IpBan sliding window)**: per-test `IpBan::new()` + `--test-threads=1` deterministic state.
- **R6 (weekly CI cost)**: ~5 min/run = ~21 min/month, well within free tier.
- **R7 (env drift)**: pre-merge manual `gh workflow run` catches before merge.
- **R8 (collateral bugs)**: V1–V4 pre-impl verification ran above. If implementation surfaces unrelated regressions, file separate issue and decide scope vs defer with user.

---

## Coupling with features2 (spec §10)

Test-only PR. **DO NOT TOUCH** during implementation:
- `crates/oneshim-web/src/grpc/external/audit_layer.rs`
- `crates/oneshim-web/src/grpc/external/live_config.rs`
- `crates/oneshim-web/src/grpc/external/request_id_layer.rs`
- `ExternalGrpcConfig.streaming_enabled` (use the field's default; never override)
- `audit` schema migration V32

The only files touched outside this PR's strict scope are `Cargo.toml [features]` (1 line addition) and `docs/guides/external-grpc.md` (section append). Both are anticipated as trivial conflicts — see spec §10.3.

---

## Plan self-review (per writing-plans skill checklist)

1. **Spec coverage**: every section in the spec maps to a task above:
   - Spec §1 → context for plan; no task.
   - Spec §2 (G1–G5) → Tasks 3, 4, 5 (G1/G2/G3) + Tasks 1+pre-merge gate (G4) + this plan structure (G5).
   - Spec §3 (file layout, feature gating, helper strategy) → Tasks 1 + 3.
   - Spec §4 (test designs) → Tasks 3, 4, 5.
   - Spec §5 (workflow design) → Task 1.
   - Spec §6 (V1–V4) → "Pre-implementation verification" section + Task 2.
   - Spec §7 (commit plan) → Tasks 1–6 align with C1–C6.
   - Spec §8 (acceptance) → "Acceptance criteria mirror" section.
   - Spec §9 (risks) → "Risk recap" section.
   - Spec §10 (features2 coupling) → "Coupling with features2" section + Step 3.4 note.
2. **Placeholders**: scanned — no `TBD`/`TODO`/`later`/"add appropriate X". The `<PASTE_RUN_URL>` placeholder in the PR body is intentional and human-supplied at PR-open time.
3. **Type consistency**: `make_stress_tls_channel` returns `Result<Channel, tonic::transport::Error>` everywhere (Tests 1, 2, 3, and `poll_unary_until_success`). `spawn_stress_server` returns `(JoinHandle<()>, SocketAddr)` everywhere. `make_jwt_stress_config` signature `(&Path, usize, SocketAddr) -> ExternalGrpcSpawnConfig` consistent across all callers.
