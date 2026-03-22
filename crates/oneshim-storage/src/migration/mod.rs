//! SQLite schema migrations for ONESHIM local storage.
//!
//! ## Directory Module Structure (ADR-003)
//!
//! - `mod.rs` — orchestrator (`run_migrations`, `get_version`, version constant)
//! - `v01_v08.rs` — foundation tables (events, frames, metrics, sessions, tags, edge intelligence)
//! - `v09_v18.rs` — tiered memory, vectors, sync, IVF index, coaching engine, trigram FTS

#[cfg(test)]
mod tests;
mod v01_v08;
mod v09_v18;

use rusqlite::Connection;
use tracing::{info, warn};

pub(crate) const CURRENT_VERSION: u32 = 18;

/// Back up the database file before running schema migrations.
fn backup_if_needed(conn: &Connection, current_version: u32) -> Option<std::path::PathBuf> {
    if current_version >= CURRENT_VERSION {
        return None;
    }

    // conn.path() returns Option<&str> in rusqlite 0.38+
    let db_path_str = conn.path().filter(|p| !p.is_empty() && *p != ":memory:")?;
    let db_path = std::path::PathBuf::from(db_path_str);

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup_path = db_path.with_extension(format!("backup.v{current_version}.{timestamp}"));

    match std::fs::copy(&db_path, &backup_path) {
        Ok(bytes) => {
            info!(
                "DB backup created before migration v{current_version}→v{CURRENT_VERSION}: {} ({bytes} bytes)",
                backup_path.display()
            );
            Some(backup_path)
        }
        Err(e) => {
            warn!("DB backup failed (continuing with migration): {e}");
            None
        }
    }
}

/// Execute a single migration step inside a SAVEPOINT for rollback safety.
fn run_migration_step(
    conn: &Connection,
    version: u32,
    migrate_fn: fn(&Connection) -> Result<(), rusqlite::Error>,
) -> Result<(), rusqlite::Error> {
    let sp_name = format!("migration_v{version}");
    conn.execute_batch(&format!("SAVEPOINT {sp_name}"))?;
    match migrate_fn(conn) {
        Ok(()) => {
            conn.execute_batch(&format!("RELEASE SAVEPOINT {sp_name}"))?;
            Ok(())
        }
        Err(e) => {
            warn!("migration v{version} failed, rolling back: {e}");
            let _ = conn.execute_batch(&format!("ROLLBACK TO SAVEPOINT {sp_name}"));
            Err(e)
        }
    }
}

pub fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;

    let current = get_version(conn)?;
    info!("current schema version: {current}, target: {CURRENT_VERSION}");

    backup_if_needed(conn, current);

    if current < 1 {
        run_migration_step(conn, 1, v01_v08::migrate_v1)?;
    }
    if current < 2 {
        run_migration_step(conn, 2, v01_v08::migrate_v2)?;
    }
    if current < 3 {
        run_migration_step(conn, 3, v01_v08::migrate_v3)?;
    }
    if current < 4 {
        run_migration_step(conn, 4, v01_v08::migrate_v4)?;
    }
    if current < 5 {
        run_migration_step(conn, 5, v01_v08::migrate_v5)?;
    }
    if current < 6 {
        run_migration_step(conn, 6, v01_v08::migrate_v6)?;
    }
    if current < 7 {
        run_migration_step(conn, 7, v01_v08::migrate_v7)?;
    }
    if current < 8 {
        run_migration_step(conn, 8, v01_v08::migrate_v8)?;
    }
    if current < 9 {
        run_migration_step(conn, 9, v09_v18::migrate_v9)?;
    }
    if current < 10 {
        run_migration_step(conn, 10, v09_v18::migrate_v10)?;
    }
    if current < 11 {
        run_migration_step(conn, 11, v09_v18::migrate_v11)?;
    }
    if current < 12 {
        run_migration_step(conn, 12, v09_v18::migrate_v12)?;
    }
    if current < 13 {
        run_migration_step(conn, 13, v09_v18::migrate_v13)?;
    }
    if current < 14 {
        run_migration_step(conn, 14, v09_v18::migrate_v14)?;
    }
    // V15 is reserved for Sync 3b (lan_peer_pins)
    if current < 15 {
        run_migration_step(conn, 15, v09_v18::migrate_v15)?;
    }
    if current < 16 {
        run_migration_step(conn, 16, v09_v18::migrate_v16)?;
    }
    if current < 17 {
        run_migration_step(conn, 17, v09_v18::migrate_v17)?;
    }
    if current < 18 {
        run_migration_step(conn, 18, v09_v18::migrate_v18)?;
    }

    Ok(())
}

fn get_version(conn: &Connection) -> Result<u32, rusqlite::Error> {
    let result: Result<u32, _> = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get(0),
    );
    result.or(Ok(0))
}
