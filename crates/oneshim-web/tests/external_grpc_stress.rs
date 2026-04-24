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
