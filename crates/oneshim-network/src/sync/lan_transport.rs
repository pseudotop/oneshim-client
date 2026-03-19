//! LanSyncTransport -- mDNS + HTTP(S) peer-to-peer sync orchestrator.
//!
//! Coordinates `LanDiscovery`, `LanPeerServer`, and per-peer reqwest clients
//! into a single `SyncTransport` implementation.
//!
//! ## Push flow
//!
//! 1. Serialize + encrypt the `ChangeSet` with the shared passphrase
//! 2. Discover LAN peers via mDNS
//! 3. POST encrypted payload to each peer's `/sync/push` endpoint
//! 4. Log successes/failures (best-effort fanout, not all-or-nothing)
//!
//! ## Pull flow
//!
//! 1. Discover LAN peers via mDNS
//! 2. GET `/sync/pull?since_wall_ms=...&since_counter=...` from peers
//! 3. First peer to return 200 with data wins; decrypt + deserialize
//! 4. Return `None` if no peer has new data
//!
//! Requires the `lan-sync` feature flag.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::RwLock;
use tracing::{debug, info, warn};

use oneshim_core::error::CoreError;
use oneshim_core::models::sync::{ChangeSet, PeerInfo};
use oneshim_core::ports::sync_transport::SyncTransport;
use oneshim_core::sync::Hlc;

use super::lan_discovery::{LanDiscovery, LanPeerInfo};
use super::lan_server::LanPeerServer;
use super::sync_crypto;

/// HTTP request timeout for peer-to-peer operations.
const PEER_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// LAN sync transport -- orchestrates mDNS discovery + HTTP peer server.
#[allow(dead_code)]
pub struct LanSyncTransport {
    discovery: parking_lot::Mutex<LanDiscovery>,
    server: parking_lot::Mutex<LanPeerServer>,
    http_client: reqwest::Client,
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

        // Start the HTTP peer server
        let actual_port = server.start(&cert_pem, &key_pem, lan_port).await?;

        // Update discovery with the actual bound port, then start mDNS
        if lan_advertise {
            // Re-create discovery with the real port (important when lan_port was 0)
            discovery = LanDiscovery::new(
                device_id.clone(),
                device_name.clone(),
                actual_port,
                server.fingerprint().to_string(),
            );
            discovery.start()?;
        }

        // Build the HTTP client for outbound peer requests
        let http_client = reqwest::Client::builder()
            .timeout(PEER_REQUEST_TIMEOUT)
            .danger_accept_invalid_certs(true) // Self-signed certs in LAN mode
            .build()
            .map_err(|e| CoreError::Network(format!("failed to build LAN HTTP client: {e}")))?;

        info!(
            device_id = %device_id,
            port = actual_port,
            advertise = lan_advertise,
            "LAN sync transport started"
        );

        Ok(Self {
            discovery: parking_lot::Mutex::new(discovery),
            server: parking_lot::Mutex::new(server),
            http_client,
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

    /// Get the bound server port.
    pub fn server_port(&self) -> u16 {
        self.server.lock().port()
    }

    /// Enqueue a changeset for peers to pull from this device's server.
    pub fn enqueue_outbound(&self, changeset: ChangeSet) {
        self.server.lock().enqueue_outbound(changeset);
    }

    /// Drain changesets received from peers (pushed to this device's server).
    pub fn drain_received(&self) -> Vec<ChangeSet> {
        self.server.lock().drain_received()
    }

    /// Refresh the verified peers list from mDNS discovery.
    ///
    /// Merges newly discovered peers into the verified peer map.
    /// Peers that are no longer advertised via mDNS are removed,
    /// but only when discovery is actively running (to avoid pruning
    /// manually added peers when mDNS is disabled).
    pub fn refresh_peers(&self) {
        let disc = self.discovery.lock();
        let discovered = disc.peers();
        let is_running = disc.is_running();
        drop(disc);

        let mut verified = self.verified_peers.write();

        // Add newly discovered peers
        for (id, peer) in &discovered {
            if !verified.contains_key(id) {
                debug!(device_id = %id, host = %peer.host, "new LAN peer discovered");
            }
            verified.insert(id.clone(), peer.clone());
        }

        // Only prune when discovery is actively running.
        // When discovery is off (e.g., tests), keep manually added peers.
        if is_running && !discovered.is_empty() {
            let active_ids: std::collections::HashSet<&String> = discovered.keys().collect();
            verified.retain(|id, _| active_ids.contains(id));
        }
    }

    /// Return a snapshot of the current verified peers.
    fn current_peers(&self) -> HashMap<String, LanPeerInfo> {
        self.verified_peers.read().clone()
    }

    /// Build the base URL for a peer's sync server.
    fn peer_url(peer: &LanPeerInfo, path: &str) -> String {
        // TODO: Switch to HTTPS when TLS is enabled
        format!("http://{}:{}{}", peer.host, peer.port, path)
    }

    /// Push encrypted changeset to a single peer. Returns Ok(true) on success.
    async fn push_to_peer(
        &self,
        peer_id: &str,
        peer: &LanPeerInfo,
        encrypted: &[u8],
    ) -> Result<bool, CoreError> {
        let url = Self::peer_url(peer, "/sync/push");

        let resp = self
            .http_client
            .post(&url)
            .header("content-type", "application/octet-stream")
            .body(encrypted.to_vec())
            .send()
            .await;

        match resp {
            Ok(r) if r.status().is_success() => {
                debug!(peer_id, "push to LAN peer succeeded");
                Ok(true)
            }
            Ok(r) => {
                let status = r.status();
                let body = r.text().await.unwrap_or_default();
                warn!(peer_id, %status, body, "push to LAN peer rejected");
                Ok(false)
            }
            Err(e) => {
                warn!(peer_id, error = %e, "push to LAN peer failed");
                Ok(false)
            }
        }
    }

    /// Pull encrypted changesets from a single peer. Returns decrypted changeset(s).
    async fn pull_from_peer(
        &self,
        peer_id: &str,
        peer: &LanPeerInfo,
        since: &Hlc,
    ) -> Result<Option<ChangeSet>, CoreError> {
        let url = format!(
            "{}?since_wall_ms={}&since_counter={}&device_id={}",
            Self::peer_url(peer, "/sync/pull"),
            since.wall_ms,
            since.counter,
            self.local_device_id,
        );

        let resp = self.http_client.get(&url).send().await;

        match resp {
            Ok(r) if r.status().as_u16() == 204 => {
                debug!(peer_id, "peer has no new data");
                Ok(None)
            }
            Ok(r) if r.status().is_success() => {
                let bytes = r
                    .bytes()
                    .await
                    .map_err(|e| CoreError::Network(format!("read pull body: {e}")))?;

                if bytes.is_empty() {
                    return Ok(None);
                }

                let plaintext = sync_crypto::decrypt(&self.passphrase, &bytes)?;
                let changesets: Vec<ChangeSet> = serde_json::from_slice(&plaintext)
                    .map_err(|e| CoreError::Internal(format!("deserialize pull response: {e}")))?;

                if changesets.is_empty() {
                    return Ok(None);
                }

                // Merge all pulled changesets into a single composite changeset
                // by concatenating their Vec fields and keeping the latest watermark.
                let mut iter = changesets.into_iter();
                let mut merged = iter.next().unwrap();
                for cs in iter {
                    merged.segments.extend(cs.segments);
                    merged.regimes.extend(cs.regimes);
                    merged.overrides.extend(cs.overrides);
                    merged.embeddings.extend(cs.embeddings);
                    merged.suggestions.extend(cs.suggestions);
                    merged.param_snapshots.extend(cs.param_snapshots);
                    merged.preferences.extend(cs.preferences);
                    // Keep the latest watermark
                    if cs.watermark.wall_ms > merged.watermark.wall_ms
                        || (cs.watermark.wall_ms == merged.watermark.wall_ms
                            && cs.watermark.counter > merged.watermark.counter)
                    {
                        merged.watermark = cs.watermark;
                    }
                }
                debug!(
                    peer_id,
                    origin = %merged.origin_device_id,
                    rows = merged.row_count(),
                    "pulled from LAN peer"
                );
                Ok(Some(merged))
            }
            Ok(r) => {
                let status = r.status();
                warn!(peer_id, %status, "pull from LAN peer returned unexpected status");
                Ok(None)
            }
            Err(e) => {
                warn!(peer_id, error = %e, "pull from LAN peer failed");
                Ok(None)
            }
        }
    }
}

#[async_trait]
impl SyncTransport for LanSyncTransport {
    /// Push a changeset to all discovered LAN peers.
    ///
    /// Best-effort fanout: logs errors per peer but does not fail the
    /// overall push unless serialization/encryption fails.
    async fn push(&self, changes: &ChangeSet) -> Result<(), CoreError> {
        self.refresh_peers();
        let peers = self.current_peers();
        if peers.is_empty() {
            debug!("no LAN peers discovered, push is a no-op");
            return Ok(());
        }

        let json = serde_json::to_vec(changes)
            .map_err(|e| CoreError::Internal(format!("serialize changeset: {e}")))?;
        let encrypted = sync_crypto::encrypt(&self.passphrase, &json)?;

        let mut success_count = 0u32;
        let mut fail_count = 0u32;

        for (peer_id, peer) in &peers {
            match self.push_to_peer(peer_id, peer, &encrypted).await {
                Ok(true) => success_count += 1,
                Ok(false) => fail_count += 1,
                Err(_) => fail_count += 1,
            }
        }

        debug!(
            total_peers = peers.len(),
            success_count, fail_count, "LAN push completed"
        );
        Ok(())
    }

    /// Pull changesets from the first available LAN peer.
    ///
    /// Iterates discovered peers in arbitrary order and returns the first
    /// successful response containing data. Returns `None` if no peer has
    /// new data.
    async fn pull(&self, since: &Hlc) -> Result<Option<ChangeSet>, CoreError> {
        self.refresh_peers();
        let peers = self.current_peers();
        if peers.is_empty() {
            return Ok(None);
        }

        for (peer_id, peer) in &peers {
            match self.pull_from_peer(peer_id, peer, since).await {
                Ok(Some(cs)) => return Ok(Some(cs)),
                Ok(None) => continue,
                Err(e) => {
                    debug!(peer_id, error = %e, "pull from peer failed, trying next");
                    continue;
                }
            }
        }

        Ok(None)
    }

    /// Discover peers via mDNS.
    async fn discover_peers(&self) -> Result<Vec<PeerInfo>, CoreError> {
        self.refresh_peers();
        let disc_peers = self.current_peers();
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
    use oneshim_core::models::sync::ChangeSetKind;

    fn test_changeset() -> ChangeSet {
        ChangeSet {
            kind: ChangeSetKind::Data,
            origin_device_id: "dev-a".to_string(),
            origin_device_name: "Test A".to_string(),
            watermark: Hlc {
                wall_ms: 100,
                counter: 1,
                device_id: "dev-a".to_string(),
            },
            segments: vec![serde_json::json!({"id": "seg-1"})],
            ..Default::default()
        }
    }

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

        assert!(transport.server_port() > 0);

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

    #[tokio::test]
    async fn push_to_local_server_roundtrip() {
        // Start two transports: "sender" pushes to "receiver"'s server.
        let passphrase = "shared-secret-123";

        // Start receiver
        let receiver = LanSyncTransport::start(
            "receiver".to_string(),
            "Receiver".to_string(),
            passphrase.to_string(),
            b"cert".to_vec(),
            b"key".to_vec(),
            "fp-recv".to_string(),
            0,
            false,
        )
        .await
        .unwrap();

        let receiver_port = receiver.server_port();

        // Manually inject receiver as a verified peer in sender's peer map
        let sender = LanSyncTransport::start(
            "sender".to_string(),
            "Sender".to_string(),
            passphrase.to_string(),
            b"cert".to_vec(),
            b"key".to_vec(),
            "fp-send".to_string(),
            0,
            false,
        )
        .await
        .unwrap();

        // Inject receiver as a known peer
        sender.verified_peers.write().insert(
            "receiver".to_string(),
            LanPeerInfo {
                device_id: "receiver".to_string(),
                device_name: "Receiver".to_string(),
                host: "127.0.0.1".to_string(),
                port: receiver_port,
                fingerprint: "fp-recv".to_string(),
                version: "1".to_string(),
            },
        );

        // Yield to let the server task start accepting connections
        tokio::task::yield_now().await;

        // Push a changeset
        let cs = test_changeset();
        sender.push(&cs).await.unwrap();

        // Verify receiver got it
        let received = receiver.drain_received();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].origin_device_id, "dev-a");

        sender.stop();
        receiver.stop();
    }

    #[tokio::test]
    async fn pull_from_peer_server() {
        let passphrase = "pull-test-pass";

        // Start a server with an outbound changeset
        let provider = LanSyncTransport::start(
            "provider".to_string(),
            "Provider".to_string(),
            passphrase.to_string(),
            b"cert".to_vec(),
            b"key".to_vec(),
            "fp-prov".to_string(),
            0,
            false,
        )
        .await
        .unwrap();

        let provider_port = provider.server_port();
        provider.enqueue_outbound(test_changeset());

        // Start a consumer and inject provider as a peer
        let consumer = LanSyncTransport::start(
            "consumer".to_string(),
            "Consumer".to_string(),
            passphrase.to_string(),
            b"cert".to_vec(),
            b"key".to_vec(),
            "fp-cons".to_string(),
            0,
            false,
        )
        .await
        .unwrap();

        consumer.verified_peers.write().insert(
            "provider".to_string(),
            LanPeerInfo {
                device_id: "provider".to_string(),
                device_name: "Provider".to_string(),
                host: "127.0.0.1".to_string(),
                port: provider_port,
                fingerprint: "fp-prov".to_string(),
                version: "1".to_string(),
            },
        );

        // Yield to let the server task start accepting connections
        tokio::task::yield_now().await;

        // Pull from provider
        let pulled = consumer.pull(&Hlc::default()).await.unwrap();

        assert!(pulled.is_some());
        let cs = pulled.unwrap();
        assert_eq!(cs.origin_device_id, "dev-a");
        assert_eq!(cs.segments.len(), 1);

        provider.stop();
        consumer.stop();
    }

    #[tokio::test]
    async fn pull_wrong_passphrase_returns_none() {
        // Server uses one passphrase, client uses another
        let provider = LanSyncTransport::start(
            "provider".to_string(),
            "Provider".to_string(),
            "server-pass".to_string(),
            b"cert".to_vec(),
            b"key".to_vec(),
            "fp".to_string(),
            0,
            false,
        )
        .await
        .unwrap();

        provider.enqueue_outbound(test_changeset());
        let provider_port = provider.server_port();

        let consumer = LanSyncTransport::start(
            "consumer".to_string(),
            "Consumer".to_string(),
            "wrong-pass".to_string(),
            b"cert".to_vec(),
            b"key".to_vec(),
            "fp".to_string(),
            0,
            false,
        )
        .await
        .unwrap();

        consumer.verified_peers.write().insert(
            "provider".to_string(),
            LanPeerInfo {
                device_id: "provider".to_string(),
                device_name: "Provider".to_string(),
                host: "127.0.0.1".to_string(),
                port: provider_port,
                fingerprint: "fp".to_string(),
                version: "1".to_string(),
            },
        );

        tokio::task::yield_now().await;

        // Pull should fail to decrypt and return None (graceful degradation)
        let pulled = consumer.pull(&Hlc::default()).await;
        // The error from decryption is logged but should not crash
        // It may return Err or Ok(None) depending on how the error propagates
        match pulled {
            Ok(None) => {} // graceful: no data
            Err(_) => {}   // also acceptable: decryption error propagated
            Ok(Some(_)) => panic!("should not succeed with wrong passphrase"),
        }

        provider.stop();
        consumer.stop();
    }
}
