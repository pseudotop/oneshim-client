//! In-process atomic metrics for external gRPC.
//! No Prometheus dependency — values are exported via the existing telemetry adapter
//! as a follow-up (see spec §5.2 and Task 11a).

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicUsize};

use parking_lot::RwLock;

/// Snapshot of in-process counters for the external gRPC binding.
///
/// All fields are atomics so they can be read lock-free from any thread.
/// The `RwLock<HashMap<…, AtomicU64>>` fields are write-locked only when a new
/// label combination is seen for the first time (amortised constant write cost).
#[derive(Default)]
pub struct ExternalMetrics {
    /// Keyed by `"transport|auth_type|result"` (e.g. `"external|jwt|ok"`).
    pub requests_total: RwLock<HashMap<String, AtomicU64>>,
    /// Keyed by auth-failure reason string (e.g. `"invalid_jwt"`, `"expired_cert"`).
    pub auth_failures_total: RwLock<HashMap<&'static str, AtomicU64>>,
    /// Signed so decrement is safe without wrapping.
    pub active_streams: AtomicI64,
    /// Seconds until TLS cert notAfter (updated daily by `spawn_expiry_monitor`).
    pub tls_cert_expiry_seconds: AtomicI64,
    /// Current number of actively banned IPs.
    pub ip_bans_active: AtomicUsize,
    /// Cumulative successful cert hot-reloads.
    pub cert_reloads_ok: AtomicU64,
    /// Cumulative failed cert hot-reload attempts.
    pub cert_reloads_failed: AtomicU64,
    /// Cumulative connections rejected by the IP ban list.
    pub ip_bans_blocked_total: AtomicU64,
}

impl ExternalMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Increment the request counter for the given `(transport, auth_type, result)` triple.
    pub fn request_bump(&self, transport: &str, auth_type: &str, result: &str) {
        let key = format!("{transport}|{auth_type}|{result}");
        let guard = self.requests_total.read();
        if let Some(counter) = guard.get(&key) {
            counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return;
        }
        drop(guard);
        let mut guard = self.requests_total.write();
        guard
            .entry(key)
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Increment the auth-failure counter for the given static `reason` string.
    pub fn auth_failure_bump(&self, reason: &'static str) {
        let guard = self.auth_failures_total.read();
        if let Some(counter) = guard.get(reason) {
            counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return;
        }
        drop(guard);
        let mut guard = self.auth_failures_total.write();
        guard
            .entry(reason)
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Read the current request count for a composite label key (for tests).
    pub fn get_request_count(&self, key: &str) -> u64 {
        self.requests_total
            .read()
            .get(key)
            .map(|c| c.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(0)
    }

    /// Read the current auth-failure count for a reason string (for tests).
    pub fn get_auth_failure_count(&self, reason: &str) -> u64 {
        self.auth_failures_total
            .read()
            .get(reason)
            .map(|c| c.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_bump_and_get() {
        let m = ExternalMetrics::new();
        m.request_bump("external", "jwt", "ok");
        m.request_bump("external", "jwt", "ok");
        assert_eq!(m.get_request_count("external|jwt|ok"), 2);
    }

    #[test]
    fn auth_failure_bump() {
        let m = ExternalMetrics::new();
        m.auth_failure_bump("invalid_jwt");
        m.auth_failure_bump("invalid_jwt");
        m.auth_failure_bump("expired_cert");
        assert_eq!(m.get_auth_failure_count("invalid_jwt"), 2);
        assert_eq!(m.get_auth_failure_count("expired_cert"), 1);
    }

    #[test]
    fn active_streams_default_zero() {
        let m = ExternalMetrics::new();
        assert_eq!(
            m.active_streams.load(std::sync::atomic::Ordering::Relaxed),
            0
        );
    }
}
