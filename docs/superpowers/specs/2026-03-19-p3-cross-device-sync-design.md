# P3: Cross-Device Sync — Design Spec

> Created: 2026-03-19
> Status: Draft
> Depends on: Layer 1 (Adaptive Tiered Memory), Layer 2 (Vector RAG)

## 1. Goal

Allow a single user running the ONESHIM desktop agent on multiple machines (e.g. work
laptop, home desktop) to see a unified view of their activity data — segments, regimes,
embeddings, overrides, and preferences — without requiring a persistent central server.
The design must be offline-first, privacy-preserving, and work across macOS/Windows/Linux.

## 2. Sync Strategy

### 2.1 Evaluated Approaches

| Strategy | Pros | Cons | Verdict |
|----------|------|------|---------|
| **CRDTs (automerge/yrs)** | Automatic merge, no server needed | Complex for relational data, large overhead for vectors | Partial use |
| **Operational Transform** | Well-proven for text | Overkill for structured records, needs central server | Rejected |
| **Last-Write-Wins + Hybrid Logical Clocks** | Simple, predictable, low overhead | Loses concurrent edits to same field | Primary strategy |
| **cr-sqlite extension** | SQLite-native CRDT columns, zero app-code merge | Nightly Rust toolchain required, SQLite extension loading complexity | Future option |
| **Peer-to-peer (libp2p)** | Serverless | NAT traversal, discovery complexity, always-on requirement | Rejected for MVP |
| **File-based (cloud folder)** | Zero infra | Race conditions, no merge, OS-dependent | Rejected |

### 2.2 Recommended: Hybrid LWW + Append-Only

**Primary mechanism**: Hybrid Logical Clocks (HLC) with Last-Write-Wins per record.

- Each device maintains an HLC (`physical_time + logical_counter + device_id`).
- Every mutable row gets `hlc_timestamp` and `origin_device_id` columns.
- On merge: higher HLC wins. Ties broken by lexicographic device ID.
- Append-only data (segments, calibration log) never conflicts — union merge.

**Why not full CRDT?** The data is mostly append-only time-series (segments,
calibration events) where union merge is trivially correct. The mutable data
(regime overrides, preferences) has low write frequency and single-user semantics,
making LWW sufficient. Full CRDTs add ~30% storage overhead for change history
that provides no benefit here.

**Future upgrade path**: If multi-user collaboration is ever needed, cr-sqlite
can be adopted as a drop-in SQLite extension to get column-level CRDTs without
changing application code.

### 2.3 Rust Crate: HLC Implementation

```toml
# No external dependency needed — HLC is ~80 lines
# Implement in oneshim-core::sync::hlc
```

HLC struct:

```rust
/// Hybrid Logical Clock for causal ordering across devices.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Hlc {
    /// Wall-clock millis (capped to max of local and received).
    pub wall_ms: u64,
    /// Logical counter (incremented on ties).
    pub counter: u32,
    /// Originating device ID (tie-breaker).
    pub device_id: String,
}
```

### 2.4 SyncConfig Section

A new `SyncConfig` section is added to `AppConfig` to control all sync behavior.
This mirrors the existing pattern used by other config sections (e.g., `TelemetryConfig`,
`PrivacyConfig`) and lives in `crates/oneshim-core/src/config/sections/`.

```rust
/// Sync transport selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncTransport {
    /// Push/pull via REST/gRPC to a remote sync endpoint.
    Remote,
    /// Read/write encrypted JSON to a shared folder (Dropbox, iCloud, NAS).
    File,
    /// mDNS discovery + direct TCP between devices on the same LAN (Phase 3b).
    Lan,
}

impl Default for SyncTransport {
    fn default() -> Self {
        Self::File
    }
}

/// Cross-device sync configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Master switch. When false, all sync operations are disabled.
    #[serde(default)]
    pub enabled: bool,

    /// Selected transport mechanism.
    #[serde(default)]
    pub transport: SyncTransport,

    /// Interval between periodic sync cycles (seconds). Default: 300 (5 minutes).
    #[serde(default = "default_sync_interval_secs")]
    pub interval_secs: u64,

    /// Include raw `content_activities_json` in synced segments.
    /// Default: false (only `dominant_category`, `duration_secs`, `app_breakdown`,
    /// `llm_summary` are synced).
    #[serde(default)]
    pub include_content_activities: bool,

    /// Include `original_text` in synced embedding vectors.
    /// Default: false (only vector blobs sync).
    #[serde(default)]
    pub include_embedding_text: bool,

    /// Human-readable name for this device (e.g., "Work MacBook").
    /// Shown to the user on peer devices. Defaults to hostname.
    #[serde(default = "default_device_name")]
    pub device_name: String,
}

fn default_sync_interval_secs() -> u64 { 300 }
fn default_device_name() -> String { hostname::get().unwrap_or_default().to_string_lossy().into() }

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            transport: SyncTransport::default(),
            interval_secs: default_sync_interval_secs(),
            include_content_activities: false,
            include_embedding_text: false,
            device_name: default_device_name(),
        }
    }
}
```

**Placement in `AppConfig`** (in `crates/oneshim-core/src/config/mod.rs`):

```rust
pub struct AppConfig {
    // ... existing sections (server, monitor, storage, vision, ..., analysis) ...
    #[serde(default)]
    pub sync: SyncConfig,
}
```

The `sync` field uses `#[serde(default)]` so existing config files without a
`sync` section deserialize without error (all defaults = sync disabled).

## 3. Data Classification

### 3.1 What Syncs

| Table | Sync Mode | Conflict Strategy | Notes |
|-------|-----------|-------------------|-------|
| `activity_segments` | Append-only union | No conflict (PK = UUID) | Core value of cross-device sync |
| `regimes` | LWW per row | HLC on `last_seen_at` update | Regime definitions converge |
| `regime_overrides` | Append-only union | No conflict (PK = UUID) | User corrections sync everywhere |
| `embedding_vectors` | Selective push | LWW per segment_id | Only shared-model embeddings sync (see 3.2) |
| `trigger_params_snapshots` | Append-only union | No conflict (immutable rows) | Parameter history |
| `suggestions` | LWW per row | Monotonic status merge (see 3.4) | Acted/dismissed state syncs |
| Config/preferences | LWW per key | HLC timestamp | `AppConfig` sections |

### 3.2 What Does NOT Sync

| Data | Reason |
|------|--------|
| `events` (raw) | Too high-volume (~5K/day); already uploaded via BatchUploader if server available |
| `frames` (images) | Large binary data; device-specific screenshots |
| `calibration_log` | Device-specific signal values; not meaningful on other hardware |
| `focus_metrics` | Derived/aggregable; re-computed from local segments |
| `weekly_digests` / `daily_digests` | Regenerated from segments; no need to sync |
| `work_sessions` | Derived from events; can be reconstructed |
| `search_fts` | Virtual table; rebuilt from synced segments |

### 3.3 Embedding Sync Policy

Embeddings are only synced when:
1. Same `model_id` is used on both devices (different models = incompatible vectors).
2. The embedding is not marked `is_stale`.
3. The segment it belongs to has already synced.

Device-local embeddings (from a model not present on the peer) are retained locally
but not transmitted.

### 3.4 Suggestion Status Merge Rule

The `suggestions` table has multiple status timestamp fields (`shown_at`,
`dismissed_at`, `acted_at`) that together form a status state machine. Rather than
tracking field-level HLC for each timestamp column, we exploit the fact that status
transitions are **monotonic** — they only move forward through a fixed ordering:

```
null  -->  shown  -->  dismissed  -->  acted
 (0)        (1)          (2)           (3)
```

**Merge rule**: On conflict, the side with the **higher status ordinal** wins.
If both sides are at the same status level, standard HLC-based LWW applies to
the row as a whole.

**Deterministic resolution**:
- `acted > dismissed > shown > null`
- A suggestion that was "acted" on Device A and "dismissed" on Device B resolves
  to "acted" because `acted (3) > dismissed (2)`.
- Field-level HLC is NOT needed because these transitions are monotonic — a higher
  status always represents a later logical state.

**Implementation**: The `ChangeMerger` computes a status ordinal from the timestamp
fields and compares ordinals before falling back to row-level HLC comparison.

## 4. Architecture

### 4.1 Sync Layer Overview

```
Device A                                    Device B
┌────────────────────┐                      ┌────────────────────┐
│  SQLite            │                      │  SQLite            │
│  (oneshim-storage) │                      │  (oneshim-storage) │
└────────┬───────────┘                      └───────┬────────────┘
         │                                          │
   ┌─────▼──────────┐                         ┌─────▼──────────┐
   │ ChangeExtractor│                         │ ChangeExtractor│
   │ ChangeMerger   │                         │ ChangeMerger   │
   │ (oneshim-storage)                        │ (oneshim-storage)
   └─────┬──────────┘                         └─────┬──────────┘
         │                                          │
    ┌────▼──────────┐                          ┌────▼──────────┐
    │ SyncEngine    │                          │ SyncEngine    │
    │ (orchestrator)│                          │ (orchestrator)│
    └────┬──────────┘                          └────┬──────────┘
         │                                          │
         │     ┌──────────────────────────┐         │
         └────►│  Sync Transport          │◄────────┘
               │  (one of):               │
               │  a) Remote sync endpoint │
               │  b) Shared file/folder   │
               │  c) Direct LAN (mDNS)    │
               └──────────────────────────┘
```

### 4.2 Port Traits

The original monolithic `SyncPort` is split into two focused traits following the
read/write separation principle. This keeps `SyncEngine` as a pure orchestrator
that coordinates extraction, transport, and merging without containing any
storage or merge logic itself.

```rust
/// Read-side port: extracts local changes for outbound sync.
/// Implemented by oneshim-storage (SQLite queries).
#[async_trait]
pub trait ChangeExtractor: Send + Sync {
    /// Get local changes since the given HLC watermark.
    async fn get_changes_since(&self, since: &Hlc) -> Result<ChangeSet, CoreError>;

    /// Get the current device's high-watermark HLC.
    async fn local_watermark(&self) -> Result<Hlc, CoreError>;
}

/// Write-side port: applies inbound changesets with LWW conflict resolution.
/// Implemented by oneshim-storage (SQLite queries with HLC comparison).
#[async_trait]
pub trait ChangeMerger: Send + Sync {
    /// Apply a remote changeset, resolving conflicts via HLC.
    /// Returns statistics on applied/skipped rows.
    async fn apply_changes(&self, changes: ChangeSet) -> Result<SyncResult, CoreError>;
}
```

`SyncEngine` is a pure orchestrator that takes `Arc<dyn ChangeExtractor>`,
`Arc<dyn ChangeMerger>`, and `Arc<dyn SyncTransport>` via constructor injection.
It contains no merge logic, no SQL, and no transport logic — only the sync cycle
coordination (pull, merge, push sequence).

### 4.3 ChangeSet Format

```rust
/// The kind of changeset being transmitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeSetKind {
    /// Normal data synchronization.
    Data,
    /// GDPR Article 17 deletion propagation (see section 6.5).
    /// All peers receiving this changeset MUST perform local erasure.
    DeletionEvent,
}

/// A batch of changes to sync between devices.
#[derive(Debug, Serialize, Deserialize)]
pub struct ChangeSet {
    pub kind: ChangeSetKind,
    pub origin_device_id: String,
    pub origin_device_name: String,
    pub watermark: Hlc,
    pub segments: Vec<SegmentRow>,
    pub regimes: Vec<RegimeRow>,
    pub overrides: Vec<OverrideRow>,
    pub embeddings: Vec<EmbeddingRow>,
    pub suggestions: Vec<SuggestionRow>,
    pub param_snapshots: Vec<ParamSnapshotRow>,
    pub preferences: Vec<PrefEntry>,
}

/// Result of applying a changeset.
#[derive(Debug)]
pub struct SyncResult {
    pub applied: usize,
    pub skipped_lww: usize,  // lost LWW race
    pub skipped_dup: usize,  // already present
    pub tombstoned: usize,   // soft-deleted via tombstone
    pub new_watermark: Hlc,
}
```

**Deletion event changesets** (GDPR Article 17): When `kind == DeletionEvent`, the
changeset body is empty (all Vec fields are empty). The `ChangeMerger` receiving
this changeset MUST trigger a full local data erasure for the originating user,
equivalent to calling `ConsentManager::revoke_consent()` locally. See section 6.5
for the full deletion propagation protocol.

### 4.4 Transport Adapters

Three transports, selected by `SyncConfig::transport`. All share the same
`SyncTransport` trait:

```rust
#[async_trait]
pub trait SyncTransport: Send + Sync {
    async fn push(&self, changes: &ChangeSet) -> Result<(), CoreError>;
    async fn pull(&self, since: &Hlc) -> Result<Option<ChangeSet>, CoreError>;
    async fn discover_peers(&self) -> Result<Vec<PeerInfo>, CoreError>;
}
```

| Transport | Config value | Crate | Use case |
|-----------|-------------|-------|----------|
| `RemoteSyncTransport` | `SyncTransport::Remote` | `oneshim-network` | Push/pull via REST/gRPC to any sync endpoint |
| `FileSyncTransport` | `SyncTransport::File` | `oneshim-storage` | Read/write encrypted JSON to a shared folder (Dropbox, iCloud, NAS) |
| `LanSyncTransport` | `SyncTransport::Lan` | `oneshim-network` | mDNS discovery + direct TCP between devices on same network |

**Note**: `FileSyncTransport` is placed in `oneshim-storage` (not in `oneshim-sync`)
because it is a pure file I/O adapter. The `oneshim-sync` crate (if it exists)
remains a pure orchestrator with no transport implementations. This follows the
hexagonal architecture rule: adapters live in adapter crates, orchestrators have
no I/O of their own.

**MVP scope**: `FileSyncTransport` only (Phase 3a-2). `RemoteSyncTransport`
deferred to Phase 3a-2.

#### 4.4.1 File-Based Sync Atomicity

The file transport uses per-device changeset files with a deterministic naming
convention. No shared file locking is needed because each device writes only its
own changeset files and reads files from other devices.

**File naming convention**:

```
{sync_folder}/
  changeset-{device_id}-{hlc_wall_ms}-{hlc_counter}.enc
```

Example:
```
~/Dropbox/oneshim-sync/
  changeset-a1b2c3d4-1710859200000-42.enc
  changeset-e5f6g7h8-1710859205000-17.enc
```

**Atomic write protocol**:

1. Write the encrypted changeset to a temporary file:
   `changeset-{device_id}-{hlc}.enc.tmp`
2. `fsync` the temporary file to ensure durability.
3. Rename the temporary file to the final name (atomic on all supported OS).
4. On read, ignore any `.tmp` files (incomplete writes from crashes).

This guarantees that readers never see partial changesets. No distributed locking
(e.g., lockfiles, advisory locks) is required because each device owns its own
file namespace via the `device_id` prefix.

**Garbage collection**: Changeset files older than `storage.retention_days`
(default 30) are deleted during the retention enforcement pass.

### 4.5 Sync Schedule

- **Periodic**: Every 5 minutes (configurable via `SyncConfig::interval_secs`).
- **On-demand**: Triggered by user action ("Sync Now" button in web dashboard).
- **On startup**: Pull on app launch after 10-second warm-up delay.
- **On shutdown**: Push pending changes before exit.

Sync runs in the existing scheduler loop (10th loop alongside the current 9).

## 5. Schema Changes

### 5.1 Migration V14: Sync Metadata

Migration V14 adds sync metadata columns to all syncable tables, plus tombstone
support for LWW-managed tables, plus new sync infrastructure tables.

**`origin_device_id` semantics**: The default value `''` (empty string) means
"local-only, never synced". This is the state of all pre-existing rows after the
V14 migration runs. On first sync, the local device's `device_id` (from the
`device_identity` table) is backfilled into all rows where
`origin_device_id = ''`. Rows received from peers always have a non-empty
`origin_device_id` set by the sender.

```sql
-- Add HLC columns to syncable tables
ALTER TABLE activity_segments ADD COLUMN hlc_wall_ms INTEGER NOT NULL DEFAULT 0;
ALTER TABLE activity_segments ADD COLUMN hlc_counter INTEGER NOT NULL DEFAULT 0;
ALTER TABLE activity_segments ADD COLUMN origin_device_id TEXT NOT NULL DEFAULT '';

ALTER TABLE regimes ADD COLUMN hlc_wall_ms INTEGER NOT NULL DEFAULT 0;
ALTER TABLE regimes ADD COLUMN hlc_counter INTEGER NOT NULL DEFAULT 0;
ALTER TABLE regimes ADD COLUMN origin_device_id TEXT NOT NULL DEFAULT '';

ALTER TABLE regime_overrides ADD COLUMN hlc_wall_ms INTEGER NOT NULL DEFAULT 0;
ALTER TABLE regime_overrides ADD COLUMN hlc_counter INTEGER NOT NULL DEFAULT 0;
ALTER TABLE regime_overrides ADD COLUMN origin_device_id TEXT NOT NULL DEFAULT '';

ALTER TABLE embedding_vectors ADD COLUMN hlc_wall_ms INTEGER NOT NULL DEFAULT 0;
ALTER TABLE embedding_vectors ADD COLUMN hlc_counter INTEGER NOT NULL DEFAULT 0;
ALTER TABLE embedding_vectors ADD COLUMN origin_device_id TEXT NOT NULL DEFAULT '';

ALTER TABLE suggestions ADD COLUMN hlc_wall_ms INTEGER NOT NULL DEFAULT 0;
ALTER TABLE suggestions ADD COLUMN hlc_counter INTEGER NOT NULL DEFAULT 0;
ALTER TABLE suggestions ADD COLUMN origin_device_id TEXT NOT NULL DEFAULT '';

-- Tombstone columns for LWW-managed tables (regimes, suggestions, embedding_vectors).
-- Append-only tables (activity_segments, regime_overrides, trigger_params_snapshots)
-- do not need tombstones because rows are never deleted during normal operation.
ALTER TABLE regimes ADD COLUMN is_deleted INTEGER NOT NULL DEFAULT 0;
ALTER TABLE regimes ADD COLUMN deleted_at TEXT;

ALTER TABLE suggestions ADD COLUMN is_deleted INTEGER NOT NULL DEFAULT 0;
ALTER TABLE suggestions ADD COLUMN deleted_at TEXT;

ALTER TABLE embedding_vectors ADD COLUMN is_deleted INTEGER NOT NULL DEFAULT 0;
ALTER TABLE embedding_vectors ADD COLUMN deleted_at TEXT;

-- Sync watermark tracking (per-peer high watermark)
CREATE TABLE IF NOT EXISTS sync_peers (
    device_id TEXT PRIMARY KEY,
    device_name TEXT NOT NULL,
    last_sync_at TEXT NOT NULL,
    watermark_wall_ms INTEGER NOT NULL DEFAULT 0,
    watermark_counter INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Local device identity
CREATE TABLE IF NOT EXISTS device_identity (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    device_id TEXT NOT NULL UNIQUE,
    device_name TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

### 5.2 Tombstone Strategy

**Problem**: When a row is deleted on Device A, Device B must learn about the
deletion rather than re-inserting the row on the next sync cycle. Without
tombstones, deletes cannot propagate.

**Design**: LWW-managed tables (`regimes`, `suggestions`, `embedding_vectors`)
gain two columns:

| Column | Type | Default | Purpose |
|--------|------|---------|---------|
| `is_deleted` | `INTEGER NOT NULL` | `0` | Soft-delete flag. `1` = tombstoned. |
| `deleted_at` | `TEXT` | `NULL` | ISO-8601 timestamp of deletion. |

**Deletion protocol**:
1. Instead of `DELETE FROM regimes WHERE id = ?`, set
   `is_deleted = 1, deleted_at = datetime('now')` and advance the row's HLC.
2. The tombstoned row participates in normal sync: it is included in
   `ChangeExtractor::get_changes_since()` output.
3. The receiving `ChangeMerger` applies the tombstone via LWW. If the remote
   HLC is higher, the local row is marked as deleted.
4. All application queries MUST include `WHERE is_deleted = 0` to hide
   tombstoned rows from the UI and business logic.

**Tombstone retention**: Tombstoned rows are retained for **30 days** after
`deleted_at`. This gives all devices ample time to receive the deletion. After
30 days, tombstones are permanently purged (hard `DELETE`) during the retention
enforcement pass that already runs on `storage.retention_days`.

**Garbage collection path**: The existing retention enforcement logic in
`oneshim-storage` is extended to include:
```sql
DELETE FROM regimes WHERE is_deleted = 1
    AND deleted_at < datetime('now', '-30 days');
DELETE FROM suggestions WHERE is_deleted = 1
    AND deleted_at < datetime('now', '-30 days');
DELETE FROM embedding_vectors WHERE is_deleted = 1
    AND deleted_at < datetime('now', '-30 days');
```

**Append-only tables** (`activity_segments`, `regime_overrides`,
`trigger_params_snapshots`) do not need tombstones because rows are never
individually deleted — they are only bulk-purged by the retention policy, which
runs independently on each device.

## 6. Privacy and Security Model

### 6.1 Encryption at Rest and in Transit

- **File transport**: ChangeSets are encrypted with AES-256-GCM before writing.
  Key derived from a user-chosen passphrase via Argon2id (stored nowhere — user
  must enter on each device).
- **Remote transport**: TLS (HTTPS/gRPC-TLS) for transit. Server stores encrypted
  blobs if configured.
- **LAN transport**: TLS with self-signed certs + TOFU (Trust On First Use) pinning.

### 6.2 What Never Leaves the Device

- Raw screenshots / frame images.
- Raw events (keystrokes, mouse movements, window titles with PII).
- Calibration signals (hardware-specific sensor data).
- Encryption keys or passphrases.

### 6.3 Data Minimization

- Segments sync `dominant_category`, `duration_secs`, `app_breakdown`, and
  `llm_summary` — but NOT raw `content_activities_json` unless user opts in
  via `SyncConfig::include_content_activities = true`.
- Embedding `original_text` is excluded by default; only vectors sync.
  Enable with `SyncConfig::include_embedding_text = true`.

### 6.4 Device Identity

- On first launch, a random UUID v4 is generated and persisted in `device_identity`.
- User assigns a human-readable `device_name` via `SyncConfig::device_name`
  (defaults to OS hostname, e.g., "Work MacBook").
- Identity is local-only; the remote endpoint sees only the opaque `device_id`.

### 6.5 Consent Gate (GDPR Article 6)

All sync operations are gated on explicit user consent via the existing
`ConsentManager` (defined in `crates/oneshim-core/src/consent.rs`).

**New field in `ConsentPermissions`**:

```rust
pub struct ConsentPermissions {
    // --- existing Tier 1-4 fields ---
    pub screen_capture: bool,
    pub ocr_processing: bool,
    pub telemetry: bool,
    pub process_monitoring: bool,
    pub input_activity: bool,
    pub window_title_collection: bool,
    pub app_usage_analytics: bool,
    pub clipboard_monitoring: bool,
    pub file_access_monitoring: bool,
    pub activity_pattern_learning: bool,

    // --- Tier 5: Cross-Device Sync ---
    /// Permits cross-device synchronization of activity data.
    /// GDPR Article 6 — processing requires explicit consent for data
    /// transfer between devices, even when both are owned by the same user.
    #[serde(default)]
    pub cross_device_sync: bool,
}
```

**Enforcement**: Before any sync operation (`SyncEngine::run_cycle()`), the
engine MUST check:

```rust
if !consent_manager.is_permitted(|p| p.cross_device_sync) {
    debug!("sync skipped: cross_device_sync consent not granted");
    return Ok(());
}
```

Both `SyncConfig::enabled` AND `consent_manager.is_permitted(|p| p.cross_device_sync)`
must be true for any data to leave the device. This is a hard requirement — the
config flag alone is not sufficient.

### 6.6 Deletion Propagation (GDPR Article 17)

When a user revokes consent on one device (Device A), all synced copies of their
data on other devices (Device B, C, ...) must be erased. This is implemented via
a special deletion event changeset.

**Protocol**:

1. User calls `ConsentManager::revoke_consent()` on Device A.
2. Device A's `SyncEngine` detects `consent_manager.has_pending_deletion() == true`.
3. Device A pushes a `ChangeSet` with `kind: ChangeSetKind::DeletionEvent` via
   the active transport. The changeset body is empty — only the `kind` and
   `origin_device_id` fields are meaningful.
4. Device B receives the deletion event during its next pull cycle.
5. Device B's `ChangeMerger` triggers local data erasure:
   - All synced rows with `origin_device_id == deletion_event.origin_device_id`
     are hard-deleted (not tombstoned — this is a GDPR erasure, not a soft delete).
   - If the deletion event's `origin_device_id` matches the local device, ALL
     synced data is erased (full wipe).
6. Device B acknowledges the deletion by advancing its peer watermark past the
   deletion event's HLC.

**Timing**: The deletion event changeset is the LAST changeset pushed before
Device A's sync is disabled (consent revocation disables sync). Device A calls
`consent_manager.clear_pending_deletion()` only after the push succeeds.

**Offline peers**: If Device B is offline when Device A revokes consent, the
deletion event persists in the transport (file or remote endpoint) until Device B
comes online and pulls it. The deletion event is never garbage-collected from the
transport — it remains until all known peers have acknowledged it.

## 7. Bandwidth Estimation

| Data type | Per day (est.) | Per sync cycle (5 min) |
|-----------|---------------|----------------------|
| Segments | ~100 segments x 500 bytes = 50 KB | ~1-2 KB |
| Regimes | ~5 updates x 200 bytes = 1 KB | < 100 bytes |
| Overrides | ~2 overrides x 150 bytes = 300 bytes | < 50 bytes |
| Embeddings | ~100 vectors x 3 KB (768-dim f32) = 300 KB | ~5-10 KB |
| Preferences | Rare changes | < 100 bytes |
| **Total** | **~350 KB/day** | **~15 KB/cycle** |

With zstd compression (already in `oneshim-network`): **~100 KB/day**.

## 8. Phase Scope

### Phase 3a-1: Foundational Primitives (MVP)

Establishes the core abstractions and schema without any runtime sync behavior.
Everything in this phase is inert — no data moves between devices until Phase 3a-2.

- [ ] `Hlc` implementation in `oneshim-core::sync`
- [ ] `ChangeExtractor` trait in `oneshim-core::ports`
- [ ] `ChangeMerger` trait in `oneshim-core::ports`
- [ ] `SyncTransport` trait in `oneshim-core::ports`
- [ ] `ChangeSet`, `ChangeSetKind`, `SyncResult`, `PeerInfo` models in `oneshim-core::models::sync`
- [ ] Schema migration V14 (HLC columns, tombstone columns, `sync_peers`, `device_identity`)
- [ ] `SyncConfig` + `SyncTransport` enum in `oneshim-core::config::sections`
- [ ] `cross_device_sync: bool` in `ConsentPermissions`
- [ ] Device identity generation (UUID v4 on first launch, persist to `device_identity` table)

### Phase 3a-2: Sync Runtime (Deferred)

Wires the abstractions into a working sync loop.

- [ ] `SyncEngine` orchestrator (pure coordination: pull, merge, push)
- [ ] `ChangeExtractor` impl in `oneshim-storage` (SQLite queries)
- [ ] `ChangeMerger` impl in `oneshim-storage` (LWW resolution + tombstone handling)
- [ ] `FileSyncTransport` adapter in `oneshim-storage` (encrypted JSON, atomic write)
- [ ] `RemoteSyncTransport` adapter in `oneshim-network` (REST/gRPC)
- [ ] 10th scheduler loop for periodic sync
- [ ] Consent gate enforcement in `SyncEngine`
- [ ] Deletion propagation (GDPR Article 17 changeset)
- [ ] Device name config UI in web dashboard

### Phase 3b: Polish

- [ ] `LanSyncTransport` (mDNS + TCP)
- [ ] Merge conflict viewer in web dashboard (show LWW decisions)
- [ ] Selective sync (choose which tables to sync per device)
- [ ] Bandwidth throttling config
- [ ] Sync health metrics (last sync time, peer status, error count)

### Phase 3c: Advanced (Future)

- [ ] cr-sqlite integration for column-level CRDT merge
- [ ] Partial segment sync (date range filters)
- [ ] Multi-user sync (requires auth + access control)

## 9. Crate Placement

```
crates/
├── oneshim-core/
│   └── src/
│       ├── sync/
│       │   ├── mod.rs          # re-exports
│       │   └── hlc.rs          # Hybrid Logical Clock
│       ├── ports/
│       │   ├── change_extractor.rs  # ChangeExtractor trait (read-side)
│       │   ├── change_merger.rs     # ChangeMerger trait (write-side)
│       │   └── sync_transport.rs    # SyncTransport trait
│       ├── models/
│       │   └── sync.rs         # ChangeSet, ChangeSetKind, SyncResult, PeerInfo
│       └── config/
│           └── sections/       # SyncConfig added here
│
├── oneshim-storage/
│   └── src/
│       ├── sync_extractor.rs   # ChangeExtractor impl (SQLite queries)
│       ├── sync_merger.rs      # ChangeMerger impl (LWW + tombstones)
│       └── file_transport.rs   # FileSyncTransport (encrypted JSON files)
│
└── oneshim-network/
    └── src/
        └── sync_transport.rs   # RemoteSyncTransport (REST/gRPC)
```

This follows the hexagonal architecture:

- **`oneshim-core`** defines all port traits (`ChangeExtractor`, `ChangeMerger`,
  `SyncTransport`), models, and config. No implementations.
- **`oneshim-storage`** implements the read-side and write-side ports (SQLite
  queries) and the file-based transport (pure file I/O).
- **`oneshim-network`** implements the remote transport (REST/gRPC).
- **No `oneshim-sync` crate**: The `SyncEngine` orchestrator lives directly in
  `src-tauri` / `oneshim-app` as a wiring-level component, similar to how
  `Scheduler` already lives there. It depends on the port traits from
  `oneshim-core` and is wired to concrete implementations at startup. This
  avoids creating a crate that would either be a pure pass-through or would
  need to pull in adapter dependencies.

**Dependency graph**:
```
oneshim-core  <--  oneshim-storage  (ChangeExtractor, ChangeMerger, FileSyncTransport)
              <--  oneshim-network  (RemoteSyncTransport)
              <--  src-tauri        (SyncEngine orchestrator, DI wiring)
```

No direct dependency between `oneshim-storage` and `oneshim-network`.
All cross-crate communication goes through `oneshim-core` traits.

## 10. Open Questions

1. **Device pairing UX**: QR code? Shared passphrase? Manual device-ID exchange?
2. ~~**Tombstones**: How long to keep deletion markers for synced-then-deleted data?~~
   **Resolved**: 30-day retention, garbage collected during retention enforcement (section 5.2).
3. **Clock skew**: HLC handles moderate skew, but should we warn on >1 hour drift?
4. **Embedding model migration**: When a device upgrades its embedding model, should
   it re-embed and push, or let peers keep their own model's vectors?

## References

- [Automerge](https://automerge.org/) — CRDT library (evaluated, deferred)
- [Yrs (Yjs Rust port)](https://github.com/y-crdt/y-crdt) — CRDT library (evaluated, deferred)
- [cr-sqlite](https://github.com/vlcn-io/cr-sqlite) — SQLite CRDT extension (future option)
- [Hybrid Logical Clocks](https://cse.buffalo.edu/tech-reports/2014-04.pdf) — Kulkarni et al., 2014
- [SyncKit](https://news.ycombinator.com/item?id=46069598) — Offline-first sync engine reference
- [Mozilla Application Services](https://mozilla.github.io/application-services/book/howtos/building-a-rust-component.html) — Rust sync component patterns
