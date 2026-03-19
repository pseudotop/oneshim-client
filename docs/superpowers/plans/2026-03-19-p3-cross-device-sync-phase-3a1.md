# P3 Cross-Device Sync Phase 3a-1 — Foundational Primitives

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish the sync foundation: trait definitions, domain models, config section, consent field, and device identity. Everything is inert -- no data moves between devices until Phase 3a-2.

**Architecture:** All new abstractions live in `oneshim-core` (models, ports, config, consent). Device identity generation is the only storage-side change and uses the existing `SqliteStorage` + V14 `device_identity` table. No new crate. No SyncEngine. No transports. No scheduler loop.

**Tech Stack:** Rust, serde, chrono, async_trait, uuid, oneshim-core, oneshim-storage

**Spec:** `docs/superpowers/specs/2026-03-19-p3-cross-device-sync-design.md`

**Already done (DO NOT re-implement):**
- HLC implementation: `crates/oneshim-core/src/sync/hlc.rs` (Hlc struct, tick, merge, ordering, 7 tests)
- HLC module export: `crates/oneshim-core/src/sync/mod.rs`
- V14 schema migration: `crates/oneshim-storage/src/migration.rs` (HLC columns, tombstone columns, sync_peers, device_identity tables)

---

## File Map

### New files

| File | Responsibility |
|------|----------------|
| `crates/oneshim-core/src/config/sections/sync.rs` | `SyncTransportKind` enum + `SyncConfig` section (enabled, transport, interval, device_name) |
| `crates/oneshim-core/src/models/sync.rs` | `ChangeSet`, `ChangeSetKind`, `SyncResult`, `PeerInfo` domain models |
| `crates/oneshim-core/src/ports/change_extractor.rs` | `ChangeExtractor` read-side port trait |
| `crates/oneshim-core/src/ports/change_merger.rs` | `ChangeMerger` write-side port trait |
| `crates/oneshim-core/src/ports/sync_transport.rs` | `SyncTransport` network port trait |

### Modified files

| File | Change |
|------|--------|
| `crates/oneshim-core/src/config/sections/mod.rs` | Add `mod sync;` + `pub use sync::*;` |
| `crates/oneshim-core/src/config/mod.rs` | Add `sync: SyncConfig` field to `AppConfig` + update `default_config()` |
| `crates/oneshim-core/src/consent.rs` | Add `cross_device_sync: bool` to `ConsentPermissions` (Tier 5) |
| `crates/oneshim-core/src/models/mod.rs` | Add `pub mod sync;` |
| `crates/oneshim-core/src/ports/mod.rs` | Add `pub mod change_extractor;` + `pub mod change_merger;` + `pub mod sync_transport;` |
| `crates/oneshim-storage/src/sqlite/mod.rs` | Add `ensure_device_identity()` method to `SqliteStorage` |

---

## Task 1: Add SyncConfig section (oneshim-core)

**Files:**
- New: `crates/oneshim-core/src/config/sections/sync.rs`
- Modify: `crates/oneshim-core/src/config/sections/mod.rs`
- Modify: `crates/oneshim-core/src/config/mod.rs`

- [ ] **Step 1: Create `sync.rs` config section**

Create file `crates/oneshim-core/src/config/sections/sync.rs`:

```rust
// Cross-device sync configuration (Phase 3 — P3).
use serde::{Deserialize, Serialize};

/// Sync transport selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncTransportKind {
    /// Push/pull via REST/gRPC to a remote sync endpoint.
    Remote,
    /// Read/write encrypted JSON to a shared folder (Dropbox, iCloud, NAS).
    File,
    /// mDNS discovery + direct TCP between devices on the same LAN (Phase 3b).
    Lan,
}

impl Default for SyncTransportKind {
    fn default() -> Self {
        Self::File
    }
}

/// Cross-device sync configuration.
///
/// Controls whether activity data is synchronized between devices
/// owned by the same user. Default: disabled. Both `enabled` AND
/// `ConsentPermissions::cross_device_sync` must be true for any
/// data to leave the device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Master switch. When false, all sync operations are disabled.
    #[serde(default)]
    pub enabled: bool,

    /// Selected transport mechanism.
    #[serde(default)]
    pub transport: SyncTransportKind,

    /// Interval between periodic sync cycles (seconds). Default: 300 (5 min).
    #[serde(default = "default_sync_interval_secs")]
    pub interval_secs: u64,

    /// Include raw `content_activities_json` in synced segments.
    /// Default: false (only dominant_category, duration, app_breakdown,
    /// llm_summary are synced).
    #[serde(default)]
    pub include_content_activities: bool,

    /// Include `original_text` in synced embedding vectors.
    /// Default: false (only vector blobs sync).
    #[serde(default)]
    pub include_embedding_text: bool,

    /// Human-readable name for this device (e.g., "Work MacBook").
    /// Shown to the user on peer devices. Defaults to OS hostname.
    #[serde(default = "default_device_name")]
    pub device_name: String,
}

fn default_sync_interval_secs() -> u64 {
    300
}

fn default_device_name() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "unknown".to_string())
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            transport: SyncTransportKind::default(),
            interval_secs: default_sync_interval_secs(),
            include_content_activities: false,
            include_embedding_text: false,
            device_name: default_device_name(),
        }
    }
}
```

- [ ] **Step 2: Add `hostname` dependency to `oneshim-core/Cargo.toml`**

Add to `[dependencies]`:

```toml
hostname = "0.4"
```

- [ ] **Step 3: Register sync module in sections/mod.rs**

In `crates/oneshim-core/src/config/sections/mod.rs`, add after the `mod storage;` line:

```rust
mod sync;
```

And after the `pub use storage::*;` line:

```rust
pub use sync::*;
```

- [ ] **Step 4: Add `sync` field to `AppConfig`**

In `crates/oneshim-core/src/config/mod.rs`, add after the `analysis: AnalysisConfig` field:

```rust
    #[serde(default)]
    pub sync: SyncConfig,
```

- [ ] **Step 5: Add `sync` to `AppConfig::default_config()`**

In the `default_config()` method body, add after `analysis: AnalysisConfig::default(),`:

```rust
            sync: SyncConfig::default(),
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check -p oneshim-core`

- [ ] **Step 7: Add SyncConfig unit tests**

Append to `sync.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_config_default_is_disabled() {
        let config = SyncConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.transport, SyncTransportKind::File);
        assert_eq!(config.interval_secs, 300);
        assert!(!config.include_content_activities);
        assert!(!config.include_embedding_text);
        assert!(!config.device_name.is_empty());
    }

    #[test]
    fn sync_config_serde_roundtrip() {
        let config = SyncConfig {
            enabled: true,
            transport: SyncTransportKind::Remote,
            interval_secs: 600,
            include_content_activities: true,
            include_embedding_text: false,
            device_name: "Test Machine".to_string(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: SyncConfig = serde_json::from_str(&json).unwrap();
        assert!(parsed.enabled);
        assert_eq!(parsed.transport, SyncTransportKind::Remote);
        assert_eq!(parsed.interval_secs, 600);
        assert!(parsed.include_content_activities);
        assert_eq!(parsed.device_name, "Test Machine");
    }

    #[test]
    fn sync_config_empty_json_uses_defaults() {
        let parsed: SyncConfig = serde_json::from_str("{}").unwrap();
        assert!(!parsed.enabled);
        assert_eq!(parsed.transport, SyncTransportKind::File);
        assert_eq!(parsed.interval_secs, 300);
    }

    #[test]
    fn sync_transport_kind_serde_snake_case() {
        let json = serde_json::to_string(&SyncTransportKind::Remote).unwrap();
        assert_eq!(json, "\"remote\"");
        let json = serde_json::to_string(&SyncTransportKind::Lan).unwrap();
        assert_eq!(json, "\"lan\"");
    }

    #[test]
    fn app_config_with_sync_section_deserializes() {
        // Existing configs without a "sync" key must still parse
        // (the #[serde(default)] on AppConfig::sync handles this).
        let minimal = r#"{ "server": { "base_url": "http://localhost:8000", "request_timeout_ms": 5000, "sse_max_retry_secs": 30 }, "monitor": { "poll_interval_ms": 1000, "sync_interval_ms": 10000, "heartbeat_interval_ms": 60000, "idle_threshold_secs": 300, "process_interval_secs": 10, "process_monitoring": true, "input_activity": true, "upload_enabled": false }, "storage": { "retention_days": 30, "max_storage_mb": 500 }, "vision": { "capture_enabled": false, "capture_throttle_ms": 5000, "thumbnail_width": 480, "thumbnail_height": 270, "ocr_enabled": false, "privacy_mode": false } }"#;
        let config: crate::config::AppConfig = serde_json::from_str(minimal).unwrap();
        assert!(!config.sync.enabled);
    }
}
```

- [ ] **Step 8: Run tests**

Run: `cargo test -p oneshim-core -- sync_config`

- [ ] **Step 9: Commit**

```
feat(core): add SyncConfig section for cross-device sync (P3 Phase 3a-1)
```

---

## Task 2: Add cross_device_sync to ConsentPermissions (oneshim-core)

**Files:**
- Modify: `crates/oneshim-core/src/consent.rs`

- [ ] **Step 1: Add Tier 5 field to ConsentPermissions**

In `crates/oneshim-core/src/consent.rs`, add after the `activity_pattern_learning` field inside `ConsentPermissions`:

```rust
    // --- Tier 5: Cross-Device Sync ---
    /// Permits cross-device synchronization of activity data.
    /// GDPR Article 6 -- processing requires explicit consent for data
    /// transfer between devices, even when both are owned by the same user.
    #[serde(default)]
    pub cross_device_sync: bool,
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p oneshim-core`

- [ ] **Step 3: Add consent tests for cross_device_sync**

Add the following tests to the existing `mod tests` block in `consent.rs`:

```rust
    #[test]
    fn consent_permissions_cross_device_sync_default_false() {
        let perms = ConsentPermissions::default();
        assert!(
            !perms.cross_device_sync,
            "cross_device_sync must default to false (GDPR Article 6)"
        );
    }

    #[test]
    fn consent_cross_device_sync_permission_check() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("consent.json");
        let mut manager = ConsentManager::new(path);

        // Without cross_device_sync
        let perms = ConsentPermissions::default();
        manager.grant_consent(perms, 30).unwrap();
        assert!(!manager.is_permitted(|p| p.cross_device_sync));

        // With cross_device_sync
        let perms_with_sync = ConsentPermissions {
            cross_device_sync: true,
            ..Default::default()
        };
        manager.grant_consent(perms_with_sync, 30).unwrap();
        assert!(manager.is_permitted(|p| p.cross_device_sync));
    }

    #[test]
    fn consent_permissions_legacy_json_without_cross_device_sync() {
        // Records written before cross_device_sync was added must deserialize.
        let legacy_json = r#"{
            "screen_capture": true,
            "ocr_processing": false,
            "telemetry": true,
            "process_monitoring": true,
            "input_activity": false,
            "window_title_collection": false,
            "app_usage_analytics": false,
            "clipboard_monitoring": false,
            "file_access_monitoring": false,
            "activity_pattern_learning": false
        }"#;
        let perms: ConsentPermissions = serde_json::from_str(legacy_json).unwrap();
        assert!(perms.screen_capture);
        assert!(
            !perms.cross_device_sync,
            "missing field must default to false"
        );
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p oneshim-core -- consent`

- [ ] **Step 5: Commit**

```
feat(core): add cross_device_sync consent field (Tier 5, GDPR Article 6)
```

---

## Task 3: Create sync domain models (oneshim-core)

**Files:**
- New: `crates/oneshim-core/src/models/sync.rs`
- Modify: `crates/oneshim-core/src/models/mod.rs`

- [ ] **Step 1: Create sync.rs models**

Create file `crates/oneshim-core/src/models/sync.rs`:

```rust
//! Domain models for cross-device sync (P3).
//!
//! These are the data structures that flow between ChangeExtractor,
//! SyncTransport, and ChangeMerger. They carry no logic -- pure DTOs.

use serde::{Deserialize, Serialize};

use crate::sync::Hlc;

/// The kind of changeset being transmitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeSetKind {
    /// Normal data synchronization.
    Data,
    /// GDPR Article 17 deletion propagation.
    /// All peers receiving this changeset MUST perform local erasure.
    DeletionEvent,
}

impl Default for ChangeSetKind {
    fn default() -> Self {
        Self::Data
    }
}

/// A batch of changes to sync between devices.
///
/// Each Vec field holds serialized rows for one syncable table.
/// In Phase 3a-1 the row types are opaque `serde_json::Value`
/// placeholders; Phase 3a-2 will replace them with typed row structs
/// when the ChangeExtractor impl is built.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChangeSet {
    pub kind: ChangeSetKind,
    pub origin_device_id: String,
    pub origin_device_name: String,
    pub watermark: Hlc,
    pub segments: Vec<serde_json::Value>,
    pub regimes: Vec<serde_json::Value>,
    pub overrides: Vec<serde_json::Value>,
    pub embeddings: Vec<serde_json::Value>,
    pub suggestions: Vec<serde_json::Value>,
    pub param_snapshots: Vec<serde_json::Value>,
    pub preferences: Vec<serde_json::Value>,
}

impl ChangeSet {
    /// Returns true when the changeset carries no data rows.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
            && self.regimes.is_empty()
            && self.overrides.is_empty()
            && self.embeddings.is_empty()
            && self.suggestions.is_empty()
            && self.param_snapshots.is_empty()
            && self.preferences.is_empty()
    }

    /// Total number of rows across all tables.
    pub fn row_count(&self) -> usize {
        self.segments.len()
            + self.regimes.len()
            + self.overrides.len()
            + self.embeddings.len()
            + self.suggestions.len()
            + self.param_snapshots.len()
            + self.preferences.len()
    }
}

/// Result of applying a changeset via ChangeMerger.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncResult {
    /// Rows successfully applied (inserted or updated via LWW).
    pub applied: usize,
    /// Rows skipped because local HLC was higher (lost LWW race).
    pub skipped_lww: usize,
    /// Rows skipped because they already exist (duplicate PK).
    pub skipped_dup: usize,
    /// Rows soft-deleted via tombstone propagation.
    pub tombstoned: usize,
    /// The new high-watermark HLC after applying the changeset.
    pub new_watermark: Hlc,
}

/// Information about a known sync peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// Unique device identifier (UUID v4).
    pub device_id: String,
    /// Human-readable device name (e.g., "Work MacBook").
    pub device_name: String,
    /// ISO-8601 timestamp of last successful sync.
    pub last_sync_at: String,
    /// Peer's high-watermark HLC at last sync.
    pub watermark: Hlc,
}
```

- [ ] **Step 2: Register sync module in models/mod.rs**

In `crates/oneshim-core/src/models/mod.rs`, add after the `pub mod suggestion;` line:

```rust
pub mod sync;
```

(Keep alphabetical order -- it goes between `suggestion` and `system`.)

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p oneshim-core`

- [ ] **Step 4: Add model unit tests**

Append to `models/sync.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn sample_hlc() -> Hlc {
        Hlc {
            wall_ms: 1710859200000,
            counter: 42,
            device_id: "dev-a".to_string(),
        }
    }

    #[test]
    fn changeset_is_empty_when_default() {
        let cs = ChangeSet::default();
        assert!(cs.is_empty());
        assert_eq!(cs.row_count(), 0);
    }

    #[test]
    fn changeset_row_count_sums_all_tables() {
        let mut cs = ChangeSet::default();
        cs.segments.push(serde_json::json!({"id": "seg-1"}));
        cs.regimes.push(serde_json::json!({"id": "reg-1"}));
        cs.regimes.push(serde_json::json!({"id": "reg-2"}));
        assert_eq!(cs.row_count(), 3);
        assert!(!cs.is_empty());
    }

    #[test]
    fn changeset_serde_roundtrip() {
        let cs = ChangeSet {
            kind: ChangeSetKind::Data,
            origin_device_id: "dev-a".to_string(),
            origin_device_name: "Work Mac".to_string(),
            watermark: sample_hlc(),
            segments: vec![serde_json::json!({"id": "seg-1"})],
            ..Default::default()
        };
        let json = serde_json::to_string(&cs).unwrap();
        let parsed: ChangeSet = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.origin_device_id, "dev-a");
        assert_eq!(parsed.segments.len(), 1);
        assert!(parsed.regimes.is_empty());
    }

    #[test]
    fn changeset_kind_serde_snake_case() {
        let json = serde_json::to_string(&ChangeSetKind::DeletionEvent).unwrap();
        assert_eq!(json, "\"deletion_event\"");
        let parsed: ChangeSetKind = serde_json::from_str("\"data\"").unwrap();
        assert_eq!(parsed, ChangeSetKind::Data);
    }

    #[test]
    fn sync_result_default_zeros() {
        let result = SyncResult::default();
        assert_eq!(result.applied, 0);
        assert_eq!(result.skipped_lww, 0);
        assert_eq!(result.skipped_dup, 0);
        assert_eq!(result.tombstoned, 0);
    }

    #[test]
    fn peer_info_serde_roundtrip() {
        let peer = PeerInfo {
            device_id: "dev-b".to_string(),
            device_name: "Home Desktop".to_string(),
            last_sync_at: "2026-03-19T12:00:00Z".to_string(),
            watermark: sample_hlc(),
        };
        let json = serde_json::to_string(&peer).unwrap();
        let parsed: PeerInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.device_id, "dev-b");
        assert_eq!(parsed.device_name, "Home Desktop");
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p oneshim-core -- models::sync`

- [ ] **Step 6: Commit**

```
feat(core): add sync domain models (ChangeSet, SyncResult, PeerInfo)
```

---

## Task 4: Create ChangeExtractor port trait (oneshim-core)

**Files:**
- New: `crates/oneshim-core/src/ports/change_extractor.rs`
- Modify: `crates/oneshim-core/src/ports/mod.rs`

- [ ] **Step 1: Create change_extractor.rs**

Create file `crates/oneshim-core/src/ports/change_extractor.rs`:

```rust
use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::sync::ChangeSet;
use crate::sync::Hlc;

/// Read-side port: extracts local changes for outbound sync.
///
/// Implemented by oneshim-storage (SQLite queries against syncable
/// tables). The SyncEngine calls this to build an outbound ChangeSet
/// containing all rows modified since the peer's last-known watermark.
#[async_trait]
pub trait ChangeExtractor: Send + Sync {
    /// Get local changes since the given HLC watermark.
    ///
    /// Returns a ChangeSet containing all rows where
    /// `(hlc_wall_ms, hlc_counter, origin_device_id) > since`.
    async fn get_changes_since(&self, since: &Hlc) -> Result<ChangeSet, CoreError>;

    /// Get the current device's high-watermark HLC.
    ///
    /// This is the maximum HLC across all syncable tables on this device.
    async fn local_watermark(&self) -> Result<Hlc, CoreError>;
}
```

- [ ] **Step 2: Register in ports/mod.rs**

In `crates/oneshim-core/src/ports/mod.rs`, add after the `pub mod calibration_store;` line:

```rust
pub mod change_extractor;
```

(Alphabetical order -- between `calibration_store` and `compressor`.)

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p oneshim-core`

- [ ] **Step 4: Commit**

```
feat(core): add ChangeExtractor port trait for sync read-side
```

---

## Task 5: Create ChangeMerger port trait (oneshim-core)

**Files:**
- New: `crates/oneshim-core/src/ports/change_merger.rs`
- Modify: `crates/oneshim-core/src/ports/mod.rs`

- [ ] **Step 1: Create change_merger.rs**

Create file `crates/oneshim-core/src/ports/change_merger.rs`:

```rust
use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::sync::{ChangeSet, SyncResult};

/// Write-side port: applies inbound changesets with LWW conflict resolution.
///
/// Implemented by oneshim-storage (SQLite queries with HLC comparison).
/// The SyncEngine calls this after pulling a remote ChangeSet from the
/// transport to merge it into the local database.
///
/// Conflict resolution rules:
/// - Append-only tables (segments, overrides, param_snapshots): insert if PK absent.
/// - LWW tables (regimes, suggestions, embeddings): compare HLC, higher wins.
/// - Tombstoned rows: propagate soft-delete via is_deleted + deleted_at.
/// - DeletionEvent changeset: hard-delete all rows from the originating device.
#[async_trait]
pub trait ChangeMerger: Send + Sync {
    /// Apply a remote changeset, resolving conflicts via HLC.
    ///
    /// Returns statistics on applied/skipped/tombstoned rows.
    async fn apply_changes(&self, changes: ChangeSet) -> Result<SyncResult, CoreError>;
}
```

- [ ] **Step 2: Register in ports/mod.rs**

In `crates/oneshim-core/src/ports/mod.rs`, add after the `pub mod change_extractor;` line:

```rust
pub mod change_merger;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p oneshim-core`

- [ ] **Step 4: Commit**

```
feat(core): add ChangeMerger port trait for sync write-side
```

---

## Task 6: Create SyncTransport port trait (oneshim-core)

**Files:**
- New: `crates/oneshim-core/src/ports/sync_transport.rs`
- Modify: `crates/oneshim-core/src/ports/mod.rs`

- [ ] **Step 1: Create sync_transport.rs**

Create file `crates/oneshim-core/src/ports/sync_transport.rs`:

```rust
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
}
```

- [ ] **Step 2: Register in ports/mod.rs**

In `crates/oneshim-core/src/ports/mod.rs`, add after the `pub mod storage;` line:

```rust
pub mod sync_transport;
```

(Alphabetical order -- between `storage` and `text_search`.)

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p oneshim-core`

- [ ] **Step 4: Commit**

```
feat(core): add SyncTransport port trait for cross-device data transfer
```

---

## Task 7: Device identity generation (oneshim-storage)

**Files:**
- Modify: `crates/oneshim-storage/src/sqlite/mod.rs`

- [ ] **Step 1: Add `ensure_device_identity` method to SqliteStorage**

In `crates/oneshim-storage/src/sqlite/mod.rs`, add a new public method to the `impl SqliteStorage` block:

```rust
    /// Ensure a device identity row exists in the `device_identity` table.
    ///
    /// On first call (empty table), generates a UUID v4 device_id and inserts
    /// it with the given device_name. On subsequent calls, returns the existing
    /// identity. The table enforces `id = 1` (singleton row).
    ///
    /// Returns `(device_id, device_name)`.
    pub fn ensure_device_identity(
        &self,
        device_name: &str,
    ) -> Result<(String, String), CoreError> {
        let conn = self.conn.lock().map_err(|e| {
            CoreError::Internal(format!("SQLite lock poisoned: {e}"))
        })?;

        // Try to read existing identity first.
        let existing: Option<(String, String)> = conn
            .query_row(
                "SELECT device_id, device_name FROM device_identity WHERE id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        if let Some(identity) = existing {
            return Ok(identity);
        }

        // First launch -- generate a new UUID v4 device_id.
        let device_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO device_identity (id, device_id, device_name) VALUES (1, ?1, ?2)",
            rusqlite::params![device_id, device_name],
        )
        .map_err(|e| CoreError::Internal(format!("Failed to insert device identity: {e}")))?;

        info!(
            device_id = %device_id,
            device_name = %device_name,
            "device identity generated (first launch)"
        );

        Ok((device_id, device_name.to_string()))
    }
```

- [ ] **Step 2: Ensure `uuid` dependency in oneshim-storage/Cargo.toml**

Verify that `uuid` with `v4` feature is in `[dependencies]`. If absent, add:

```toml
uuid = { version = "1", features = ["v4"] }
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p oneshim-storage`

- [ ] **Step 4: Add device identity tests**

Add a new test section at the bottom of `crates/oneshim-storage/src/sqlite/mod.rs` (inside the existing `#[cfg(test)]` block if one exists, or create one):

```rust
#[cfg(test)]
mod device_identity_tests {
    use super::*;

    #[test]
    fn ensure_device_identity_generates_uuid_on_first_call() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();
        let (device_id, device_name) = storage
            .ensure_device_identity("Test Machine")
            .unwrap();

        assert!(!device_id.is_empty());
        // Validate UUID v4 format (8-4-4-4-12 hex chars)
        assert_eq!(device_id.len(), 36);
        assert_eq!(device_name, "Test Machine");
    }

    #[test]
    fn ensure_device_identity_returns_same_id_on_second_call() {
        let storage = SqliteStorage::open_in_memory(30).unwrap();
        let (id1, _) = storage.ensure_device_identity("Machine A").unwrap();
        let (id2, name2) = storage.ensure_device_identity("Machine B").unwrap();

        // Second call must return the FIRST identity, not generate a new one.
        assert_eq!(id1, id2);
        assert_eq!(name2, "Machine A"); // Original name preserved
    }

    #[test]
    fn ensure_device_identity_persists_across_reopens() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let id1 = {
            let storage = SqliteStorage::open(&db_path, 30).unwrap();
            let (id, _) = storage.ensure_device_identity("Laptop").unwrap();
            id
        };

        // Reopen the database
        let id2 = {
            let storage = SqliteStorage::open(&db_path, 30).unwrap();
            let (id, name) = storage.ensure_device_identity("Different Name").unwrap();
            assert_eq!(name, "Laptop"); // Original name preserved
            id
        };

        assert_eq!(id1, id2);
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p oneshim-storage -- device_identity`

- [ ] **Step 6: Commit**

```
feat(storage): add device identity generation for cross-device sync
```

---

## Final Verification

- [ ] **Step 1: Full workspace check**

Run: `cargo check --workspace`

- [ ] **Step 2: Full workspace test**

Run: `cargo test --workspace`

- [ ] **Step 3: Clippy lint**

Run: `cargo clippy --workspace`

- [ ] **Step 4: Format check**

Run: `cargo fmt --check`

---

## Summary of Changes

| Task | Files touched | New tests | Crate |
|------|---------------|-----------|-------|
| 1. SyncConfig section | 3 new/modified | 5 | oneshim-core |
| 2. Consent field | 1 modified | 3 | oneshim-core |
| 3. Sync domain models | 2 new/modified | 6 | oneshim-core |
| 4. ChangeExtractor port | 2 new/modified | 0 (trait only) | oneshim-core |
| 5. ChangeMerger port | 2 new/modified | 0 (trait only) | oneshim-core |
| 6. SyncTransport port | 2 new/modified | 0 (trait only) | oneshim-core |
| 7. Device identity | 1 modified | 3 | oneshim-storage |
| **Total** | **13 files** | **17 tests** | |

**What is NOT in this phase:**
- No SyncEngine orchestrator (Phase 3a-2)
- No ChangeExtractor/ChangeMerger/SyncTransport implementations (Phase 3a-2)
- No FileSyncTransport or RemoteSyncTransport (Phase 3a-2)
- No scheduler loop for periodic sync (Phase 3a-2)
- No encryption or key derivation (Phase 3a-2)
- No web dashboard UI changes (Phase 3a-2)
- No LAN transport or mDNS (Phase 3b)

**Dependencies to add:**
- `hostname = "0.4"` to `oneshim-core/Cargo.toml` (for default device name)
- `uuid = { version = "1", features = ["v4"] }` to `oneshim-storage/Cargo.toml` (if not already present)
