use rusqlite::Connection;

pub fn migrate_v20(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS session_audit_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL DEFAULT (datetime('now')),
            session_id TEXT NOT NULL,
            category TEXT NOT NULL,
            event_type TEXT NOT NULL,
            provider TEXT NOT NULL DEFAULT '',
            payload TEXT,
            duration_ms INTEGER,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_session_audit_session_id
            ON session_audit_log(session_id);
        CREATE INDEX IF NOT EXISTS idx_session_audit_category
            ON session_audit_log(category);
        CREATE INDEX IF NOT EXISTS idx_session_audit_timestamp
            ON session_audit_log(timestamp);

        INSERT OR IGNORE INTO schema_version (version) VALUES (20);",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// Create the prerequisite schema_version table (normally created by run_migrations).
    fn setup_schema_version(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .unwrap();
    }

    #[test]
    fn creates_session_audit_log_table() {
        let conn = Connection::open_in_memory().unwrap();
        setup_schema_version(&conn);
        migrate_v20(&conn).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM session_audit_log", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn inserts_and_queries_audit_entry() {
        let conn = Connection::open_in_memory().unwrap();
        setup_schema_version(&conn);
        migrate_v20(&conn).unwrap();

        conn.execute(
            "INSERT INTO session_audit_log (session_id, category, event_type, provider)
             VALUES (?1, ?2, ?3, ?4)",
            ["sess-1", "session", "create", "claude"],
        )
        .unwrap();

        let session_id: String = conn
            .query_row(
                "SELECT session_id FROM session_audit_log WHERE category = 'session'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(session_id, "sess-1");
    }

    #[test]
    fn migration_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        setup_schema_version(&conn);
        migrate_v20(&conn).unwrap();
        migrate_v20(&conn).unwrap();
    }
}
