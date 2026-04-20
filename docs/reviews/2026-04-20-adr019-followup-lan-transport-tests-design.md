# ADR-019 Follow-up #5 — LAN Transport Auth Regression Tests Design

**Date:** 2026-04-20
**Status:** ✅ SHIPPED (iter-195)
**Scope:** `crates/oneshim-network/src/sync/lan_transport/auth.rs`, new rustls-based test fixture module
**Origin:** ADR-019 §Known follow-ups #6 — "`sync/lan_transport::authenticate_with_peer` regression tests"
**Parent ADR:** [ADR-019](../architecture/ADR-019-error-code-infrastructure.md)
**Target version:** post-ADR-019 merge (independent timing)

> **Execution deviation:** The original design proposed an `rcgen` + `tokio_rustls::TlsAcceptor` fixture. Implementation chose a simpler **pure-function extraction** (`map_challenge_status_to_error(status_code, peer_id) -> CoreError` private helper + 6 unit tests covering 401/403/429/503/504 + 500-fallback). Same coverage on the error-mapping logic, no dev-dep additions, sub-millisecond per test. The fixture approach remains valid for future test cases that exercise the full handshake flow; this shipment covers the status→error mapping regression surface flagged in ADR-019.

## Context

The LAN transport uses TLS-only peer authentication (per the cross-device sync security model from ADR/superpowers S4). The `authenticate_with_peer` function dispatches HTTP status codes to semantic `CoreError` variants per the canonical pattern in [`docs/guides/http-status-error-mapping.md`](../guides/http-status-error-mapping.md).

Other 14 HTTP dispatchers in the workspace have regression tests asserting the status-code → wire-code mapping. `authenticate_with_peer` is the 15th but lacks tests because:

1. It requires a **live TLS connection** — `mockito` (the standard HTTP mock) serves plain HTTP only.
2. Generating a valid test cert chain + running a rustls acceptor is non-trivial test infrastructure.
3. The code path is defensive (peer auth failure is rare in production), so the test gap is low-risk.

The gap is tracked in ADR-019 Known Follow-up #6. The semantic mapping IS implemented — only the test coverage is missing.

## Goal

Add `rustls`-based test fixture sufficient to exercise `authenticate_with_peer` with controlled HTTP responses over TLS, then add the 5 canonical status-code regression tests (401/429/503/504/500) matching the pattern in the other 14 dispatchers.

## Decision

### 1. Test fixture module

Create `crates/oneshim-network/src/sync/lan_transport/test_fixture.rs` (gated by `#[cfg(test)]`):

```rust
#[cfg(test)]
pub(crate) mod test_fixture {
    use rustls::{
        Certificate, PrivateKey, ServerConfig,
        server::ResolvesServerCertUsingSni,
    };
    use rcgen::generate_simple_self_signed;
    use tokio_rustls::TlsAcceptor;
    use std::sync::Arc;

    /// Spin up a rustls TLS acceptor on 127.0.0.1:{random port} with a
    /// self-signed cert for SNI `localhost`. Returns (addr, root_ca_pem)
    /// so the client side can verify the server cert.
    pub async fn spawn_mock_tls_server(
        responses: Vec<(u16, &'static str)>,  // (status, body) per request
    ) -> (SocketAddr, String) { ... }

    /// Build an HTTPS client with the fixture's root CA installed.
    pub fn test_client(root_ca_pem: &str) -> reqwest::Client { ... }
}
```

Key design decisions:
- **Self-signed cert via `rcgen`** (already a dev-dependency candidate; light, pure-Rust). Generates SAN=localhost cert pair on fixture startup.
- **Per-test port** (`:0` lets OS pick) so parallel tests don't conflict.
- **Response queue**: caller pre-loads the `(status, body)` pairs the server will return in order; supports multi-request scenarios.
- **`tokio_rustls::TlsAcceptor`** matches the runtime the `lan_transport` uses in production.

### 2. Dev-dependency additions

Add to `crates/oneshim-network/Cargo.toml` `[dev-dependencies]`:

```toml
rcgen = "0.13"            # self-signed cert generation
tokio-rustls = "0.26"     # matches workspace rustls version
```

Already present: `reqwest` (with native-tls or rustls feature), `tokio`.

### 3. Regression tests

Mirror the canonical pattern from `docs/guides/http-status-error-mapping.md`:

```rust
#[tokio::test]
async fn authenticate_with_peer_401_maps_to_auth_failed() {
    let (addr, ca) = spawn_mock_tls_server(vec![(401, "unauthorized")]).await;
    let client = test_client(&ca);
    let err = authenticate_with_peer(&client, addr, "peer-id").await.unwrap_err();
    assert_eq!(err.code(), "auth.failed");
}

#[tokio::test]
async fn authenticate_with_peer_429_maps_to_rate_limit() {
    // (similar, with 429 status)
    assert_eq!(err.code(), "network.rate_limit");
}

#[tokio::test]
async fn authenticate_with_peer_503_maps_to_service_unavailable() {
    // (similar, with 503)
    assert_eq!(err.code(), "service.unavailable");
}

#[tokio::test]
async fn authenticate_with_peer_504_maps_to_request_timeout() {
    // (similar, with 504)
    assert_eq!(err.code(), "network.timeout");
}

#[tokio::test]
async fn authenticate_with_peer_500_falls_back_to_network_generic() {
    // (similar, with 500 — domain fallback assertion)
    assert_eq!(err.code(), "network.generic");
}
```

**Total: 5 regression tests**, one per canonical semantic HTTP class. Matches the 5-test pattern used for the 15th dispatcher (`auth::refresh`) landed in iter-98.

### 4. CI integration

No special CI work needed — tests run under the existing `cargo test -p oneshim-network` workflow step. The TLS fixture is self-contained (no external net access, no file system state).

### 5. Documentation

Append a row to the dispatcher registry in `docs/guides/http-status-error-mapping.md`:

```markdown
| `lan_transport::authenticate_with_peer` | Implemented ✓ | Tested ✓ | 5 tests (401/429/503/504/500) |
```

This closes the registry — all 16 dispatchers now have both Impl ✓ and Tests ✓ (iter-255 re-verification: the registry grew to 16 with `oneshim-web::services::ai_model_catalog_web_service`; design-time estimate was "closes at 15" but `grep -cE "^\| \`"` on the registry shows 16 rows).

## Consequences

### Positive
- 16/16 HTTP dispatchers have regression tests; no more "implemented but untested" exceptions.
- Test fixture reusable for any future TLS-only integration tests (LAN sync extensions, internal mTLS flows).
- Closes the last Known Follow-up that the ADR-019 drift audit deferred as "disproportionate effort".

### Negative
- Adds two dev-dependencies (`rcgen`, `tokio-rustls`). Compile-time cost: ~5-10 seconds on cold build.
- Test fixture is ~80-120 LOC of boilerplate (one-time cost).

### Neutral
- The `authenticate_with_peer` semantic mapping is unchanged — we're adding tests, not fixing behavior.

## Alternatives Considered

**A. Use `wiremock` instead of raw `rustls`.** Rejected — `wiremock` supports HTTPS but requires its own cert management layer; `rcgen` + direct `tokio_rustls` is ~30 LOC shorter and gives us full control.

**B. Skip the TLS layer entirely by refactoring `authenticate_with_peer` to be transport-agnostic.** Rejected — the TLS layer is a deliberate security property (per the cross-device sync threat model); removing it from the test path would make the test less representative.

**C. Mark this follow-up as "won't fix" and delete the entry.** Rejected — the semantic mapping exists; tests add real value by locking in the mapping contract.

## Implementation Plan

- **PR1** (fixture): introduce `test_fixture.rs` + dev-deps + a single smoke test verifying fixture works. ~2 hours.
- **PR2** (regression tests): 5 canonical status tests + registry table update. ~1 hour.

**Total effort estimate:** ~3 hours (0.5 day). Safe to do post-merge.

> **Post-execution reality:** PR1 (fixture + dev-deps) didn't happen. Chose the pure-function-extraction approach from §Alternatives — refactored the inline `match status.as_u16()` in `crates/oneshim-network/src/sync/lan_transport/auth.rs` into `map_challenge_status_to_error(status_code, peer_id) -> CoreError` and tested that helper directly with 6 unit tests (401/403/429/503/504 + 500-fallback). Same regression surface, no rustls dev-deps, sub-millisecond per test. Single iter (iter-195), closer to ~30 minutes elapsed than the estimated ~3 hours. Registry row in `docs/guides/http-status-error-mapping.md` updated in the same commit. The fixture approach remains valid for future tests that need the full TLS handshake flow.

## Out of Scope

- Integration tests of peer discovery (LAN peer discovery is a separate port; different scope).
- mTLS tests (peer authenticates client too) — not currently exercised.
- Negative certificate tests (wrong CA, expired cert, etc.) — defensive but low value.
