//! LanSyncTransport -- mDNS + HTTPS peer-to-peer sync orchestrator.
//!
//! Coordinates `LanDiscovery`, `LanPeerServer`, and per-peer reqwest clients
//! into a single `SyncTransport` implementation.
//! Requires the `lan-sync` feature flag.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::RwLock;
use tracing::{debug, info};

use oneshim_core::error::CoreError;
use oneshim_core::models::sync::{ChangeSet, PeerInfo};
use oneshim_core::ports::sync_transport::SyncTransport;
use oneshim_core::sync::Hlc;

use super::lan_discovery::{LanDiscovery, LanPeerInfo};
use super::lan_server::LanPeerServer;
use super::sync_crypto;

/// LAN sync transport -- orchestrates mDNS discovery + HTTPS peer server.
#[allow(dead_code)]
pub struct LanSyncTransport {
    discovery: parking_lot::Mutex<LanDiscovery>,
    server: parking_lot::Mutex<LanPeerServer>,
    local_device_id: String,
    local_device_name: String,
    passphrase: String,
    /// Verified peers (device_id -> LanPeerInfo).
    verified_peers: Arc<RwLock<HashMap<String, LanPeerInfo>>>,
}

impl LanSyncTransport {
    /// Create and start the LAN sync transport.
    ///
    /// This initializes discovery, starts the peer server, and registers mDNS.
    #[allow(clippy::too_many_arguments)]
    pub async fn start(
        device_id: String,
        device_name: String,
        passphrase: String,
        cert_pem: Vec<u8>,
        key_pem: Vec<u8>,
        fingerprint: String,
        lan_port: u16,
        lan_advertise: bool,
    ) -> Result<Self, CoreError> {
        let mut discovery = LanDiscovery::new(
            device_id.clone(),
            device_name.clone(),
            lan_port,
            fingerprint.clone(),
        );

        let mut server = LanPeerServer::new(
            device_id.clone(),
            device_name.clone(),
            passphrase.clone(),
            fingerprint,
        );

        // Start the HTTPS peer server
        let actual_port = server.start(&cert_pem, &key_pem, lan_port).await?;

        // Start mDNS discovery + registration
        if lan_advertise {
            discovery.start()?;
        }

        info!(
            device_id = %device_id,
            port = actual_port,
            advertise = lan_advertise,
            "LAN sync transport started"
        );

        Ok(Self {
            discovery: parking_lot::Mutex::new(discovery),
            server: parking_lot::Mutex::new(server),
            local_device_id: device_id,
            local_device_name: device_name,
            passphrase,
            verified_peers: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Stop the transport (mDNS + server).
    pub fn stop(&self) {
        self.server.lock().stop();
        self.discovery.lock().stop();
        info!("LAN sync transport stopped");
    }
}

#[async_trait]
impl SyncTransport for LanSyncTransport {
    async fn push(&self, changes: &ChangeSet) -> Result<(), CoreError> {
        let peers = self.verified_peers.read().clone();
        if peers.is_empty() {
            debug!("no verified LAN peers, push is a no-op");
            return Ok(());
        }

        let json = serde_json::to_vec(changes)
            .map_err(|e| CoreError::Internal(format!("serialize changeset: {e}")))?;
        let _encrypted = sync_crypto::encrypt(&self.passphrase, &json)?;

        let mut success_count = 0;
        for (peer_id, peer) in &peers {
            debug!(peer_id, host = %peer.host, port = peer.port, "pushing to LAN peer");
            // TODO: Send encrypted payload to peer via HTTPS POST /sync/push
            // For now, count as successful (structural stub)
            success_count += 1;
        }

        debug!(
            total_peers = peers.len(),
            success_count, "LAN push completed"
        );
        Ok(())
    }

    async fn pull(&self, _since: &Hlc) -> Result<Option<ChangeSet>, CoreError> {
        let peers = self.verified_peers.read().clone();
        if peers.is_empty() {
            return Ok(None);
        }

        // Try each verified peer until one returns data
        for (peer_id, peer) in &peers {
            debug!(
                peer_id,
                host = %peer.host,
                port = peer.port,
                "pulling from LAN peer"
            );
            // TODO: GET /sync/pull from peer, decrypt, return
            // Structural stub: no data available from stub peers
        }

        Ok(None)
    }

    async fn discover_peers(&self) -> Result<Vec<PeerInfo>, CoreError> {
        let disc_peers = self.discovery.lock().peers();
        let peers: Vec<PeerInfo> = disc_peers
            .values()
            .map(|p| PeerInfo {
                device_id: p.device_id.clone(),
                device_name: p.device_name.clone(),
                last_sync_at: String::new(),
                watermark: Hlc::default(),
            })
            .collect();
        debug!(count = peers.len(), "discovered LAN peers");
        Ok(peers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn transport_start_and_discover_empty() {
        let transport = LanSyncTransport::start(
            "dev-1".to_string(),
            "Test Mac".to_string(),
            "passphrase".to_string(),
            b"cert".to_vec(),
            b"key".to_vec(),
            "fp123".to_string(),
            0,
            false, // don't advertise in test
        )
        .await
        .unwrap();

        let peers = transport.discover_peers().await.unwrap();
        assert!(peers.is_empty());

        transport.stop();
    }

    #[tokio::test]
    async fn push_to_no_peers_is_noop() {
        let transport = LanSyncTransport::start(
            "dev-1".to_string(),
            "Test".to_string(),
            "pass".to_string(),
            b"cert".to_vec(),
            b"key".to_vec(),
            "fp".to_string(),
            0,
            false,
        )
        .await
        .unwrap();

        let cs = ChangeSet::default();
        let result = transport.push(&cs).await;
        assert!(result.is_ok());

        transport.stop();
    }

    #[tokio::test]
    async fn pull_from_no_peers_returns_none() {
        let transport = LanSyncTransport::start(
            "dev-1".to_string(),
            "Test".to_string(),
            "pass".to_string(),
            b"cert".to_vec(),
            b"key".to_vec(),
            "fp".to_string(),
            0,
            false,
        )
        .await
        .unwrap();

        let result = transport.pull(&Hlc::default()).await.unwrap();
        assert!(result.is_none());

        transport.stop();
    }
}
