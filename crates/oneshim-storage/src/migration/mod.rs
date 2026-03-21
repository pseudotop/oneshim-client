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
use tracing::info;

pub(crate) const CURRENT_VERSION: u32 = 18;

pub fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;

    let current = get_version(conn)?;
    info!("current schema version: {current}, target: {CURRENT_VERSION}");

    if current < 1 {
        v01_v08::migrate_v1(conn)?;
    }

    if current < 2 {
        v01_v08::migrate_v2(conn)?;
    }

    if current < 3 {
        v01_v08::migrate_v3(conn)?;
    }

    if current < 4 {
        v01_v08::migrate_v4(conn)?;
    }

    if current < 5 {
        v01_v08::migrate_v5(conn)?;
    }

    if current < 6 {
        v01_v08::migrate_v6(conn)?;
    }

    if current < 7 {
        v01_v08::migrate_v7(conn)?;
    }

    if current < 8 {
        v01_v08::migrate_v8(conn)?;
    }

    if current < 9 {
        v09_v18::migrate_v9(conn)?;
    }

    if current < 10 {
        v09_v18::migrate_v10(conn)?;
    }

    if current < 11 {
        v09_v18::migrate_v11(conn)?;
    }

    if current < 12 {
        v09_v18::migrate_v12(conn)?;
    }

    if current < 13 {
        v09_v18::migrate_v13(conn)?;
    }

    if current < 14 {
        v09_v18::migrate_v14(conn)?;
    }

    // V15 is reserved for Sync 3b (lan_peer_pins)
    if current < 15 {
        v09_v18::migrate_v15(conn)?;
    }

    if current < 16 {
        v09_v18::migrate_v16(conn)?;
    }

    if current < 17 {
        v09_v18::migrate_v17(conn)?;
    }

    if current < 18 {
        v09_v18::migrate_v18(conn)?;
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
