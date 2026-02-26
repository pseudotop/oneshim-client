[English](./oneshim-storage.md) | [한국어](./oneshim-storage.ko.md)

# oneshim-storage

The SQLite-based local data storage crate.

## Role

- **Offline Support**: Local storage when network is unavailable
- **Event History**: Persistent storage of context events
- **Frame Cache**: Temporary storage of processed frames
- **Data Retention**: Automatic cleanup based on configured period/capacity

## Directory Structure

```
oneshim-storage/src/
├── lib.rs         # Crate root
├── sqlite.rs      # SqliteStorage - StorageService implementation
└── migration.rs   # Schema migration
```

## Key Components

### SqliteStorage (sqlite.rs)

`StorageService` port implementation:

```rust
pub struct SqliteStorage {
    pool: Pool<SqliteConnectionManager>,
}

impl SqliteStorage {
    pub fn new(db_path: &Path) -> Result<Self, CoreError> {
        let manager = SqliteConnectionManager::file(db_path);
        let pool = Pool::builder()
            .max_size(5)
            .build(manager)?;

        let storage = Self { pool };
        storage.run_migrations()?;

        Ok(storage)
    }
}
```

### StorageService Implementation

```rust
#[async_trait]
impl StorageService for SqliteStorage {
    async fn save_event(&self, event: &ContextEvent) -> Result<(), CoreError> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO events (event_id, event_type, window_title, app_name, timestamp, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event.event_id,
                event.event_type.to_string(),
                event.window_title,
                event.app_name,
                event.timestamp.to_rfc3339(),
                serde_json::to_string(&event.metadata)?,
            ],
        )?;
        Ok(())
    }

    async fn get_events(&self, since: DateTime<Utc>) -> Result<Vec<ContextEvent>, CoreError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT event_id, event_type, window_title, app_name, timestamp, metadata
             FROM events WHERE timestamp >= ?1 ORDER BY timestamp ASC"
        )?;

        let events = stmt.query_map([since.to_rfc3339()], |row| {
            // Convert to ContextEvent
        })?;

        Ok(events.collect::<Result<Vec<_>, _>>()?)
    }

    async fn save_frame(&self, frame: &ProcessedFrame) -> Result<(), CoreError> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO frames (frame_id, data, processing_type, width, height, captured_at, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                frame.frame_id,
                frame.data,
                frame.processing_type.to_string(),
                frame.width,
                frame.height,
                frame.captured_at.to_rfc3339(),
                serde_json::to_string(&frame.metadata)?,
            ],
        )?;
        Ok(())
    }

    async fn cleanup_old_data(&self, before: DateTime<Utc>) -> Result<usize, CoreError> {
        let conn = self.pool.get()?;

        let events_deleted = conn.execute(
            "DELETE FROM events WHERE timestamp < ?1",
            [before.to_rfc3339()],
        )?;

        let frames_deleted = conn.execute(
            "DELETE FROM frames WHERE captured_at < ?1",
            [before.to_rfc3339()],
        )?;

        Ok(events_deleted + frames_deleted)
    }
}
```

## Database Schema

### V1 Schema (migration.rs)

```sql
-- events table: stores context events
CREATE TABLE IF NOT EXISTS events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL UNIQUE,
    event_type TEXT NOT NULL,
    window_title TEXT,
    app_name TEXT,
    timestamp TEXT NOT NULL,
    metadata TEXT,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_events_timestamp ON events(timestamp);
CREATE INDEX idx_events_event_type ON events(event_type);

-- frames table: stores processed frames
CREATE TABLE IF NOT EXISTS frames (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    frame_id TEXT NOT NULL UNIQUE,
    data BLOB NOT NULL,
    processing_type TEXT NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    captured_at TEXT NOT NULL,
    metadata TEXT,
    created_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_frames_captured_at ON frames(captured_at);
```

### WAL Mode

```rust
fn configure_connection(conn: &Connection) -> Result<(), CoreError> {
    conn.execute_batch("
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA temp_store = MEMORY;
        PRAGMA mmap_size = 268435456;
    ")?;
    Ok(())
}
```

**WAL Benefits**:
- Concurrent read/write operations
- Improved write performance
- Crash recovery stability

## Data Retention Policy

Automatic cleanup based on configuration:

```rust
pub struct RetentionPolicy {
    pub max_days: u32,       // Default 30 days
    pub max_size_mb: u32,    // Default 500MB
}

impl SqliteStorage {
    pub async fn enforce_retention(&self, policy: &RetentionPolicy) -> Result<(), CoreError> {
        // Time-based cleanup
        let cutoff = Utc::now() - Duration::days(policy.max_days as i64);
        self.cleanup_old_data(cutoff).await?;

        // Capacity-based cleanup (if needed)
        let size = self.get_database_size()?;
        if size > policy.max_size_mb * 1024 * 1024 {
            self.vacuum().await?;
        }

        Ok(())
    }
}
```

## Database Location

| Platform | Path |
|----------|------|
| macOS | `~/Library/Application Support/oneshim/data.db` |
| Windows | `%APPDATA%\oneshim\data.db` |
| Linux | `~/.local/share/oneshim/data.db` |

## Offline Synchronization

Syncs unsent data when network is restored:

```rust
impl SqliteStorage {
    /// Query events not yet sent to the server
    pub async fn get_pending_events(&self) -> Result<Vec<ContextEvent>, CoreError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM events WHERE synced_at IS NULL ORDER BY timestamp ASC LIMIT 100"
        )?;
        // ...
    }

    /// Mark events as synced
    pub async fn mark_synced(&self, event_ids: &[String]) -> Result<(), CoreError> {
        let conn = self.pool.get()?;
        let sql = format!(
            "UPDATE events SET synced_at = ?1 WHERE event_id IN ({})",
            event_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",")
        );
        // ...
    }
}
```

## Dependencies

- `rusqlite`: SQLite bindings (bundled mode)
- `r2d2`: Connection pool
- `directories`: Platform-specific data directories

## Tests

```rust
#[tokio::test]
async fn test_event_crud() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let storage = SqliteStorage::new(&db_path).unwrap();

    // Save event
    let event = ContextEvent {
        event_id: "evt_001".to_string(),
        event_type: EventType::WindowFocus,
        window_title: Some("Test Window".to_string()),
        app_name: Some("Test App".to_string()),
        timestamp: Utc::now(),
        metadata: serde_json::json!({}),
    };
    storage.save_event(&event).await.unwrap();

    // Query events
    let since = Utc::now() - Duration::hours(1);
    let events = storage.get_events(since).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_id, "evt_001");
}

#[tokio::test]
async fn test_cleanup() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let storage = SqliteStorage::new(&db_path).unwrap();

    // Save old event
    let old_event = ContextEvent {
        timestamp: Utc::now() - Duration::days(60),
        // ...
    };
    storage.save_event(&old_event).await.unwrap();

    // Run cleanup
    let cutoff = Utc::now() - Duration::days(30);
    let deleted = storage.cleanup_old_data(cutoff).await.unwrap();
    assert_eq!(deleted, 1);
}
```

## Performance Characteristics

| Operation | Expected Time |
|-----------|--------------|
| Single event save | < 1ms |
| Query 100 events | < 5ms |
| Save frame (100KB) | < 10ms |
| VACUUM | Several seconds (depends on data volume) |
