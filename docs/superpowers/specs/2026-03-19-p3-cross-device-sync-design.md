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

## 3. Data Classification

### 3.1 What Syncs

| Table | Sync Mode | Conflict Strategy | Notes |
|-------|-----------|-------------------|-------|
| `activity_segments` | Append-only union | No conflict (PK = UUID) | Core value of cross-device sync |
| `regimes` | LWW per row | HLC on `last_seen_at` update | Regime definitions converge |
| `regime_overrides` | Append-only union | No conflict (PK = UUID) | User corrections sync everywhere |
| `embedding_vectors` | Selective push | LWW per segment_id | Only shared-model embeddings sync (see 3.2) |
| `trigger_params_snapshots` | Append-only union | No conflict (immutable rows) | Parameter history |
| `suggestions` | LWW per row | HLC on status changes | Acted/dismissed state syncs |
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

## 4. Architecture

### 4.1 Sync Layer Overview

```
Device A                                    Device B
┌────────────────────┐                      ┌────────────────────┐
│  SQLite            │                      │  SQLite            │
│  (oneshim-storage) │                      │  (oneshim-storage) │
└────────┬───────────┘                      └───────┬────────────┘
         │                                          │
    ┌────▼────┐                                ┌────▼────┐
    │ SyncPort│ (trait in oneshim-core)         │ SyncPort│
    └────┬────┘                                └────┬────┘
         │                                          │
    ┌────▼──────────┐                          ┌────▼──────────┐
    │ SyncEngine    │                          │ SyncEngine    │
    │ (oneshim-sync)│                          │ (oneshim-sync)│
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

### 4.2 Port Trait

```rust
/// Port trait for cross-device sync (defined in oneshim-core).
#[async_trait]
pub trait SyncPort: Send + Sync {
    /// Get local changes since the given HLC watermark.
    async fn get_changes_since(&self, since: &Hlc) -> Result<ChangeSet, CoreError>;

    /// Apply a remote changeset, resolving conflicts via HLC.
    async fn apply_changes(&self, changes: ChangeSet) -> Result<SyncResult, CoreError>;

    /// Get the current device's high-watermark HLC.
    async fn local_watermark(&self) -> Result<Hlc, CoreError>;
}
```

### 4.3 ChangeSet Format

```rust
/// A batch of changes to sync between devices.
#[derive(Debug, Serialize, Deserialize)]
pub struct ChangeSet {
    pub origin_device_id: String,
    pub origin_device_name: String,
    pub watermark: Hlc,
    pub segments: Vec<SegmentRow>,
    pub regimes: Vec<RegimeRow>,
    pub overrides: Vec<OverrideRow>,
    pub embeddings: Vec<EmbeddingRow>,
    pub param_snapshots: Vec<ParamSnapshotRow>,
    pub preferences: Vec<PrefEntry>,
}

/// Result of applying a changeset.
#[derive(Debug)]
pub struct SyncResult {
    pub applied: usize,
    pub skipped_lww: usize,  // lost LWW race
    pub skipped_dup: usize,  // already present
    pub new_watermark: Hlc,
}
```

### 4.4 Transport Adapters

Three transports, selected by config. All share the same `SyncTransport` trait:

```rust
#[async_trait]
pub trait SyncTransport: Send + Sync {
    async fn push(&self, changes: &ChangeSet) -> Result<(), CoreError>;
    async fn pull(&self, since: &Hlc) -> Result<Option<ChangeSet>, CoreError>;
    async fn discover_peers(&self) -> Result<Vec<PeerInfo>, CoreError>;
}
```

| Transport | Config key | Use case |
|-----------|-----------|----------|
| `RemoteSyncTransport` | `sync.transport = "remote"` | Push/pull via REST/gRPC to any sync endpoint |
| `FileSyncTransport` | `sync.transport = "file"` | Read/write encrypted JSON to a shared folder (Dropbox, iCloud, NAS) |
| `LanSyncTransport` | `sync.transport = "lan"` | mDNS discovery + direct TCP between devices on same network |

**MVP scope**: `RemoteSyncTransport` and `FileSyncTransport` only.

### 4.5 Sync Schedule

- **Periodic**: Every 5 minutes (configurable via `sync.interval_secs`).
- **On-demand**: Triggered by user action ("Sync Now" button in web dashboard).
- **On startup**: Pull on app launch after 10-second warm-up delay.
- **On shutdown**: Push pending changes before exit.

Sync runs in the existing scheduler loop (10th loop alongside the current 9).

## 5. Schema Changes

Migration V14 adds sync metadata columns:

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
  via `sync.include_content_activities = true`.
- Embedding `original_text` is excluded by default; only vectors sync.
  Enable with `sync.include_embedding_text = true`.

### 6.4 Device Identity

- On first launch, a random UUID v4 is generated and persisted in `device_identity`.
- User assigns a human-readable `device_name` (e.g., "Work MacBook").
- Identity is local-only; the remote endpoint sees only the opaque `device_id`.

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

### Phase 3a: Foundation (MVP)

- [ ] `Hlc` implementation in `oneshim-core::sync`
- [ ] `SyncPort` trait in `oneshim-core::ports`
- [ ] `SyncTransport` trait in `oneshim-core::ports`
- [ ] New crate: `oneshim-sync` — SyncEngine (orchestrates port + transport)
- [ ] Schema migration V14 (HLC columns, `sync_peers`, `device_identity`)
- [ ] `SyncPort` implementation in `oneshim-storage` (SQLite queries)
- [ ] `FileSyncTransport` adapter in `oneshim-sync` (encrypted JSON files)
- [ ] `RemoteSyncTransport` adapter in `oneshim-network`
- [ ] Device identity generation + config UI in web dashboard
- [ ] 10th scheduler loop for periodic sync

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
│       │   ├── sync_port.rs    # SyncPort trait
│       │   └── sync_transport.rs # SyncTransport trait
│       └── models/
│           └── sync.rs         # ChangeSet, SyncResult, PeerInfo
│
├── oneshim-sync/              # NEW CRATE
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs             # SyncEngine orchestrator
│       ├── merge.rs           # LWW merge logic
│       ├── file_transport.rs  # FileSyncTransport (encrypted JSON)
│       └── lan_transport.rs   # LanSyncTransport (Phase 3b)
│
├── oneshim-storage/
│   └── src/sqlite/
│       └── sync_impl.rs       # SyncPort impl (SQLite queries)
│
└── oneshim-network/
    └── src/
        └── sync_transport.rs  # RemoteSyncTransport (REST/gRPC)
```

This follows the hexagonal architecture: `oneshim-core` defines the ports,
`oneshim-sync` is a new adapter crate for sync orchestration, and existing
adapter crates (`storage`, `network`) implement their respective port sides.

The `oneshim-sync` crate depends only on `oneshim-core` (for traits and models).
It does NOT depend on `oneshim-storage` or `oneshim-network` — those are wired
together at the binary level in `src-tauri` / `oneshim-app`.

## 10. Open Questions

1. **Device pairing UX**: QR code? Shared passphrase? Manual device-ID exchange?
2. **Tombstones**: How long to keep deletion markers for synced-then-deleted data?
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
