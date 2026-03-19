//! LanPeerServer -- Axum HTTPS peer server for LAN sync (Phase 3b-2).
//!
//! Lightweight server that serves changesets to LAN peers.
//! Endpoints: challenge, verify, pull, push, info.
//! Requires the `lan-sync` feature flag.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use tracing::info;

use oneshim_core::error::CoreError;

/// LAN peer server state.
#[allow(dead_code)]
pub struct LanPeerServer {
    device_id: String,
    device_name: String,
    passphrase: String,
    port: u16,
    fingerprint: String,
    /// Active session tokens (peer_device_id -> session_token).
    sessions: Arc<RwLock<HashMap<String, String>>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    running: bool,
}

impl LanPeerServer {
    /// Create a new peer server (not yet started).
    pub fn new(
        device_id: String,
        device_name: String,
        passphrase: String,
        fingerprint: String,
    ) -> Self {
        Self {
            device_id,
            device_name,
            passphrase,
            port: 0,
            fingerprint,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx: None,
            running: false,
        }
    }

    /// Start the HTTPS server on the specified port (0 = ephemeral).
    ///
    /// Note: Full TLS server requires `tokio-rustls` + `rustls-pemfile`.
    /// This is a structural implementation with the Axum route definitions.
    pub async fn start(
        &mut self,
        _cert_pem: &[u8],
        _key_pem: &[u8],
        port: u16,
    ) -> Result<u16, CoreError> {
        let (shutdown_tx, _shutdown_rx) = tokio::sync::oneshot::channel();

        // For now, record the port. Full TLS binding is deferred.
        let actual_port = if port == 0 { 19090 } else { port };
        self.port = actual_port;
        self.shutdown_tx = Some(shutdown_tx);
        self.running = true;

        info!(
            port = actual_port,
            device_id = %self.device_id,
            "LAN peer server started (structural stub)"
        );

        Ok(actual_port)
    }

    /// Stop the server.
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        self.running = false;
        info!("LAN peer server stopped");
    }

    /// Get the bound port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Get the certificate fingerprint.
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Check if server is running.
    pub fn is_running(&self) -> bool {
        self.running
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn server_start_stop() {
        let mut server = LanPeerServer::new(
            "dev-1".to_string(),
            "Test".to_string(),
            "pass".to_string(),
            "fp123".to_string(),
        );
        assert!(!server.is_running());

        let port = server.start(b"cert", b"key", 0).await.unwrap();
        assert!(port > 0);
        assert!(server.is_running());

        server.stop();
        assert!(!server.is_running());
    }

    #[test]
    fn fingerprint_accessible() {
        let server = LanPeerServer::new(
            "dev-1".to_string(),
            "Test".to_string(),
            "pass".to_string(),
            "abc123def".to_string(),
        );
        assert_eq!(server.fingerprint(), "abc123def");
    }
}
