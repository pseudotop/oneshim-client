//! mDNS service registration + browse via `mdns-sd` (Phase 3b-2).
//!
//! Discovers LAN peers advertising `_oneshim-sync._tcp.local.` services.
//! Requires the `lan-sync` feature flag.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use tracing::{debug, info};

use oneshim_core::error::CoreError;

const SERVICE_TYPE: &str = "_oneshim-sync._tcp.local.";

/// Peer information discovered via mDNS.
#[derive(Debug, Clone)]
pub struct LanPeerInfo {
    pub device_id: String,
    pub device_name: String,
    pub host: String,
    pub port: u16,
    pub fingerprint: String,
    pub version: String,
}

/// mDNS-based LAN peer discovery.
#[allow(dead_code)]
pub struct LanDiscovery {
    device_id: String,
    device_name: String,
    port: u16,
    fingerprint: String,
    peers: Arc<RwLock<HashMap<String, LanPeerInfo>>>,
    running: bool,
}

impl LanDiscovery {
    /// Create a new discovery instance (does not start browsing).
    pub fn new(device_id: String, device_name: String, port: u16, fingerprint: String) -> Self {
        Self {
            device_id,
            device_name,
            port,
            fingerprint,
            peers: Arc::new(RwLock::new(HashMap::new())),
            running: false,
        }
    }

    /// Register mDNS service and start browsing for peers.
    ///
    /// Note: Full mDNS implementation requires the `mdns-sd` crate.
    /// This is a structural stub that logs the intent.
    pub fn start(&mut self) -> Result<(), CoreError> {
        info!(
            device_id = %self.device_id,
            port = self.port,
            "LAN discovery start requested (mDNS service: {SERVICE_TYPE})"
        );
        self.running = true;
        // TODO: Full mDNS registration via mdns-sd::ServiceDaemon
        // when mdns-sd dependency is wired in.
        debug!("mDNS registration stub — real implementation deferred");
        Ok(())
    }

    /// Stop discovery and unregister mDNS service.
    pub fn stop(&mut self) {
        if self.running {
            info!("LAN discovery stopped");
            self.running = false;
        }
    }

    /// Return a snapshot of currently discovered peers.
    pub fn peers(&self) -> HashMap<String, LanPeerInfo> {
        self.peers.read().clone()
    }

    /// Check if discovery is running.
    pub fn is_running(&self) -> bool {
        self.running
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_stop_lifecycle() {
        let mut disc = LanDiscovery::new(
            "dev-1".to_string(),
            "Test Mac".to_string(),
            0,
            "abc123".to_string(),
        );
        assert!(!disc.is_running());
        disc.start().unwrap();
        assert!(disc.is_running());
        disc.stop();
        assert!(!disc.is_running());
    }

    #[test]
    fn peers_initially_empty() {
        let disc = LanDiscovery::new(
            "dev-1".to_string(),
            "Test".to_string(),
            9090,
            "fp".to_string(),
        );
        assert!(disc.peers().is_empty());
    }
}
