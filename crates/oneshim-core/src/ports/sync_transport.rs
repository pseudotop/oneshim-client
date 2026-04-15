//! Transport port for moving changesets between devices during cross-device sync.

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::sync::{ChangeSet, PeerInfo};
use crate::sync::Hlc;

/// Transport port: moves changesets between devices.
///
/// Three implementations planned (selected via SyncConfig::transport):
/// - `FileSyncTransport`   (oneshim-storage)  -- encrypted JSON in shared folder
/// - `RemoteSyncTransport` (oneshim-network)  -- REST/gRPC to sync endpoint
/// - `LanSyncTransport`    (oneshim-network)  -- mDNS + direct TCP (Phase 3b)
///
/// The SyncEngine holds an `Arc<dyn SyncTransport>` and uses it for
/// push/pull without knowing which transport is active.
#[async_trait]
pub trait SyncTransport: Send + Sync {
    /// Push a local changeset to the transport for other devices to pull.
    async fn push(&self, changes: &ChangeSet) -> Result<(), CoreError>;

    /// Pull the next changeset from the transport since the given watermark.
    ///
    /// Returns `None` if no new changes are available.
    async fn pull(&self, since: &Hlc) -> Result<Option<ChangeSet>, CoreError>;

    /// Discover known peer devices via the transport.
    ///
    /// For file transport: list device folders in the sync directory.
    /// For remote transport: query the sync endpoint's peer registry.
    /// For LAN transport: mDNS service discovery.
    async fn discover_peers(&self) -> Result<Vec<PeerInfo>, CoreError>;

    /// Remove a peer from the transport's known-peers list.
    ///
    /// For LAN transport: evicts the peer from the verified-peers map.
    /// For remote transport: sends a DELETE request to the peer registry.
    /// For file transport: removes changeset files originating from the peer.
    ///
    /// Default: no-op (transports that don't maintain peer state).
    async fn forget_peer(&self, _device_id: &str) -> Result<(), CoreError> {
        Ok(())
    }
}
