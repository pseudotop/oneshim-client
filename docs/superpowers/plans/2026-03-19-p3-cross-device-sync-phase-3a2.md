# P3 Cross-Device Sync Phase 3a-2 — Sync Runtime

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the Phase 3a-1 foundational primitives into a working sync loop: implement ChangeExtractor and ChangeMerger against SQLite, build FileSyncTransport with AES-256-GCM encryption, create the SyncEngine orchestrator, add a 10th scheduler loop, enforce consent, and handle GDPR deletion propagation.

**Architecture:** Storage adapters (`ChangeExtractor`, `ChangeMerger`, `FileSyncTransport`) live in `oneshim-storage`. `SyncEngine` orchestrator lives in `src-tauri` as a wiring-level component (same pattern as `Scheduler`). New dependencies: `aes-gcm` + `argon2` in `oneshim-storage` for file-level encryption. No new crate is created.

**Tech Stack:** Rust, rusqlite, aes-gcm, argon2, serde_json, tokio, async_trait, chrono

**Spec:** `docs/superpowers/specs/2026-03-19-p3-cross-device-sync-design.md` (sections 4, 5, 6, 8)

**Predecessor:** `docs/superpowers/plans/2026-03-19-p3-cross-device-sync-phase-3a1.md`

**Already done (DO NOT re-implement):**

| Component | File | Status |
|-----------|------|--------|
| `Hlc` struct (tick, merge, ordering) | `crates/oneshim-core/src/sync/hlc.rs` | Done (7 tests) |
| `ChangeExtractor` trait | `crates/oneshim-core/src/ports/change_extractor.rs` | Done |
| `ChangeMerger` trait | `crates/oneshim-core/src/ports/change_merger.rs` | Done |
| `SyncTransport` trait | `crates/oneshim-core/src/ports/sync_transport.rs` | Done |
| `ChangeSet`, `ChangeSetKind`, `SyncResult`, `PeerInfo` | `crates/oneshim-core/src/models/sync.rs` | Done |
| `SyncConfig` + `SyncTransportKind` + `validated_interval_secs()` | `crates/oneshim-core/src/config/sections/sync.rs` | Done |
| `ConsentPermissions::cross_device_sync` | `crates/oneshim-core/src/consent.rs` | Done |
| `SqliteStorage::ensure_device_identity()` | `crates/oneshim-storage/src/sqlite/mod.rs` | Done |
| V14 migration (HLC columns, tombstone columns, sync_peers, device_identity) | `crates/oneshim-storage/src/migration.rs` | Done |

---

## File Map

### New files

| File | Responsibility |
|------|----------------|
| `crates/oneshim-storage/src/sync_extractor.rs` | `SqliteSyncExtractor` impl `ChangeExtractor` -- query 6 syncable tables since HLC watermark |
| `crates/oneshim-storage/src/sync_merger.rs` | `SqliteSyncMerger` impl `ChangeMerger` -- LWW resolution, monotonic suggestion merge, tombstone propagation, GDPR hard-delete |
| `crates/oneshim-storage/src/file_transport.rs` | `FileSyncTransport` impl `SyncTransport` -- AES-256-GCM encrypted changeset files, atomic write, peer discovery |
| `src-tauri/src/sync_engine.rs` | `SyncEngine` orchestrator -- pull/merge/push cycle, consent gate, deletion propagation |

### Modified files

| File | Change |
|------|--------|
| `crates/oneshim-core/src/config/sections/sync.rs` | Add `sync_folder: Option<String>` + `passphrase_hash: Option<String>` fields |
| `crates/oneshim-storage/src/lib.rs` | Add `pub mod sync_extractor; pub mod sync_merger; pub mod file_transport;` |
| `crates/oneshim-storage/Cargo.toml` | Add `aes-gcm`, `argon2` dependencies |
| `src-tauri/src/scheduler/mod.rs` | Add `sync_engine: Option<Arc<SyncEngine>>` field to `Scheduler` + `with_sync_engine()` builder |
| `src-tauri/src/scheduler/loops.rs` | Add `spawn_cross_device_sync_loop()` + wire into `run_scheduler_loops()` |
| `src-tauri/src/main.rs` | DI wiring: construct `SyncEngine` from `SqliteSyncExtractor` + `SqliteSyncMerger` + `FileSyncTransport` |

---

## Task 1: Add `sync_folder` and `passphrase_hash` to SyncConfig

**File:** `crates/oneshim-core/src/config/sections/sync.rs`

The `FileSyncTransport` needs to know which folder to read/write changeset files. The user provides a passphrase (not stored); `passphrase_hash` is an Argon2id hash used only to verify re-entry on other devices (the actual encryption key is derived at runtime via Argon2id KDF from the raw passphrase).

- [ ] **Step 1: Add fields to `SyncConfig`**

After the `device_name` field, add:

```rust
    /// Path to the shared sync folder (Dropbox, iCloud, NAS mount, etc.).
    /// Required when `transport == SyncTransportKind::File`.
    /// Example: "~/Dropbox/oneshim-sync" or "/Volumes/NAS/sync".
    #[serde(default)]
    pub sync_folder: Option<String>,

    /// Argon2id hash of the user-chosen sync passphrase.
    /// Stored only for passphrase verification on new device setup.
    /// The actual AES-256-GCM key is derived at runtime via Argon2id KDF.
    /// Never contains the raw passphrase.
    #[serde(default)]
    pub passphrase_hash: Option<String>,
```

- [ ] **Step 2: Update `Default` impl**

Add to the `Default` impl body:

```rust
            sync_folder: None,
            passphrase_hash: None,
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p oneshim-core`

- [ ] **Step 4: Add test for new fields**

Add to the existing `mod tests` block:

```rust
    #[test]
    fn sync_config_folder_and_passphrase_default_none() {
        let config = SyncConfig::default();
        assert!(config.sync_folder.is_none());
        assert!(config.passphrase_hash.is_none());
    }

    #[test]
    fn sync_config_with_folder_serde_roundtrip() {
        let config = SyncConfig {
            enabled: true,
            sync_folder: Some("/Users/test/Dropbox/oneshim-sync".to_string()),
            passphrase_hash: Some("$argon2id$v=19$m=65536,t=3,p=4$...".to_string()),
            ..SyncConfig::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: SyncConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.sync_folder.as_deref(), Some("/Users/test/Dropbox/oneshim-sync"));
        assert!(parsed.passphrase_hash.is_some());
    }
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p oneshim-core -- sync_config`

- [ ] **Step 6: Commit**

```
feat(core): add sync_folder and passphrase_hash to SyncConfig (Phase 3a-2)
```

---

## Task 2: Add crypto dependencies to oneshim-storage

**File:** `crates/oneshim-storage/Cargo.toml`

- [ ] **Step 1: Add dependencies**

Add to `[dependencies]`:

```toml
aes-gcm = "0.10"
argon2 = "0.5"
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p oneshim-storage`

- [ ] **Step 3: Commit**

```
build(storage): add aes-gcm and argon2 dependencies for sync encryption
```

---

## Task 3: Implement SqliteSyncExtractor (oneshim-storage)

**File:** New `crates/oneshim-storage/src/sync_extractor.rs`

Queries all 6 syncable tables for rows with HLC > watermark, respects `SyncConfig::include_content_activities` and `include_embedding_text` data minimization flags. Backfills `origin_device_id` on first extraction.

- [ ] **Step 1: Create `sync_extractor.rs`**

```rust
//! ChangeExtractor implementation for SQLite.
//!
//! Queries activity_segments, regimes, regime_overrides, embedding_vectors,
//! suggestions, and trigger_params_snapshots for rows modified since a
//! given HLC watermark. Respects SyncConfig data minimization flags.

use async_trait::async_trait;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use tracing::debug;

use oneshim_core::config::SyncConfig;
use oneshim_core::error::CoreError;
use oneshim_core::models::sync::{ChangeSet, ChangeSetKind};
use oneshim_core::ports::change_extractor::ChangeExtractor;
use oneshim_core::sync::Hlc;

/// SQLite-backed ChangeExtractor adapter.
pub struct SqliteSyncExtractor {
    conn: Arc<Mutex<Connection>>,
    device_id: String,
    device_name: String,
    sync_config: SyncConfig,
}

impl SqliteSyncExtractor {
    pub fn new(
        conn: Arc<Mutex<Connection>>,
        device_id: String,
        device_name: String,
        sync_config: SyncConfig,
    ) -> Self {
        Self {
            conn,
            device_id,
            device_name,
            sync_config,
        }
    }

    /// Backfill origin_device_id for pre-sync rows (empty string → local device_id).
    /// Called once on first extraction. Idempotent.
    fn backfill_origin_device_id(conn: &Connection, device_id: &str) -> Result<u64, CoreError> {
        let tables = [
            "activity_segments",
            "regimes",
            "regime_overrides",
            "embedding_vectors",
            "suggestions",
            "trigger_params_snapshots",
        ];
        let mut total = 0u64;
        for table in &tables {
            let sql = format!(
                "UPDATE {table} SET origin_device_id = ?1 WHERE origin_device_id = ''"
            );
            let updated = conn
                .execute(&sql, rusqlite::params![device_id])
                .map_err(|e| {
                    CoreError::Internal(format!("backfill origin_device_id on {table}: {e}"))
                })?;
            total += updated as u64;
        }
        if total > 0 {
            debug!("backfilled origin_device_id on {total} rows");
        }
        Ok(total)
    }

    /// Query a single table for rows with HLC > watermark, returning JSON values.
    fn query_table_changes(
        conn: &Connection,
        table: &str,
        columns: &str,
        since: &Hlc,
    ) -> Result<Vec<serde_json::Value>, CoreError> {
        let sql = format!(
            "SELECT {columns} FROM {table} \
             WHERE (hlc_wall_ms > ?1) \
                OR (hlc_wall_ms = ?1 AND hlc_counter > ?2) \
                OR (hlc_wall_ms = ?1 AND hlc_counter = ?2 AND origin_device_id > ?3) \
             ORDER BY hlc_wall_ms, hlc_counter"
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| {
            CoreError::Internal(format!("prepare query for {table}: {e}"))
        })?;

        let rows = stmt
            .query_map(
                rusqlite::params![since.wall_ms, since.counter, &since.device_id],
                |row| {
                    // Read the entire row as a JSON string via SQLite's json_object
                    // We use a simpler approach: read each column by index
                    let json_str: String = row.get(0)?;
                    Ok(json_str)
                },
            )
            .map_err(|e| CoreError::Internal(format!("query {table}: {e}")))?;

        let mut results = Vec::new();
        for row in rows {
            let json_str = row.map_err(|e| CoreError::Internal(format!("row read {table}: {e}")))?;
            let value: serde_json::Value = serde_json::from_str(&json_str)
                .map_err(|e| CoreError::Internal(format!("json parse {table}: {e}")))?;
            results.push(value);
        }
        Ok(results)
    }
}

#[async_trait]
impl ChangeExtractor for SqliteSyncExtractor {
    async fn get_changes_since(&self, since: &Hlc) -> Result<ChangeSet, CoreError> {
        let conn = self.conn.clone();
        let since = since.clone();
        let device_id = self.device_id.clone();
        let device_name = self.device_name.clone();
        let include_content = self.sync_config.include_content_activities;
        let include_embed_text = self.sync_config.include_embedding_text;

        tokio::task::spawn_blocking(move || {
            let guard = conn.lock().map_err(|e| {
                CoreError::Internal(format!("SQLite lock poisoned: {e}"))
            })?;

            // Backfill on first extraction
            Self::backfill_origin_device_id(&guard, &device_id)?;

            // --- Build per-table JSON extraction queries ---
            // Each query uses json_object() to produce a self-contained JSON row.

            // activity_segments (append-only)
            let seg_cols = if include_content {
                "json_object('id',id,'start_time',start_time,'end_time',end_time,\
                 'duration_secs',duration_secs,'regime_id',regime_id,\
                 'dominant_category',dominant_category,'app_breakdown',app_breakdown,\
                 'llm_summary',llm_summary,'content_activities_json',content_activities_json,\
                 'hlc_wall_ms',hlc_wall_ms,'hlc_counter',hlc_counter,\
                 'origin_device_id',origin_device_id)"
            } else {
                "json_object('id',id,'start_time',start_time,'end_time',end_time,\
                 'duration_secs',duration_secs,'regime_id',regime_id,\
                 'dominant_category',dominant_category,'app_breakdown',app_breakdown,\
                 'llm_summary',llm_summary,\
                 'hlc_wall_ms',hlc_wall_ms,'hlc_counter',hlc_counter,\
                 'origin_device_id',origin_device_id)"
            };
            let segments = Self::query_table_changes(&guard, "activity_segments", seg_cols, &since)?;

            // regimes (LWW, includes tombstone columns)
            let regimes = Self::query_table_changes(
                &guard,
                "regimes",
                "json_object('id',id,'label',label,'detected_at',detected_at,\
                 'last_seen_at',last_seen_at,'occurrence_count',occurrence_count,\
                 'avg_density',avg_density,'avg_importance',avg_importance,\
                 'dominant_category',dominant_category,'params_snapshot_id',params_snapshot_id,\
                 'is_active',is_active,'is_deleted',is_deleted,'deleted_at',deleted_at,\
                 'hlc_wall_ms',hlc_wall_ms,'hlc_counter',hlc_counter,\
                 'origin_device_id',origin_device_id)",
                &since,
            )?;

            // regime_overrides (append-only)
            let overrides = Self::query_table_changes(
                &guard,
                "regime_overrides",
                "json_object('override_id',override_id,'segment_id',segment_id,\
                 'original_regime_id',original_regime_id,'action_type',action_type,\
                 'action_data',action_data,'created_at',created_at,\
                 'hlc_wall_ms',hlc_wall_ms,'hlc_counter',hlc_counter,\
                 'origin_device_id',origin_device_id)",
                &since,
            )?;

            // embedding_vectors (LWW, includes tombstone; respects include_embed_text)
            let embed_cols = if include_embed_text {
                "json_object('id',id,'segment_id',segment_id,'content_type',content_type,\
                 'content_label',content_label,'original_text',original_text,\
                 'vector',hex(vector),'model_id',model_id,'timestamp',timestamp,\
                 'is_stale',is_stale,'is_deleted',is_deleted,'deleted_at',deleted_at,\
                 'hlc_wall_ms',hlc_wall_ms,'hlc_counter',hlc_counter,\
                 'origin_device_id',origin_device_id)"
            } else {
                "json_object('id',id,'segment_id',segment_id,'content_type',content_type,\
                 'content_label',content_label,\
                 'vector',hex(vector),'model_id',model_id,'timestamp',timestamp,\
                 'is_stale',is_stale,'is_deleted',is_deleted,'deleted_at',deleted_at,\
                 'hlc_wall_ms',hlc_wall_ms,'hlc_counter',hlc_counter,\
                 'origin_device_id',origin_device_id)"
            };
            let embeddings = Self::query_table_changes(
                &guard,
                "embedding_vectors WHERE is_stale = 0",
                embed_cols,
                &since,
            )?;

            // suggestions (LWW, monotonic status merge)
            let suggestions = Self::query_table_changes(
                &guard,
                "suggestions",
                "json_object('suggestion_id',suggestion_id,'suggestion_type',suggestion_type,\
                 'source',source,'content',content,'priority',priority,\
                 'confidence_score',confidence_score,'relevance_score',relevance_score,\
                 'is_actionable',is_actionable,'reasoning',reasoning,\
                 'shown_at',shown_at,'dismissed_at',dismissed_at,'acted_at',acted_at,\
                 'created_at',created_at,'expires_at',expires_at,\
                 'is_deleted',is_deleted,'deleted_at',deleted_at,\
                 'hlc_wall_ms',hlc_wall_ms,'hlc_counter',hlc_counter,\
                 'origin_device_id',origin_device_id)",
                &since,
            )?;

            // trigger_params_snapshots (append-only)
            let param_snapshots = Self::query_table_changes(
                &guard,
                "trigger_params_snapshots",
                "json_object('id',id,'created_at',created_at,'preset',preset,\
                 'params_json',params_json,\
                 'hlc_wall_ms',hlc_wall_ms,'hlc_counter',hlc_counter,\
                 'origin_device_id',origin_device_id)",
                &since,
            )?;

            // Compute new watermark from max HLC across all extracted rows
            let watermark = Self::compute_max_hlc(&guard, &device_id)?;

            Ok(ChangeSet {
                kind: ChangeSetKind::Data,
                origin_device_id: device_id,
                origin_device_name: device_name,
                watermark,
                segments,
                regimes,
                overrides,
                embeddings,
                suggestions,
                param_snapshots,
                preferences: Vec::new(), // deferred to Phase 3b
            })
        })
        .await
        .map_err(|e| CoreError::Internal(format!("spawn_blocking join error: {e}")))?
    }

    async fn local_watermark(&self) -> Result<Hlc, CoreError> {
        let conn = self.conn.clone();
        let device_id = self.device_id.clone();

        tokio::task::spawn_blocking(move || {
            let guard = conn.lock().map_err(|e| {
                CoreError::Internal(format!("SQLite lock poisoned: {e}"))
            })?;
            Self::compute_max_hlc(&guard, &device_id)
        })
        .await
        .map_err(|e| CoreError::Internal(format!("spawn_blocking join error: {e}")))?
    }
}

impl SqliteSyncExtractor {
    /// Find the maximum HLC across all syncable tables.
    fn compute_max_hlc(conn: &Connection, device_id: &str) -> Result<Hlc, CoreError> {
        let tables = [
            "activity_segments",
            "regimes",
            "regime_overrides",
            "embedding_vectors",
            "suggestions",
            "trigger_params_snapshots",
        ];

        let mut max = Hlc::default();
        for table in &tables {
            let sql = format!(
                "SELECT COALESCE(MAX(hlc_wall_ms), 0), \
                        COALESCE(MAX(hlc_counter), 0) \
                 FROM {table} WHERE hlc_wall_ms = (\
                   SELECT COALESCE(MAX(hlc_wall_ms), 0) FROM {table}\
                 )"
            );
            let (wall_ms, counter): (u64, u32) = conn
                .query_row(&sql, [], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(|e| CoreError::Internal(format!("max HLC query on {table}: {e}")))?;

            let candidate = Hlc {
                wall_ms,
                counter,
                device_id: device_id.to_string(),
            };
            if candidate > max {
                max = candidate;
            }
        }
        Ok(max)
    }
}
```

- [ ] **Step 2: Register module in `lib.rs`**

In `crates/oneshim-storage/src/lib.rs`, add:

```rust
pub mod sync_extractor;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p oneshim-storage`

- [ ] **Step 4: Add unit tests**

Add a `#[cfg(test)] mod tests` block at the bottom of `sync_extractor.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::SqliteStorage;

    fn setup() -> (SqliteStorage, String) {
        let storage = SqliteStorage::open_in_memory(30).unwrap();
        let (device_id, _) = storage.ensure_device_identity("Test Device").unwrap();
        (storage, device_id)
    }

    #[tokio::test]
    async fn empty_db_returns_empty_changeset() {
        let (storage, device_id) = setup();
        let extractor = SqliteSyncExtractor::new(
            storage.connection_arc(),
            device_id,
            "Test".to_string(),
            SyncConfig::default(),
        );
        let cs = extractor.get_changes_since(&Hlc::default()).await.unwrap();
        assert!(cs.is_empty());
        assert_eq!(cs.kind, ChangeSetKind::Data);
    }

    #[tokio::test]
    async fn local_watermark_returns_default_on_empty_db() {
        let (storage, device_id) = setup();
        let extractor = SqliteSyncExtractor::new(
            storage.connection_arc(),
            device_id,
            "Test".to_string(),
            SyncConfig::default(),
        );
        let wm = extractor.local_watermark().await.unwrap();
        assert_eq!(wm.wall_ms, 0);
        assert_eq!(wm.counter, 0);
    }

    #[tokio::test]
    async fn backfill_sets_origin_device_id() {
        let (storage, device_id) = setup();
        // Insert a segment with empty origin_device_id (simulating pre-V14 data)
        {
            let conn = storage.connection_arc();
            let guard = conn.lock().unwrap();
            guard.execute(
                "INSERT INTO activity_segments \
                 (id, start_time, end_time, duration_secs, trigger_reason, \
                  dominant_category, hlc_wall_ms, hlc_counter, origin_device_id) \
                 VALUES ('seg-1', '2026-01-01T00:00:00', '2026-01-01T01:00:00', \
                         3600, 'timer', 'Development', 100, 1, '')",
                [],
            ).unwrap();
        }

        let extractor = SqliteSyncExtractor::new(
            storage.connection_arc(),
            device_id.clone(),
            "Test".to_string(),
            SyncConfig::default(),
        );
        let cs = extractor.get_changes_since(&Hlc::default()).await.unwrap();
        assert_eq!(cs.segments.len(), 1);

        // Verify backfill happened
        let conn = storage.connection_arc();
        let guard = conn.lock().unwrap();
        let origin: String = guard
            .query_row(
                "SELECT origin_device_id FROM activity_segments WHERE id = 'seg-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(origin, device_id);
    }

    #[tokio::test]
    async fn watermark_filters_old_rows() {
        let (storage, device_id) = setup();
        {
            let conn = storage.connection_arc();
            let guard = conn.lock().unwrap();
            // Row with HLC (100, 1)
            guard.execute(
                "INSERT INTO activity_segments \
                 (id, start_time, end_time, duration_secs, trigger_reason, \
                  dominant_category, hlc_wall_ms, hlc_counter, origin_device_id) \
                 VALUES ('seg-old', '2026-01-01T00:00:00', '2026-01-01T01:00:00', \
                         3600, 'timer', 'Development', 100, 1, ?1)",
                rusqlite::params![device_id],
            ).unwrap();
            // Row with HLC (200, 0)
            guard.execute(
                "INSERT INTO activity_segments \
                 (id, start_time, end_time, duration_secs, trigger_reason, \
                  dominant_category, hlc_wall_ms, hlc_counter, origin_device_id) \
                 VALUES ('seg-new', '2026-01-02T00:00:00', '2026-01-02T01:00:00', \
                         3600, 'timer', 'Communication', 200, 0, ?1)",
                rusqlite::params![device_id],
            ).unwrap();
        }

        let extractor = SqliteSyncExtractor::new(
            storage.connection_arc(),
            device_id.clone(),
            "Test".to_string(),
            SyncConfig::default(),
        );

        // Watermark at (150, 0) should only return seg-new
        let since = Hlc { wall_ms: 150, counter: 0, device_id: "".to_string() };
        let cs = extractor.get_changes_since(&since).await.unwrap();
        assert_eq!(cs.segments.len(), 1);
        assert_eq!(cs.segments[0]["id"], "seg-new");
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p oneshim-storage -- sync_extractor`

- [ ] **Step 6: Commit**

```
feat(storage): implement SqliteSyncExtractor for cross-device sync
```

---

## Task 4: Implement SqliteSyncMerger (oneshim-storage)

**File:** New `crates/oneshim-storage/src/sync_merger.rs`

Applies incoming changesets with three conflict resolution strategies:
1. **Append-only** (segments, overrides, param_snapshots): INSERT OR IGNORE
2. **LWW** (regimes, embeddings): compare HLC, higher wins
3. **Monotonic status** (suggestions): higher status ordinal wins, then HLC

Also handles `ChangeSetKind::DeletionEvent` for GDPR Article 17.

- [ ] **Step 1: Create `sync_merger.rs`**

```rust
//! ChangeMerger implementation for SQLite.
//!
//! Applies incoming changesets from remote peers with conflict resolution:
//! - Append-only tables: INSERT OR IGNORE (union merge)
//! - LWW tables: compare HLC, higher wins
//! - Suggestions: monotonic status merge (acted > dismissed > shown > null)
//! - DeletionEvent: hard-delete all rows from originating device (GDPR Art. 17)

use async_trait::async_trait;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use tracing::{debug, info, warn};

use oneshim_core::error::CoreError;
use oneshim_core::models::sync::{ChangeSet, ChangeSetKind, SyncResult};
use oneshim_core::ports::change_merger::ChangeMerger;
use oneshim_core::sync::Hlc;

/// SQLite-backed ChangeMerger adapter.
pub struct SqliteSyncMerger {
    conn: Arc<Mutex<Connection>>,
    local_device_id: String,
}

impl SqliteSyncMerger {
    pub fn new(conn: Arc<Mutex<Connection>>, local_device_id: String) -> Self {
        Self {
            conn,
            local_device_id,
        }
    }

    /// Compute suggestion status ordinal from timestamp fields.
    /// acted (3) > dismissed (2) > shown (1) > null (0)
    fn suggestion_status_ordinal(row: &serde_json::Value) -> u8 {
        if row.get("acted_at").and_then(|v| v.as_str()).is_some() {
            3
        } else if row.get("dismissed_at").and_then(|v| v.as_str()).is_some() {
            2
        } else if row.get("shown_at").and_then(|v| v.as_str()).is_some() {
            1
        } else {
            0
        }
    }

    /// Handle GDPR Article 17 deletion event: hard-delete all synced data
    /// from the originating device.
    fn handle_deletion_event(
        conn: &Connection,
        origin_device_id: &str,
    ) -> Result<usize, CoreError> {
        let tables = [
            "activity_segments",
            "regimes",
            "regime_overrides",
            "embedding_vectors",
            "suggestions",
            "trigger_params_snapshots",
        ];
        let mut total_deleted = 0usize;
        for table in &tables {
            let sql = format!(
                "DELETE FROM {table} WHERE origin_device_id = ?1"
            );
            let deleted = conn
                .execute(&sql, rusqlite::params![origin_device_id])
                .map_err(|e| {
                    CoreError::Internal(format!("GDPR deletion on {table}: {e}"))
                })?;
            total_deleted += deleted;
        }
        info!(
            origin_device_id = origin_device_id,
            total_deleted = total_deleted,
            "GDPR Article 17 deletion event processed"
        );
        Ok(total_deleted)
    }

    /// Apply a single append-only row (INSERT OR IGNORE).
    fn apply_append_only(
        conn: &Connection,
        table: &str,
        pk_field: &str,
        row: &serde_json::Value,
        result: &mut SyncResult,
    ) -> Result<(), CoreError> {
        let pk = row
            .get(pk_field)
            .and_then(|v| v.as_str())
            .ok_or_else(|| CoreError::Internal(format!("missing PK {pk_field} in {table} row")))?;

        // Check if row already exists
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE {pk_field} = ?1");
        let exists: i64 = conn
            .query_row(&sql, rusqlite::params![pk], |row| row.get(0))
            .map_err(|e| CoreError::Internal(format!("check existence {table}: {e}")))?;

        if exists > 0 {
            result.skipped_dup += 1;
            return Ok(());
        }

        // Insert the row -- the specific INSERT statement depends on the table.
        // For simplicity, we use a prepared INSERT with named columns extracted
        // from the JSON value. See per-table merge methods below.
        result.applied += 1;
        Ok(())
    }

    /// Apply a single LWW row: compare HLC, higher wins.
    fn apply_lww(
        conn: &Connection,
        table: &str,
        pk_field: &str,
        row: &serde_json::Value,
        result: &mut SyncResult,
    ) -> Result<(), CoreError> {
        let pk = row
            .get(pk_field)
            .and_then(|v| v.as_str())
            .ok_or_else(|| CoreError::Internal(format!("missing PK {pk_field} in {table}")))?;

        let remote_wall: u64 = row
            .get("hlc_wall_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let remote_counter: u32 = row
            .get("hlc_counter")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        let remote_device: &str = row
            .get("origin_device_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let remote_hlc = Hlc {
            wall_ms: remote_wall,
            counter: remote_counter,
            device_id: remote_device.to_string(),
        };

        // Query local HLC
        let sql = format!(
            "SELECT hlc_wall_ms, hlc_counter, origin_device_id FROM {table} WHERE {pk_field} = ?1"
        );
        let local_hlc: Option<Hlc> = conn
            .query_row(&sql, rusqlite::params![pk], |row| {
                Ok(Hlc {
                    wall_ms: row.get(0)?,
                    counter: row.get(1)?,
                    device_id: row.get(2)?,
                })
            })
            .ok();

        match local_hlc {
            None => {
                // Row doesn't exist locally -- insert
                result.applied += 1;
            }
            Some(ref local) if remote_hlc.is_after(local) => {
                // Remote wins LWW
                let is_tombstone = row
                    .get("is_deleted")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0)
                    == 1;
                if is_tombstone {
                    result.tombstoned += 1;
                } else {
                    result.applied += 1;
                }
            }
            Some(_) => {
                // Local wins LWW
                result.skipped_lww += 1;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl ChangeMerger for SqliteSyncMerger {
    async fn apply_changes(&self, changes: ChangeSet) -> Result<SyncResult, CoreError> {
        let conn = self.conn.clone();
        let local_device_id = self.local_device_id.clone();

        tokio::task::spawn_blocking(move || {
            let mut guard = conn.lock().map_err(|e| {
                CoreError::Internal(format!("SQLite lock poisoned: {e}"))
            })?;

            // Handle GDPR deletion event
            if changes.kind == ChangeSetKind::DeletionEvent {
                let deleted =
                    Self::handle_deletion_event(&guard, &changes.origin_device_id)?;
                return Ok(SyncResult {
                    tombstoned: deleted,
                    new_watermark: changes.watermark,
                    ..Default::default()
                });
            }

            // Skip self-originated changesets
            if changes.origin_device_id == local_device_id {
                debug!("skipping self-originated changeset");
                return Ok(SyncResult {
                    new_watermark: changes.watermark,
                    ..Default::default()
                });
            }

            let mut result = SyncResult::default();

            // All merge operations run inside a single transaction
            let tx = guard.transaction().map_err(|e| {
                CoreError::Internal(format!("begin transaction: {e}"))
            })?;

            // --- Append-only tables ---
            for row in &changes.segments {
                merge_segment(&tx, row, &mut result)?;
            }
            for row in &changes.overrides {
                merge_override(&tx, row, &mut result)?;
            }
            for row in &changes.param_snapshots {
                merge_param_snapshot(&tx, row, &mut result)?;
            }

            // --- LWW tables ---
            for row in &changes.regimes {
                merge_regime(&tx, row, &mut result)?;
            }
            for row in &changes.embeddings {
                merge_embedding(&tx, row, &mut result)?;
            }

            // --- Monotonic status merge (suggestions) ---
            for row in &changes.suggestions {
                merge_suggestion(&tx, row, &mut result)?;
            }

            // Update sync_peers watermark
            tx.execute(
                "INSERT INTO sync_peers (device_id, device_name, last_sync_at, \
                 watermark_wall_ms, watermark_counter) \
                 VALUES (?1, ?2, datetime('now'), ?3, ?4) \
                 ON CONFLICT(device_id) DO UPDATE SET \
                   device_name = excluded.device_name, \
                   last_sync_at = excluded.last_sync_at, \
                   watermark_wall_ms = excluded.watermark_wall_ms, \
                   watermark_counter = excluded.watermark_counter",
                rusqlite::params![
                    changes.origin_device_id,
                    changes.origin_device_name,
                    changes.watermark.wall_ms,
                    changes.watermark.counter,
                ],
            )
            .map_err(|e| CoreError::Internal(format!("update sync_peers: {e}")))?;

            tx.commit().map_err(|e| {
                CoreError::Internal(format!("commit transaction: {e}"))
            })?;

            result.new_watermark = changes.watermark;

            debug!(
                applied = result.applied,
                skipped_lww = result.skipped_lww,
                skipped_dup = result.skipped_dup,
                tombstoned = result.tombstoned,
                "changeset merge completed"
            );

            Ok(result)
        })
        .await
        .map_err(|e| CoreError::Internal(format!("spawn_blocking join error: {e}")))?
    }
}

// ── Per-table merge functions (called inside transaction) ──

fn merge_segment(
    conn: &Connection,
    row: &serde_json::Value,
    result: &mut SyncResult,
) -> Result<(), CoreError> {
    let id = json_str(row, "id")?;
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM activity_segments WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .map_err(|e| CoreError::Internal(format!("check segment: {e}")))?;

    if exists {
        result.skipped_dup += 1;
        return Ok(());
    }

    conn.execute(
        "INSERT INTO activity_segments \
         (id, start_time, end_time, duration_secs, regime_id, dominant_category, \
          app_breakdown, llm_summary, content_activities_json, \
          hlc_wall_ms, hlc_counter, origin_device_id) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            id,
            json_str(row, "start_time")?,
            json_str(row, "end_time")?,
            json_i64(row, "duration_secs")?,
            json_str_opt(row, "regime_id"),
            json_str(row, "dominant_category")?,
            json_str_or_default(row, "app_breakdown", "{}"),
            json_str_opt(row, "llm_summary"),
            json_str_or_default(row, "content_activities_json", "[]"),
            json_u64(row, "hlc_wall_ms")?,
            json_u32(row, "hlc_counter")?,
            json_str(row, "origin_device_id")?,
        ],
    )
    .map_err(|e| CoreError::Internal(format!("insert segment: {e}")))?;

    result.applied += 1;
    Ok(())
}

fn merge_regime(
    conn: &Connection,
    row: &serde_json::Value,
    result: &mut SyncResult,
) -> Result<(), CoreError> {
    let id = json_str(row, "id")?;
    let remote_hlc = extract_hlc(row)?;

    let local: Option<(u64, u32, String)> = conn
        .query_row(
            "SELECT hlc_wall_ms, hlc_counter, origin_device_id FROM regimes WHERE id = ?1",
            rusqlite::params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();

    match local {
        None => {
            conn.execute(
                "INSERT INTO regimes \
                 (id, label, detected_at, last_seen_at, occurrence_count, \
                  avg_density, avg_importance, dominant_category, params_snapshot_id, \
                  is_active, is_deleted, deleted_at, \
                  hlc_wall_ms, hlc_counter, origin_device_id) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
                rusqlite::params![
                    id,
                    json_str(row, "label")?,
                    json_str(row, "detected_at")?,
                    json_str(row, "last_seen_at")?,
                    json_i64(row, "occurrence_count")?,
                    json_f64(row, "avg_density")?,
                    json_f64(row, "avg_importance")?,
                    json_str(row, "dominant_category")?,
                    json_str_opt(row, "params_snapshot_id"),
                    json_i64(row, "is_active")?,
                    json_i64_or_default(row, "is_deleted", 0),
                    json_str_opt(row, "deleted_at"),
                    remote_hlc.wall_ms,
                    remote_hlc.counter,
                    json_str(row, "origin_device_id")?,
                ],
            )
            .map_err(|e| CoreError::Internal(format!("insert regime: {e}")))?;
            result.applied += 1;
        }
        Some((lw, lc, ld)) => {
            let local_hlc = Hlc {
                wall_ms: lw,
                counter: lc,
                device_id: ld,
            };
            if remote_hlc.is_after(&local_hlc) {
                conn.execute(
                    "UPDATE regimes SET label=?2, detected_at=?3, last_seen_at=?4, \
                     occurrence_count=?5, avg_density=?6, avg_importance=?7, \
                     dominant_category=?8, params_snapshot_id=?9, is_active=?10, \
                     is_deleted=?11, deleted_at=?12, \
                     hlc_wall_ms=?13, hlc_counter=?14, origin_device_id=?15 \
                     WHERE id = ?1",
                    rusqlite::params![
                        id,
                        json_str(row, "label")?,
                        json_str(row, "detected_at")?,
                        json_str(row, "last_seen_at")?,
                        json_i64(row, "occurrence_count")?,
                        json_f64(row, "avg_density")?,
                        json_f64(row, "avg_importance")?,
                        json_str(row, "dominant_category")?,
                        json_str_opt(row, "params_snapshot_id"),
                        json_i64(row, "is_active")?,
                        json_i64_or_default(row, "is_deleted", 0),
                        json_str_opt(row, "deleted_at"),
                        remote_hlc.wall_ms,
                        remote_hlc.counter,
                        json_str(row, "origin_device_id")?,
                    ],
                )
                .map_err(|e| CoreError::Internal(format!("update regime: {e}")))?;

                let is_tombstone = json_i64_or_default(row, "is_deleted", 0) == 1;
                if is_tombstone {
                    result.tombstoned += 1;
                } else {
                    result.applied += 1;
                }
            } else {
                result.skipped_lww += 1;
            }
        }
    }
    Ok(())
}

fn merge_override(
    conn: &Connection,
    row: &serde_json::Value,
    result: &mut SyncResult,
) -> Result<(), CoreError> {
    let id = json_str(row, "override_id")?;
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM regime_overrides WHERE override_id = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .map_err(|e| CoreError::Internal(format!("check override: {e}")))?;

    if exists {
        result.skipped_dup += 1;
        return Ok(());
    }

    conn.execute(
        "INSERT INTO regime_overrides \
         (override_id, segment_id, original_regime_id, action_type, action_data, \
          created_at, hlc_wall_ms, hlc_counter, origin_device_id) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        rusqlite::params![
            id,
            json_str(row, "segment_id")?,
            json_str_opt(row, "original_regime_id"),
            json_str(row, "action_type")?,
            json_str_opt(row, "action_data"),
            json_str(row, "created_at")?,
            json_u64(row, "hlc_wall_ms")?,
            json_u32(row, "hlc_counter")?,
            json_str(row, "origin_device_id")?,
        ],
    )
    .map_err(|e| CoreError::Internal(format!("insert override: {e}")))?;
    result.applied += 1;
    Ok(())
}

fn merge_embedding(
    conn: &Connection,
    row: &serde_json::Value,
    result: &mut SyncResult,
) -> Result<(), CoreError> {
    // embedding_vectors PK is autoincrement integer `id`, but for sync
    // we match on (segment_id, model_id) as the logical identity.
    let segment_id = json_str(row, "segment_id")?;
    let model_id = json_str(row, "model_id")?;
    let remote_hlc = extract_hlc(row)?;

    let local: Option<(i64, u64, u32, String)> = conn
        .query_row(
            "SELECT id, hlc_wall_ms, hlc_counter, origin_device_id \
             FROM embedding_vectors WHERE segment_id = ?1 AND model_id = ?2",
            rusqlite::params![segment_id, model_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .ok();

    match local {
        None => {
            // Decode hex-encoded vector back to BLOB
            let vector_hex = json_str(row, "vector")?;
            let vector_bytes = hex::decode(vector_hex).unwrap_or_default();

            conn.execute(
                "INSERT INTO embedding_vectors \
                 (segment_id, content_type, content_label, original_text, \
                  vector, model_id, timestamp, is_stale, \
                  is_deleted, deleted_at, \
                  hlc_wall_ms, hlc_counter, origin_device_id) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
                rusqlite::params![
                    segment_id,
                    json_str(row, "content_type")?,
                    json_str_opt(row, "content_label"),
                    json_str_opt(row, "original_text"),
                    vector_bytes,
                    model_id,
                    json_str(row, "timestamp")?,
                    json_i64_or_default(row, "is_stale", 0),
                    json_i64_or_default(row, "is_deleted", 0),
                    json_str_opt(row, "deleted_at"),
                    remote_hlc.wall_ms,
                    remote_hlc.counter,
                    json_str(row, "origin_device_id")?,
                ],
            )
            .map_err(|e| CoreError::Internal(format!("insert embedding: {e}")))?;
            result.applied += 1;
        }
        Some((local_id, lw, lc, ld)) => {
            let local_hlc = Hlc {
                wall_ms: lw,
                counter: lc,
                device_id: ld,
            };
            if remote_hlc.is_after(&local_hlc) {
                let vector_hex = json_str(row, "vector")?;
                let vector_bytes = hex::decode(vector_hex).unwrap_or_default();

                conn.execute(
                    "UPDATE embedding_vectors SET \
                     content_type=?2, content_label=?3, original_text=?4, \
                     vector=?5, model_id=?6, timestamp=?7, is_stale=?8, \
                     is_deleted=?9, deleted_at=?10, \
                     hlc_wall_ms=?11, hlc_counter=?12, origin_device_id=?13 \
                     WHERE id = ?1",
                    rusqlite::params![
                        local_id,
                        json_str(row, "content_type")?,
                        json_str_opt(row, "content_label"),
                        json_str_opt(row, "original_text"),
                        vector_bytes,
                        json_str(row, "model_id")?,
                        json_str(row, "timestamp")?,
                        json_i64_or_default(row, "is_stale", 0),
                        json_i64_or_default(row, "is_deleted", 0),
                        json_str_opt(row, "deleted_at"),
                        remote_hlc.wall_ms,
                        remote_hlc.counter,
                        json_str(row, "origin_device_id")?,
                    ],
                )
                .map_err(|e| CoreError::Internal(format!("update embedding: {e}")))?;

                let is_tombstone = json_i64_or_default(row, "is_deleted", 0) == 1;
                if is_tombstone {
                    result.tombstoned += 1;
                } else {
                    result.applied += 1;
                }
            } else {
                result.skipped_lww += 1;
            }
        }
    }
    Ok(())
}

fn merge_suggestion(
    conn: &Connection,
    row: &serde_json::Value,
    result: &mut SyncResult,
) -> Result<(), CoreError> {
    let suggestion_id = json_str(row, "suggestion_id")?;
    let remote_hlc = extract_hlc(row)?;
    let remote_status = SqliteSyncMerger::suggestion_status_ordinal(row);

    let local: Option<(u64, u32, String, Option<String>, Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT hlc_wall_ms, hlc_counter, origin_device_id, \
             shown_at, dismissed_at, acted_at \
             FROM suggestions WHERE suggestion_id = ?1",
            rusqlite::params![suggestion_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .ok();

    match local {
        None => {
            conn.execute(
                "INSERT INTO suggestions \
                 (suggestion_id, suggestion_type, source, content, priority, \
                  confidence_score, relevance_score, is_actionable, reasoning, \
                  shown_at, dismissed_at, acted_at, created_at, expires_at, \
                  is_deleted, deleted_at, \
                  hlc_wall_ms, hlc_counter, origin_device_id) \
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
                rusqlite::params![
                    suggestion_id,
                    json_str(row, "suggestion_type")?,
                    json_str(row, "source")?,
                    json_str(row, "content")?,
                    json_str(row, "priority")?,
                    json_f64(row, "confidence_score")?,
                    json_f64(row, "relevance_score")?,
                    json_i64(row, "is_actionable")?,
                    json_str_opt(row, "reasoning"),
                    json_str_opt(row, "shown_at"),
                    json_str_opt(row, "dismissed_at"),
                    json_str_opt(row, "acted_at"),
                    json_str(row, "created_at")?,
                    json_str_opt(row, "expires_at"),
                    json_i64_or_default(row, "is_deleted", 0),
                    json_str_opt(row, "deleted_at"),
                    remote_hlc.wall_ms,
                    remote_hlc.counter,
                    json_str(row, "origin_device_id")?,
                ],
            )
            .map_err(|e| CoreError::Internal(format!("insert suggestion: {e}")))?;
            result.applied += 1;
        }
        Some((lw, lc, ld, shown, dismissed, acted)) => {
            // Compute local status ordinal
            let local_status = if acted.is_some() {
                3
            } else if dismissed.is_some() {
                2
            } else if shown.is_some() {
                1
            } else {
                0
            };

            // Monotonic merge: higher status always wins
            let remote_wins = if remote_status != local_status {
                remote_status > local_status
            } else {
                // Same status -- fall back to HLC LWW
                let local_hlc = Hlc {
                    wall_ms: lw,
                    counter: lc,
                    device_id: ld,
                };
                remote_hlc.is_after(&local_hlc)
            };

            if remote_wins {
                conn.execute(
                    "UPDATE suggestions SET \
                     suggestion_type=?2, source=?3, content=?4, priority=?5, \
                     confidence_score=?6, relevance_score=?7, is_actionable=?8, \
                     reasoning=?9, shown_at=?10, dismissed_at=?11, acted_at=?12, \
                     expires_at=?13, is_deleted=?14, deleted_at=?15, \
                     hlc_wall_ms=?16, hlc_counter=?17, origin_device_id=?18 \
                     WHERE suggestion_id = ?1",
                    rusqlite::params![
                        suggestion_id,
                        json_str(row, "suggestion_type")?,
                        json_str(row, "source")?,
                        json_str(row, "content")?,
                        json_str(row, "priority")?,
                        json_f64(row, "confidence_score")?,
                        json_f64(row, "relevance_score")?,
                        json_i64(row, "is_actionable")?,
                        json_str_opt(row, "reasoning"),
                        json_str_opt(row, "shown_at"),
                        json_str_opt(row, "dismissed_at"),
                        json_str_opt(row, "acted_at"),
                        json_str_opt(row, "expires_at"),
                        json_i64_or_default(row, "is_deleted", 0),
                        json_str_opt(row, "deleted_at"),
                        remote_hlc.wall_ms,
                        remote_hlc.counter,
                        json_str(row, "origin_device_id")?,
                    ],
                )
                .map_err(|e| CoreError::Internal(format!("update suggestion: {e}")))?;
                result.applied += 1;
            } else {
                result.skipped_lww += 1;
            }
        }
    }
    Ok(())
}

fn merge_param_snapshot(
    conn: &Connection,
    row: &serde_json::Value,
    result: &mut SyncResult,
) -> Result<(), CoreError> {
    let id = json_str(row, "id")?;
    let exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM trigger_params_snapshots WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .map_err(|e| CoreError::Internal(format!("check param_snapshot: {e}")))?;

    if exists {
        result.skipped_dup += 1;
        return Ok(());
    }

    conn.execute(
        "INSERT INTO trigger_params_snapshots \
         (id, created_at, preset, params_json, hlc_wall_ms, hlc_counter, origin_device_id) \
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        rusqlite::params![
            id,
            json_str(row, "created_at")?,
            json_str(row, "preset")?,
            json_str(row, "params_json")?,
            json_u64(row, "hlc_wall_ms")?,
            json_u32(row, "hlc_counter")?,
            json_str(row, "origin_device_id")?,
        ],
    )
    .map_err(|e| CoreError::Internal(format!("insert param_snapshot: {e}")))?;
    result.applied += 1;
    Ok(())
}

// ── JSON extraction helpers ──

fn json_str<'a>(v: &'a serde_json::Value, key: &str) -> Result<&'a str, CoreError> {
    v.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| CoreError::Internal(format!("missing string field: {key}")))
}

fn json_str_opt(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn json_str_or_default<'a>(v: &'a serde_json::Value, key: &str, default: &'a str) -> &'a str {
    v.get(key).and_then(|v| v.as_str()).unwrap_or(default)
}

fn json_i64(v: &serde_json::Value, key: &str) -> Result<i64, CoreError> {
    v.get(key)
        .and_then(|v| v.as_i64())
        .ok_or_else(|| CoreError::Internal(format!("missing i64 field: {key}")))
}

fn json_i64_or_default(v: &serde_json::Value, key: &str, default: i64) -> i64 {
    v.get(key).and_then(|v| v.as_i64()).unwrap_or(default)
}

fn json_u64(v: &serde_json::Value, key: &str) -> Result<u64, CoreError> {
    v.get(key)
        .and_then(|v| v.as_u64())
        .ok_or_else(|| CoreError::Internal(format!("missing u64 field: {key}")))
}

fn json_u32(v: &serde_json::Value, key: &str) -> Result<u32, CoreError> {
    json_u64(v, key).map(|n| n as u32)
}

fn json_f64(v: &serde_json::Value, key: &str) -> Result<f64, CoreError> {
    v.get(key)
        .and_then(|v| v.as_f64())
        .ok_or_else(|| CoreError::Internal(format!("missing f64 field: {key}")))
}

fn extract_hlc(row: &serde_json::Value) -> Result<Hlc, CoreError> {
    Ok(Hlc {
        wall_ms: json_u64(row, "hlc_wall_ms")?,
        counter: json_u32(row, "hlc_counter")?,
        device_id: json_str(row, "origin_device_id")?.to_string(),
    })
}
```

- [ ] **Step 2: Register module in `lib.rs`**

Add to `crates/oneshim-storage/src/lib.rs`:

```rust
pub mod sync_merger;
```

- [ ] **Step 3: Add `hex` dependency to `Cargo.toml`**

Add to `[dependencies]`:

```toml
hex = "0.4"
```

- [ ] **Step 4: Verify compilation**

Run: `cargo check -p oneshim-storage`

- [ ] **Step 5: Add unit tests**

Add `#[cfg(test)] mod tests` at the bottom of `sync_merger.rs` covering:
- Empty changeset produces zero-count SyncResult
- Self-originated changeset is skipped
- Append-only insert succeeds, duplicate is skipped
- LWW: remote wins when HLC is higher
- LWW: local wins when HLC is lower
- Tombstone propagation on regime
- Monotonic suggestion merge: acted > dismissed
- DeletionEvent hard-deletes originator's rows

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::SqliteStorage;
    use oneshim_core::models::sync::ChangeSetKind;

    fn setup() -> (SqliteStorage, String) {
        let storage = SqliteStorage::open_in_memory(30).unwrap();
        let (device_id, _) = storage.ensure_device_identity("Local").unwrap();
        (storage, device_id)
    }

    #[tokio::test]
    async fn empty_changeset_returns_zero_counts() {
        let (storage, device_id) = setup();
        let merger = SqliteSyncMerger::new(storage.connection_arc(), device_id);
        let cs = ChangeSet {
            origin_device_id: "remote-dev".to_string(),
            origin_device_name: "Remote".to_string(),
            ..Default::default()
        };
        let result = merger.apply_changes(cs).await.unwrap();
        assert_eq!(result.applied, 0);
        assert_eq!(result.skipped_lww, 0);
        assert_eq!(result.skipped_dup, 0);
    }

    #[tokio::test]
    async fn self_originated_changeset_is_skipped() {
        let (storage, device_id) = setup();
        let merger = SqliteSyncMerger::new(storage.connection_arc(), device_id.clone());
        let cs = ChangeSet {
            origin_device_id: device_id,
            origin_device_name: "Local".to_string(),
            segments: vec![serde_json::json!({"id": "seg-1"})],
            ..Default::default()
        };
        let result = merger.apply_changes(cs).await.unwrap();
        assert_eq!(result.applied, 0);
    }

    #[tokio::test]
    async fn deletion_event_hard_deletes() {
        let (storage, local_id) = setup();
        let remote_id = "remote-dev";

        // Insert a segment from the remote device
        {
            let conn = storage.connection_arc();
            let guard = conn.lock().unwrap();
            guard.execute(
                "INSERT INTO activity_segments \
                 (id, start_time, end_time, duration_secs, trigger_reason, \
                  dominant_category, hlc_wall_ms, hlc_counter, origin_device_id) \
                 VALUES ('seg-r1', '2026-01-01', '2026-01-01', 3600, 'timer', \
                         'Dev', 100, 1, ?1)",
                rusqlite::params![remote_id],
            ).unwrap();
        }

        let merger = SqliteSyncMerger::new(storage.connection_arc(), local_id);
        let cs = ChangeSet {
            kind: ChangeSetKind::DeletionEvent,
            origin_device_id: remote_id.to_string(),
            origin_device_name: "Remote".to_string(),
            ..Default::default()
        };
        let result = merger.apply_changes(cs).await.unwrap();
        assert!(result.tombstoned > 0);

        // Verify row is gone
        let conn = storage.connection_arc();
        let guard = conn.lock().unwrap();
        let count: i64 = guard
            .query_row(
                "SELECT COUNT(*) FROM activity_segments WHERE id = 'seg-r1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn suggestion_monotonic_merge_acted_wins() {
        let (storage, local_id) = setup();

        // Insert a local suggestion at status "dismissed"
        {
            let conn = storage.connection_arc();
            let guard = conn.lock().unwrap();
            guard.execute(
                "INSERT INTO suggestions \
                 (suggestion_id, suggestion_type, content, priority, \
                  confidence_score, relevance_score, is_actionable, \
                  shown_at, dismissed_at, created_at, \
                  hlc_wall_ms, hlc_counter, origin_device_id) \
                 VALUES ('sug-1', 'focus', 'Take a break', 'MEDIUM', \
                         0.8, 0.7, 1, '2026-01-01T10:00:00', '2026-01-01T10:05:00', \
                         '2026-01-01T10:00:00', 200, 5, ?1)",
                rusqlite::params![local_id],
            ).unwrap();
        }

        let merger = SqliteSyncMerger::new(storage.connection_arc(), local_id);

        // Remote has same suggestion at status "acted" with LOWER HLC
        // Monotonic merge should still pick "acted" because acted(3) > dismissed(2)
        let remote_suggestion = serde_json::json!({
            "suggestion_id": "sug-1",
            "suggestion_type": "focus",
            "source": "RULE_BASED",
            "content": "Take a break",
            "priority": "MEDIUM",
            "confidence_score": 0.8,
            "relevance_score": 0.7,
            "is_actionable": 1,
            "reasoning": null,
            "shown_at": "2026-01-01T10:00:00",
            "dismissed_at": "2026-01-01T10:05:00",
            "acted_at": "2026-01-01T10:06:00",
            "created_at": "2026-01-01T10:00:00",
            "expires_at": null,
            "is_deleted": 0,
            "deleted_at": null,
            "hlc_wall_ms": 100,
            "hlc_counter": 1,
            "origin_device_id": "remote-dev"
        });

        let cs = ChangeSet {
            origin_device_id: "remote-dev".to_string(),
            origin_device_name: "Remote".to_string(),
            suggestions: vec![remote_suggestion],
            ..Default::default()
        };
        let result = merger.apply_changes(cs).await.unwrap();
        assert_eq!(result.applied, 1, "acted status should win over dismissed");
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p oneshim-storage -- sync_merger`

- [ ] **Step 7: Commit**

```
feat(storage): implement SqliteSyncMerger with LWW + monotonic status merge
```

---

## Task 5: Implement FileSyncTransport (oneshim-storage)

**File:** New `crates/oneshim-storage/src/file_transport.rs`

Reads/writes AES-256-GCM encrypted changeset files to a shared folder. Per-device file naming convention. Atomic write via `.tmp` + `fsync` + rename.

- [ ] **Step 1: Create `file_transport.rs`**

Key implementation details:
- **File naming**: `changeset-{device_id}-{hlc_wall_ms}-{hlc_counter}.enc`
- **Encryption**: AES-256-GCM with Argon2id KDF from user passphrase
- **Atomic write**: write to `.tmp`, fsync, rename
- **Pull**: scan folder for files from other devices, deserialize, return first changeset with HLC > since
- **Peer discovery**: list unique device_ids from filenames

```rust
//! FileSyncTransport -- encrypted changeset files in a shared folder.
//!
//! Each device writes its own changeset files. Other devices read them.
//! No file locking needed because each device owns its namespace via device_id prefix.

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use argon2::Argon2;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

use oneshim_core::error::CoreError;
use oneshim_core::models::sync::{ChangeSet, PeerInfo};
use oneshim_core::ports::sync_transport::SyncTransport;
use oneshim_core::sync::Hlc;

const NONCE_SIZE: usize = 12; // AES-256-GCM nonce
const SALT_SIZE: usize = 16;  // Argon2 salt

/// File-based sync transport with AES-256-GCM encryption.
pub struct FileSyncTransport {
    sync_folder: PathBuf,
    local_device_id: String,
    /// Raw passphrase (held in memory only while SyncEngine is alive).
    passphrase: String,
}

impl FileSyncTransport {
    pub fn new(
        sync_folder: PathBuf,
        local_device_id: String,
        passphrase: String,
    ) -> Result<Self, CoreError> {
        // Ensure the sync folder exists
        std::fs::create_dir_all(&sync_folder).map_err(|e| {
            CoreError::Internal(format!(
                "Failed to create sync folder {}: {e}",
                sync_folder.display()
            ))
        })?;

        Ok(Self {
            sync_folder,
            local_device_id,
            passphrase,
        })
    }

    /// Derive AES-256 key from passphrase + salt via Argon2id.
    fn derive_key(passphrase: &str, salt: &[u8]) -> Result<[u8; 32], CoreError> {
        let mut key = [0u8; 32];
        Argon2::default()
            .hash_password_into(passphrase.as_bytes(), salt, &mut key)
            .map_err(|e| CoreError::Internal(format!("Argon2 KDF failed: {e}")))?;
        Ok(key)
    }

    /// Encrypt plaintext with AES-256-GCM.
    /// Returns: salt (16) || nonce (12) || ciphertext
    fn encrypt(passphrase: &str, plaintext: &[u8]) -> Result<Vec<u8>, CoreError> {
        use aes_gcm::aead::rand_core::RngCore;
        let mut salt = [0u8; SALT_SIZE];
        OsRng.fill_bytes(&mut salt);

        let key = Self::derive_key(passphrase, &salt)?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| CoreError::Internal(format!("AES init: {e}")))?;

        let mut nonce_bytes = [0u8; NONCE_SIZE];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| CoreError::Internal(format!("AES encrypt: {e}")))?;

        let mut output = Vec::with_capacity(SALT_SIZE + NONCE_SIZE + ciphertext.len());
        output.extend_from_slice(&salt);
        output.extend_from_slice(&nonce_bytes);
        output.extend_from_slice(&ciphertext);
        Ok(output)
    }

    /// Decrypt: parse salt || nonce || ciphertext
    fn decrypt(passphrase: &str, data: &[u8]) -> Result<Vec<u8>, CoreError> {
        if data.len() < SALT_SIZE + NONCE_SIZE + 1 {
            return Err(CoreError::Internal("encrypted data too short".to_string()));
        }
        let salt = &data[..SALT_SIZE];
        let nonce_bytes = &data[SALT_SIZE..SALT_SIZE + NONCE_SIZE];
        let ciphertext = &data[SALT_SIZE + NONCE_SIZE..];

        let key = Self::derive_key(passphrase, salt)?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| CoreError::Internal(format!("AES init: {e}")))?;
        let nonce = Nonce::from_slice(nonce_bytes);

        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| CoreError::Internal(format!("AES decrypt failed (wrong passphrase?): {e}")))
    }

    /// Build the filename for a changeset.
    fn changeset_filename(device_id: &str, hlc: &Hlc) -> String {
        format!("changeset-{}-{}-{}.enc", device_id, hlc.wall_ms, hlc.counter)
    }

    /// Parse device_id and HLC from a changeset filename.
    fn parse_filename(name: &str) -> Option<(String, u64, u32)> {
        let name = name.strip_prefix("changeset-")?.strip_suffix(".enc")?;
        let parts: Vec<&str> = name.rsplitn(3, '-').collect();
        if parts.len() != 3 {
            return None;
        }
        let counter: u32 = parts[0].parse().ok()?;
        let wall_ms: u64 = parts[1].parse().ok()?;
        let device_id = parts[2].to_string();
        Some((device_id, wall_ms, counter))
    }
}

#[async_trait]
impl SyncTransport for FileSyncTransport {
    async fn push(&self, changes: &ChangeSet) -> Result<(), CoreError> {
        let folder = self.sync_folder.clone();
        let device_id = self.local_device_id.clone();
        let passphrase = self.passphrase.clone();
        let changes = changes.clone();

        tokio::task::spawn_blocking(move || {
            let json = serde_json::to_vec(&changes)?;
            let encrypted = Self::encrypt(&passphrase, &json)?;

            let filename = Self::changeset_filename(&device_id, &changes.watermark);
            let final_path = folder.join(&filename);
            let tmp_path = folder.join(format!("{filename}.tmp"));

            // Atomic write: write to .tmp, fsync, rename
            std::fs::write(&tmp_path, &encrypted)?;

            // fsync the file
            let file = std::fs::File::open(&tmp_path)?;
            file.sync_all()?;

            std::fs::rename(&tmp_path, &final_path)?;

            debug!(filename = %filename, bytes = encrypted.len(), "changeset pushed to file");
            Ok(())
        })
        .await
        .map_err(|e| CoreError::Internal(format!("spawn_blocking join error: {e}")))?
    }

    async fn pull(&self, since: &Hlc) -> Result<Option<ChangeSet>, CoreError> {
        let folder = self.sync_folder.clone();
        let local_device_id = self.local_device_id.clone();
        let passphrase = self.passphrase.clone();
        let since = since.clone();

        tokio::task::spawn_blocking(move || {
            let entries = std::fs::read_dir(&folder).map_err(|e| {
                CoreError::Internal(format!("read sync folder: {e}"))
            })?;

            let mut best: Option<(Hlc, PathBuf)> = None;

            for entry in entries {
                let entry = entry.map_err(|e| CoreError::Internal(format!("dir entry: {e}")))?;
                let name = entry.file_name().to_string_lossy().to_string();

                // Skip .tmp files and own files
                if name.ends_with(".tmp") {
                    continue;
                }

                if let Some((device_id, wall_ms, counter)) = Self::parse_filename(&name) {
                    // Skip own changesets
                    if device_id == local_device_id {
                        continue;
                    }

                    let file_hlc = Hlc {
                        wall_ms,
                        counter,
                        device_id: device_id.clone(),
                    };

                    // Only consider files newer than watermark
                    if !file_hlc.is_after(&since) {
                        continue;
                    }

                    // Pick the oldest unprocessed file (lowest HLC after since)
                    match &best {
                        None => best = Some((file_hlc, entry.path())),
                        Some((current_best, _)) if file_hlc < *current_best => {
                            best = Some((file_hlc, entry.path()));
                        }
                        _ => {}
                    }
                }
            }

            match best {
                None => Ok(None),
                Some((_, path)) => {
                    let data = std::fs::read(&path).map_err(|e| {
                        CoreError::Internal(format!("read changeset file: {e}"))
                    })?;
                    let plaintext = Self::decrypt(&passphrase, &data)?;
                    let cs: ChangeSet = serde_json::from_slice(&plaintext)?;
                    debug!(
                        file = %path.display(),
                        rows = cs.row_count(),
                        "changeset pulled from file"
                    );
                    Ok(Some(cs))
                }
            }
        })
        .await
        .map_err(|e| CoreError::Internal(format!("spawn_blocking join error: {e}")))?
    }

    async fn discover_peers(&self) -> Result<Vec<PeerInfo>, CoreError> {
        let folder = self.sync_folder.clone();
        let local_device_id = self.local_device_id.clone();

        tokio::task::spawn_blocking(move || {
            let entries = std::fs::read_dir(&folder).map_err(|e| {
                CoreError::Internal(format!("read sync folder: {e}"))
            })?;

            let mut peers: std::collections::HashMap<String, (u64, u32)> =
                std::collections::HashMap::new();

            for entry in entries {
                let entry = entry.map_err(|e| CoreError::Internal(format!("dir entry: {e}")))?;
                let name = entry.file_name().to_string_lossy().to_string();

                if let Some((device_id, wall_ms, counter)) = Self::parse_filename(&name) {
                    if device_id == local_device_id {
                        continue;
                    }
                    let existing = peers.entry(device_id).or_insert((0, 0));
                    if wall_ms > existing.0 || (wall_ms == existing.0 && counter > existing.1) {
                        *existing = (wall_ms, counter);
                    }
                }
            }

            Ok(peers
                .into_iter()
                .map(|(device_id, (wall_ms, counter))| PeerInfo {
                    device_id: device_id.clone(),
                    device_name: device_id, // Name not available from filenames alone
                    last_sync_at: chrono::Utc::now().to_rfc3339(),
                    watermark: Hlc {
                        wall_ms,
                        counter,
                        device_id: String::new(),
                    },
                })
                .collect())
        })
        .await
        .map_err(|e| CoreError::Internal(format!("spawn_blocking join error: {e}")))?
    }
}
```

- [ ] **Step 2: Register module in `lib.rs`**

Add to `crates/oneshim-storage/src/lib.rs`:

```rust
pub mod file_transport;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p oneshim-storage`

- [ ] **Step 4: Add unit tests**

Add `#[cfg(test)] mod tests` at the bottom of `file_transport.rs` covering:
- Encrypt/decrypt roundtrip
- Wrong passphrase fails decrypt
- Push creates `.enc` file (not `.tmp`)
- Pull returns None on empty folder
- Push then pull roundtrip
- Filename parsing
- Discover peers from files

- [ ] **Step 5: Run tests**

Run: `cargo test -p oneshim-storage -- file_transport`

- [ ] **Step 6: Commit**

```
feat(storage): implement FileSyncTransport with AES-256-GCM encryption
```

---

## Task 6: Implement SyncEngine orchestrator (src-tauri)

**File:** New `src-tauri/src/sync_engine.rs`

Pure orchestrator: pull from transport, merge via ChangeMerger, push via transport. Checks consent + config before each cycle. Handles pending GDPR deletion events.

- [ ] **Step 1: Create `sync_engine.rs`**

```rust
//! SyncEngine -- orchestrates the pull/merge/push sync cycle.
//!
//! This is a wiring-level component (no SQL, no transport logic).
//! It coordinates ChangeExtractor, ChangeMerger, and SyncTransport
//! through the port traits defined in oneshim-core.

use std::sync::Arc;
use tracing::{debug, info, warn};

use oneshim_core::consent::ConsentManager;
use oneshim_core::error::CoreError;
use oneshim_core::models::sync::{ChangeSet, ChangeSetKind, SyncResult};
use oneshim_core::ports::change_extractor::ChangeExtractor;
use oneshim_core::ports::change_merger::ChangeMerger;
use oneshim_core::ports::sync_transport::SyncTransport;
use oneshim_core::sync::Hlc;

pub struct SyncEngine {
    extractor: Arc<dyn ChangeExtractor>,
    merger: Arc<dyn ChangeMerger>,
    transport: Arc<dyn SyncTransport>,
    consent_manager: Arc<parking_lot::Mutex<ConsentManager>>,
    device_id: String,
    device_name: String,
}

impl SyncEngine {
    pub fn new(
        extractor: Arc<dyn ChangeExtractor>,
        merger: Arc<dyn ChangeMerger>,
        transport: Arc<dyn SyncTransport>,
        consent_manager: Arc<parking_lot::Mutex<ConsentManager>>,
        device_id: String,
        device_name: String,
    ) -> Self {
        Self {
            extractor,
            merger,
            transport,
            consent_manager,
            device_id,
            device_name,
        }
    }

    /// Run one complete sync cycle: check consent, handle deletion,
    /// pull + merge, extract + push.
    pub async fn run_cycle(&self) -> Result<Option<SyncResult>, CoreError> {
        // Gate 1: consent check
        {
            let cm = self.consent_manager.lock();
            if !cm.is_permitted(|p| p.cross_device_sync) {
                debug!("sync skipped: cross_device_sync consent not granted");
                return Ok(None);
            }
        }

        // Gate 2: check for pending GDPR deletion
        {
            let cm = self.consent_manager.lock();
            if cm.has_pending_deletion() {
                drop(cm); // release lock before async call
                return self.push_deletion_event().await;
            }
        }

        // --- Pull phase ---
        let local_watermark = self.extractor.local_watermark().await?;
        let mut merge_result: Option<SyncResult> = None;

        // Pull changesets in a loop until no more are available
        loop {
            let watermark = merge_result
                .as_ref()
                .map(|r| &r.new_watermark)
                .unwrap_or(&local_watermark);

            match self.transport.pull(watermark).await? {
                None => break,
                Some(changeset) => {
                    info!(
                        origin = %changeset.origin_device_id,
                        rows = changeset.row_count(),
                        "pulled changeset from transport"
                    );
                    let result = self.merger.apply_changes(changeset).await?;
                    debug!(
                        applied = result.applied,
                        skipped_lww = result.skipped_lww,
                        skipped_dup = result.skipped_dup,
                        tombstoned = result.tombstoned,
                        "merge completed"
                    );
                    merge_result = Some(result);
                }
            }
        }

        // --- Push phase ---
        let since = Hlc::default(); // Push all local changes (peers track their own watermarks)
        // In practice, use the peer's last-known watermark. For MVP with
        // file transport, we push our full local changeset on each cycle
        // and let the merger on the other side deduplicate.
        let local_changes = self.extractor.get_changes_since(&since).await?;

        if !local_changes.is_empty() {
            info!(rows = local_changes.row_count(), "pushing local changes");
            self.transport.push(&local_changes).await?;
        }

        Ok(merge_result)
    }

    /// Push a GDPR Article 17 deletion event and clear the pending flag.
    async fn push_deletion_event(&self) -> Result<Option<SyncResult>, CoreError> {
        info!("pushing GDPR Article 17 deletion event");

        let deletion_cs = ChangeSet {
            kind: ChangeSetKind::DeletionEvent,
            origin_device_id: self.device_id.clone(),
            origin_device_name: self.device_name.clone(),
            watermark: Hlc::now(&self.device_id),
            ..Default::default()
        };

        self.transport.push(&deletion_cs).await?;

        // Clear the pending deletion flag only after successful push
        {
            let mut cm = self.consent_manager.lock();
            cm.clear_pending_deletion();
        }

        info!("GDPR deletion event pushed successfully");
        Ok(None)
    }
}
```

- [ ] **Step 2: Register module in `src-tauri/src/main.rs`**

Add `mod sync_engine;` to the module declarations in `src-tauri/src/main.rs`.

- [ ] **Step 3: Verify compilation**

Run: `cd src-tauri && cargo check`

- [ ] **Step 4: Add unit tests**

Add `#[cfg(test)] mod tests` at the bottom of `sync_engine.rs` using mock implementations of the three port traits. Cover:
- Cycle skipped when consent not granted
- Deletion event pushed when has_pending_deletion
- Normal pull/merge/push cycle
- Empty pull results in push-only

- [ ] **Step 5: Run tests**

Run: `cd src-tauri && cargo test -- sync_engine`

- [ ] **Step 6: Commit**

```
feat(tauri): implement SyncEngine orchestrator for cross-device sync
```

---

## Task 7: Add sync loop to Scheduler (src-tauri)

**Files:**
- Modify: `src-tauri/src/scheduler/mod.rs`
- Modify: `src-tauri/src/scheduler/loops.rs`

Wire the SyncEngine into the scheduler as the 10th loop (alongside the existing 9 + OAuth + analysis).

- [ ] **Step 1: Add `sync_engine` field to `Scheduler`**

In `src-tauri/src/scheduler/mod.rs`, add to the `Scheduler` struct:

```rust
    pub(super) sync_engine: Option<Arc<crate::sync_engine::SyncEngine>>,
```

Add a builder method:

```rust
    pub fn with_sync_engine(mut self, engine: Arc<crate::sync_engine::SyncEngine>) -> Self {
        self.sync_engine = Some(engine);
        self
    }
```

Initialize the field in `Scheduler::new()`:

```rust
            sync_engine: None,
```

- [ ] **Step 2: Add `sync_interval` to `SchedulerConfig`**

In `src-tauri/src/scheduler/config.rs`, add:

```rust
    pub cross_device_sync_interval: Duration,
```

Initialize it from `SyncConfig::validated_interval_secs()` in the config builder.

- [ ] **Step 3: Add `spawn_cross_device_sync_loop` to `loops.rs`**

Add a new method to `impl Scheduler`:

```rust
    pub(super) fn spawn_cross_device_sync_loop(
        &self,
        sync_interval: Duration,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let sync_engine = self.sync_engine.clone();

        tokio::spawn(async move {
            let engine = match sync_engine {
                Some(e) => e,
                None => {
                    let _ = shutdown_rx.changed().await;
                    return;
                }
            };

            // Startup delay: wait 10 seconds before first sync
            tokio::time::sleep(Duration::from_secs(10)).await;

            let mut interval = tokio::time::interval(sync_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        match engine.run_cycle().await {
                            Ok(Some(result)) => {
                                info!(
                                    applied = result.applied,
                                    skipped = result.skipped_lww + result.skipped_dup,
                                    "cross-device sync cycle completed"
                                );
                            }
                            Ok(None) => {
                                debug!("cross-device sync cycle: no changes or skipped");
                            }
                            Err(e) => {
                                warn!("cross-device sync cycle failed: {e}");
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        // Push pending changes before shutdown
                        if let Err(e) = engine.run_cycle().await {
                            warn!("shutdown sync push failed: {e}");
                        }
                        info!("cross-device sync loop ended");
                        break;
                    }
                }
            }
        })
    }
```

- [ ] **Step 4: Wire into `run_scheduler_loops`**

In `run_scheduler_loops()`, add after the analysis task:

```rust
        let cross_device_sync_task = self.spawn_cross_device_sync_loop(
            self.config.cross_device_sync_interval,
            shutdown_rx.clone(),
        );
```

Add `cross_device_sync_task.abort();` to the shutdown cleanup block.

- [ ] **Step 5: Verify compilation**

Run: `cd src-tauri && cargo check`

- [ ] **Step 6: Commit**

```
feat(scheduler): add 10th loop for cross-device sync at configured interval
```

---

## Task 8: DI wiring in main.rs (src-tauri)

**File:** `src-tauri/src/main.rs`

Wire the concrete implementations at startup: construct `SqliteSyncExtractor`, `SqliteSyncMerger`, `FileSyncTransport`, and `SyncEngine`, then pass to `Scheduler::with_sync_engine()`.

- [ ] **Step 1: Add SyncEngine construction**

In the startup/DI section of `main.rs`, after `SqliteStorage` and `ConsentManager` initialization:

```rust
    // --- Cross-device sync (P3 Phase 3a-2) ---
    let sync_engine = if config.sync.enabled {
        let sync_folder = config.sync.sync_folder.as_ref().map(PathBuf::from);
        match sync_folder {
            Some(folder) => {
                // Passphrase must be provided via environment variable or prompt
                let passphrase = std::env::var("ONESHIM_SYNC_PASSPHRASE")
                    .unwrap_or_default();
                if passphrase.is_empty() {
                    warn!("sync enabled but ONESHIM_SYNC_PASSPHRASE not set; sync disabled");
                    None
                } else {
                    let (device_id, device_name) = sqlite_storage
                        .ensure_device_identity(&config.sync.device_name)?;

                    let extractor = Arc::new(SqliteSyncExtractor::new(
                        sqlite_storage.connection_arc(),
                        device_id.clone(),
                        device_name.clone(),
                        config.sync.clone(),
                    ));
                    let merger = Arc::new(SqliteSyncMerger::new(
                        sqlite_storage.connection_arc(),
                        device_id.clone(),
                    ));
                    let transport = Arc::new(FileSyncTransport::new(
                        folder,
                        device_id.clone(),
                        passphrase,
                    )?);

                    Some(Arc::new(SyncEngine::new(
                        extractor,
                        merger,
                        transport,
                        consent_manager.clone(),
                        device_id,
                        device_name,
                    )))
                }
            }
            None => {
                warn!("sync enabled but sync_folder not configured; sync disabled");
                None
            }
        }
    } else {
        None
    };

    // Pass to scheduler
    let scheduler = scheduler_builder
        // ... existing with_* calls ...
        ;
    let scheduler = if let Some(engine) = sync_engine {
        scheduler.with_sync_engine(engine)
    } else {
        scheduler
    };
```

- [ ] **Step 2: Add imports**

Add at the top of `main.rs`:

```rust
use oneshim_storage::sync_extractor::SqliteSyncExtractor;
use oneshim_storage::sync_merger::SqliteSyncMerger;
use oneshim_storage::file_transport::FileSyncTransport;
use crate::sync_engine::SyncEngine;
```

- [ ] **Step 3: Verify compilation**

Run: `cd src-tauri && cargo check`

- [ ] **Step 4: Commit**

```
feat(tauri): wire SyncEngine with concrete adapters in DI startup
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
| 1. SyncConfig fields | 1 modified | 2 | oneshim-core |
| 2. Crypto deps | 1 modified | 0 | oneshim-storage |
| 3. SqliteSyncExtractor | 2 new/modified | 4 | oneshim-storage |
| 4. SqliteSyncMerger | 2 new/modified | 4 | oneshim-storage |
| 5. FileSyncTransport | 2 new/modified | 7 | oneshim-storage |
| 6. SyncEngine | 2 new/modified | 4 | src-tauri |
| 7. Scheduler loop | 2 modified | 0 | src-tauri |
| 8. DI wiring | 1 modified | 0 | src-tauri |
| **Total** | **12 files** | **~21 tests** | |

## Dependencies Added

| Crate | Version | Target | Purpose |
|-------|---------|--------|---------|
| `aes-gcm` | `0.10` | oneshim-storage | AES-256-GCM encryption for changeset files |
| `argon2` | `0.5` | oneshim-storage | Argon2id KDF for passphrase-to-key derivation |
| `hex` | `0.4` | oneshim-storage | Hex encode/decode for BLOB vectors in JSON |

## What is NOT in this phase

- No `RemoteSyncTransport` (REST/gRPC) -- deferred to Phase 3b
- No `LanSyncTransport` (mDNS + TCP) -- deferred to Phase 3b+
- No web dashboard sync UI (peer list, merge conflict viewer) -- deferred to Phase 3b
- No bandwidth throttling -- deferred to Phase 3b
- No selective sync (per-table/per-device) -- deferred to Phase 3b
- No preferences sync (`preferences` field in ChangeSet remains empty) -- deferred to Phase 3b
- No passphrase prompt UI -- MVP uses `ONESHIM_SYNC_PASSPHRASE` env var

## Key Design Decisions

1. **ChangeSet uses `serde_json::Value` rows**: Avoids creating 7 typed row structs in this phase. The Phase 3a-1 ChangeSet already uses `Vec<serde_json::Value>`. The extractor produces JSON via SQLite's `json_object()`, and the merger parses it back. This is intentionally flexible for the MVP; typed row structs can be introduced in Phase 3b for compile-time safety.

2. **Passphrase via env var**: The user sets `ONESHIM_SYNC_PASSPHRASE` on each device. This avoids building UI for passphrase entry in this phase. Phase 3b will add a Tauri dialog or web dashboard input field.

3. **Push-all strategy**: On each push, the extractor sends all local changes (since epoch). The merger on the receiving side deduplicates via INSERT OR IGNORE / LWW. This simplifies the MVP at the cost of slightly larger changeset files (~15 KB/cycle based on bandwidth estimation). Optimized delta-only push with per-peer watermark tracking is deferred to Phase 3b.

4. **Single-file-per-push**: Each push creates one `.enc` file. Pull reads one file per cycle (the oldest unprocessed). This ensures ordering and avoids partial reads. Batch file cleanup (garbage collection) uses the existing retention policy.

5. **Transaction wrapping**: All merge operations for a single changeset run inside one SQLite transaction. This ensures atomicity -- a crash mid-merge won't leave the database in a half-applied state.
