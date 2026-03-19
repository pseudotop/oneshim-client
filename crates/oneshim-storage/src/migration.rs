use rusqlite::Connection;
use tracing::{debug, info};

const CURRENT_VERSION: u32 = 12;

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
        migrate_v1(conn)?;
    }

    if current < 2 {
        migrate_v2(conn)?;
    }

    if current < 3 {
        migrate_v3(conn)?;
    }

    if current < 4 {
        migrate_v4(conn)?;
    }

    if current < 5 {
        migrate_v5(conn)?;
    }

    if current < 6 {
        migrate_v6(conn)?;
    }

    if current < 7 {
        migrate_v7(conn)?;
    }

    if current < 8 {
        migrate_v8(conn)?;
    }

    if current < 9 {
        migrate_v9(conn)?;
    }

    if current < 10 {
        migrate_v10(conn)?;
    }

    if current < 11 {
        migrate_v11(conn)?;
    }

    if current < 12 {
        migrate_v12(conn)?;
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

fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("migration V1 execution: events + frames table");

    conn.execute_batch(
        "
        -- event save table
        CREATE TABLE IF NOT EXISTS events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            event_id TEXT NOT NULL UNIQUE,
            event_type TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            data TEXT NOT NULL,
            is_sent INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);
        CREATE INDEX IF NOT EXISTS idx_events_is_sent ON events(is_sent);
        CREATE INDEX IF NOT EXISTS idx_events_event_type ON events(event_type);

        -- frame index table
        CREATE TABLE IF NOT EXISTS frames (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            trigger_type TEXT NOT NULL,
            app_name TEXT NOT NULL,
            window_title TEXT NOT NULL,
            importance REAL NOT NULL,
            resolution_w INTEGER NOT NULL,
            resolution_h INTEGER NOT NULL,
            has_image INTEGER NOT NULL DEFAULT 0,
            ocr_text TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_frames_timestamp ON frames(timestamp);
        CREATE INDEX IF NOT EXISTS idx_frames_app_name ON frames(app_name);

        -- 버전 record
        INSERT INTO schema_version (version) VALUES (1);
        ",
    )?;

    info!("migration V1 completed");
    Ok(())
}

fn migrate_v2(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("migration V2 execution: frames.file_path column add");

    conn.execute_batch(
        "
        -- frames table에 file path column add
        ALTER TABLE frames ADD COLUMN file_path TEXT;

        -- file path index
        CREATE INDEX IF NOT EXISTS idx_frames_file_path ON frames(file_path);

        -- 버전 record
        INSERT INTO schema_version (version) VALUES (2);
        ",
    )?;

    info!("migration V2 completed");
    Ok(())
}

fn migrate_v3(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("migration V3 execution: system_metrics table");

    conn.execute_batch(
        "
        -- 시스템 메트릭 (5초 간격)
        CREATE TABLE IF NOT EXISTS system_metrics (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            cpu_usage REAL NOT NULL,
            memory_used INTEGER NOT NULL,
            memory_total INTEGER NOT NULL,
            disk_used INTEGER NOT NULL,
            disk_total INTEGER NOT NULL,
            network_upload INTEGER DEFAULT 0,
            network_download INTEGER DEFAULT 0,
            created_at TEXT DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_metrics_timestamp ON system_metrics(timestamp);

        -- 시간별 집계 (30일 보존)
        CREATE TABLE IF NOT EXISTS system_metrics_hourly (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            hour TEXT NOT NULL UNIQUE,
            cpu_avg REAL,
            cpu_max REAL,
            memory_avg INTEGER,
            memory_max INTEGER,
            sample_count INTEGER,
            created_at TEXT DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_metrics_hourly_hour ON system_metrics_hourly(hour);

        -- 버전 record
        INSERT INTO schema_version (version) VALUES (3);
        ",
    )?;

    info!("migration V3 completed");
    Ok(())
}

fn migrate_v4(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("migration V4 execution: process/idle/session table");

    conn.execute_batch(
        "
        -- 프로세스 스냅샷 (10초 간격)
        CREATE TABLE IF NOT EXISTS process_snapshots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            snapshot_data TEXT NOT NULL,
            created_at TEXT DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_process_timestamp ON process_snapshots(timestamp);

        -- idle period
        CREATE TABLE IF NOT EXISTS idle_periods (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            start_time TEXT NOT NULL,
            end_time TEXT,
            duration_secs INTEGER,
            created_at TEXT DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_idle_start ON idle_periods(start_time);

        -- session 통계
        CREATE TABLE IF NOT EXISTS session_stats (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL UNIQUE,
            started_at TEXT NOT NULL,
            ended_at TEXT,
            total_events INTEGER DEFAULT 0,
            total_frames INTEGER DEFAULT 0,
            total_idle_secs INTEGER DEFAULT 0,
            created_at TEXT DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_session_id ON session_stats(session_id);

        -- frames table에 창 위치 column add
        ALTER TABLE frames ADD COLUMN window_x INTEGER;
        ALTER TABLE frames ADD COLUMN window_y INTEGER;
        ALTER TABLE frames ADD COLUMN window_width INTEGER;
        ALTER TABLE frames ADD COLUMN window_height INTEGER;

        -- 버전 record
        INSERT INTO schema_version (version) VALUES (4);
        ",
    )?;

    info!("migration V4 completed");
    Ok(())
}

fn migrate_v5(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("migration V5 execution: tags + frame_tags table");

    conn.execute_batch(
        "
        -- 태그 table
        CREATE TABLE IF NOT EXISTS tags (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            color TEXT NOT NULL DEFAULT '#3b82f6',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_tags_name ON tags(name);

        -- frame-태그 connection table
        CREATE TABLE IF NOT EXISTS frame_tags (
            frame_id INTEGER NOT NULL,
            tag_id INTEGER NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (frame_id, tag_id),
            FOREIGN KEY (frame_id) REFERENCES frames(id) ON DELETE CASCADE,
            FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_frame_tags_frame ON frame_tags(frame_id);
        CREATE INDEX IF NOT EXISTS idx_frame_tags_tag ON frame_tags(tag_id);

        -- 버전 record
        INSERT INTO schema_version (version) VALUES (5);
        ",
    )?;

    info!("migration V5 completed");
    Ok(())
}

fn migrate_v6(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("migration V6 execution: Edge Intelligence table");

    conn.execute_batch(
        "
        -- 작업 session table (앱 카테고리별 집중 시간 추적)
        CREATE TABLE IF NOT EXISTS work_sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            started_at TEXT NOT NULL,
            ended_at TEXT,
            primary_app TEXT NOT NULL,
            category TEXT NOT NULL,
            state TEXT NOT NULL DEFAULT 'active',
            interruption_count INTEGER NOT NULL DEFAULT 0,
            deep_work_secs INTEGER NOT NULL DEFAULT 0,
            duration_secs INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_work_sessions_started ON work_sessions(started_at);
        CREATE INDEX IF NOT EXISTS idx_work_sessions_category ON work_sessions(category);
        CREATE INDEX IF NOT EXISTS idx_work_sessions_state ON work_sessions(state);

        -- 인터럽션 table (앱 전환 context 추적)
        CREATE TABLE IF NOT EXISTS interruptions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            interrupted_at TEXT NOT NULL,
            from_app TEXT NOT NULL,
            from_category TEXT NOT NULL,
            to_app TEXT NOT NULL,
            to_category TEXT NOT NULL,
            snapshot_frame_id INTEGER,
            resumed_at TEXT,
            resumed_to_app TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (snapshot_frame_id) REFERENCES frames(id) ON DELETE SET NULL
        );

        CREATE INDEX IF NOT EXISTS idx_interruptions_time ON interruptions(interrupted_at);
        CREATE INDEX IF NOT EXISTS idx_interruptions_from ON interruptions(from_app);

        -- 집중도 메트릭 table (일별 집계)
        CREATE TABLE IF NOT EXISTS focus_metrics (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            date TEXT NOT NULL UNIQUE,
            total_active_secs INTEGER NOT NULL DEFAULT 0,
            deep_work_secs INTEGER NOT NULL DEFAULT 0,
            communication_secs INTEGER NOT NULL DEFAULT 0,
            context_switches INTEGER NOT NULL DEFAULT 0,
            interruption_count INTEGER NOT NULL DEFAULT 0,
            avg_focus_duration_secs INTEGER NOT NULL DEFAULT 0,
            max_focus_duration_secs INTEGER NOT NULL DEFAULT 0,
            focus_score REAL NOT NULL DEFAULT 0.0,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_focus_metrics_date ON focus_metrics(date);

        -- 로컬 suggestion table (client 단독 suggestion)
        CREATE TABLE IF NOT EXISTS local_suggestions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            suggestion_type TEXT NOT NULL,
            payload TEXT NOT NULL,
            shown_at TEXT,
            dismissed_at TEXT,
            acted_at TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_local_suggestions_type ON local_suggestions(suggestion_type);
        CREATE INDEX IF NOT EXISTS idx_local_suggestions_created ON local_suggestions(created_at);

        -- 버전 record
        INSERT INTO schema_version (version) VALUES (6);
        ",
    )?;

    info!("migration V6 completed");
    Ok(())
}

fn migrate_v7(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("migration V7 execution: composite index performance optimization");

    conn.execute_batch(
        "
        -- events: sent되지 않은 event query 최적화 (is_sent=0 AND timestamp 정렬)
        CREATE INDEX IF NOT EXISTS idx_events_sent_timestamp ON events(is_sent, timestamp);

        -- work_sessions: active session query 최적화 (state='active' AND started_at)
        CREATE INDEX IF NOT EXISTS idx_work_sessions_state_started ON work_sessions(state, started_at);

        -- interruptions: 미복귀 인터럽션 query 최적화 (resumed_at IS NULL)
        CREATE INDEX IF NOT EXISTS idx_interruptions_not_resumed ON interruptions(resumed_at)
            WHERE resumed_at IS NULL;

        -- focus_metrics: 날짜 범위 query 최적화
        CREATE INDEX IF NOT EXISTS idx_focus_metrics_date_score ON focus_metrics(date, focus_score);

        -- local_suggestions: 미확인 suggestion query 최적화
        CREATE INDEX IF NOT EXISTS idx_suggestions_pending ON local_suggestions(shown_at, acted_at, dismissed_at)
            WHERE shown_at IS NULL OR (acted_at IS NULL AND dismissed_at IS NULL);

        -- 버전 record
        INSERT INTO schema_version (version) VALUES (7);
        ",
    )?;

    info!("migration V7 completed");
    Ok(())
}

fn migrate_v8(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("migration V8 execution: unified suggestions table");

    conn.execute_batch(
        "
        -- unified suggestions table (server + local + LLM)
        CREATE TABLE IF NOT EXISTS suggestions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            suggestion_id TEXT NOT NULL UNIQUE,
            suggestion_type TEXT NOT NULL,
            -- Default must match SuggestionSource::RULE_BASED_STR
            source TEXT NOT NULL DEFAULT 'RULE_BASED',
            content TEXT NOT NULL,
            priority TEXT NOT NULL DEFAULT 'MEDIUM',
            confidence_score REAL NOT NULL DEFAULT 0.0,
            relevance_score REAL NOT NULL DEFAULT 0.0,
            is_actionable INTEGER NOT NULL DEFAULT 1,
            reasoning TEXT,
            shown_at TEXT,
            dismissed_at TEXT,
            acted_at TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            expires_at TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_suggestions_source ON suggestions(source);
        CREATE INDEX IF NOT EXISTS idx_suggestions_created ON suggestions(created_at);
        CREATE INDEX IF NOT EXISTS idx_suggestions_type ON suggestions(suggestion_type);

        -- 버전 record
        INSERT INTO schema_version (version) VALUES (8);
        ",
    )?;

    info!("migration V8 completed");
    Ok(())
}

fn migrate_v9(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("migration V9 execution: tiered memory tables (calibration, regimes, segments)");

    conn.execute_batch(
        "
        -- Calibration log: one row per trigger event for offline tuning
        CREATE TABLE IF NOT EXISTS calibration_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            event_type TEXT NOT NULL,
            app_name TEXT NOT NULL,
            app_category TEXT NOT NULL,
            event_importance REAL NOT NULL,
            density_signal REAL NOT NULL,
            importance_signal REAL NOT NULL,
            context_signal REAL NOT NULL,
            buffer_signal REAL NOT NULL,
            trigger_score REAL NOT NULL,
            trigger_action TEXT,
            active_regime_id TEXT,
            params_version_id TEXT NOT NULL,
            is_noise INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_calibration_timestamp ON calibration_log(timestamp);
        CREATE INDEX IF NOT EXISTS idx_calibration_regime ON calibration_log(active_regime_id);
        CREATE INDEX IF NOT EXISTS idx_calibration_noise ON calibration_log(is_noise);
        CREATE INDEX IF NOT EXISTS idx_calibration_ts_noise ON calibration_log(timestamp, is_noise);

        -- Trigger parameter snapshots: immutable record per version
        CREATE TABLE IF NOT EXISTS trigger_params_snapshots (
            id TEXT PRIMARY KEY,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            preset TEXT NOT NULL,
            params_json TEXT NOT NULL
        );

        -- Regimes: detected behavioral regimes (clusters of similar activity)
        CREATE TABLE IF NOT EXISTS regimes (
            id TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            detected_at TEXT NOT NULL,
            last_seen_at TEXT NOT NULL,
            occurrence_count INTEGER NOT NULL DEFAULT 1,
            avg_density REAL NOT NULL DEFAULT 0.0,
            avg_importance REAL NOT NULL DEFAULT 0.0,
            dominant_category TEXT NOT NULL,
            params_snapshot_id TEXT,
            is_active INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (params_snapshot_id) REFERENCES trigger_params_snapshots(id)
        );

        CREATE INDEX IF NOT EXISTS idx_regimes_active ON regimes(is_active);
        CREATE INDEX IF NOT EXISTS idx_regimes_last_seen ON regimes(last_seen_at);

        -- Activity segments: closed segments produced by AdaptiveTrigger
        CREATE TABLE IF NOT EXISTS activity_segments (
            id TEXT PRIMARY KEY,
            start_time TEXT NOT NULL,
            end_time TEXT NOT NULL,
            duration_secs INTEGER NOT NULL,
            regime_id TEXT,
            trigger_reason TEXT NOT NULL,
            event_count INTEGER NOT NULL DEFAULT 0,
            app_breakdown TEXT NOT NULL DEFAULT '{}',
            category_breakdown TEXT NOT NULL DEFAULT '{}',
            context_switch_count INTEGER NOT NULL DEFAULT 0,
            dominant_category TEXT NOT NULL,
            avg_importance REAL NOT NULL DEFAULT 0.0,
            patterns_json TEXT NOT NULL DEFAULT '[]',
            content_activities_json TEXT NOT NULL DEFAULT '[]',
            container_json TEXT,
            llm_summary TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (regime_id) REFERENCES regimes(id)
        );

        CREATE INDEX IF NOT EXISTS idx_segments_start ON activity_segments(start_time);
        CREATE INDEX IF NOT EXISTS idx_segments_regime ON activity_segments(regime_id);
        CREATE INDEX IF NOT EXISTS idx_segments_reason ON activity_segments(trigger_reason);

        -- version record
        INSERT INTO schema_version (version) VALUES (9);
        ",
    )?;

    info!("migration V9 completed");
    Ok(())
}

fn migrate_v10(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("migration V10 execution: embedding vectors and weekly digests tables");

    conn.execute_batch(
        "
        -- Embedding vectors table (raw vectors stored as BLOB, indexed by sqlite-vec at runtime)
        CREATE TABLE IF NOT EXISTS embedding_vectors (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            segment_id TEXT NOT NULL,
            content_type TEXT NOT NULL,
            content_label TEXT,
            original_text TEXT NOT NULL,
            vector BLOB NOT NULL,
            model_id TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            is_stale INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_embedding_segment ON embedding_vectors(segment_id);
        CREATE INDEX IF NOT EXISTS idx_embedding_timestamp ON embedding_vectors(timestamp);
        CREATE INDEX IF NOT EXISTS idx_embedding_model ON embedding_vectors(model_id);
        CREATE INDEX IF NOT EXISTS idx_embedding_stale ON embedding_vectors(is_stale);

        -- Weekly digest table (aggregated stats per week)
        CREATE TABLE IF NOT EXISTS weekly_digests (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            week_start TEXT NOT NULL,
            week_end TEXT NOT NULL,
            stats_json TEXT NOT NULL,
            comparison_json TEXT,
            llm_narrative TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_digest_week ON weekly_digests(week_start);

        -- version record
        INSERT INTO schema_version (version) VALUES (10);
        ",
    )?;

    info!("migration V10 completed");
    Ok(())
}

fn migrate_v11(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("migration V11 execution: FTS5 search index + daily digests table");

    // FTS5 virtual table — may fail if the fts5 extension is not compiled in.
    // We log a warning and continue; the TextSearchProvider can return empty results.
    let fts5_result = conn.execute_batch(
        "
        -- FTS5 full-text search index
        CREATE VIRTUAL TABLE IF NOT EXISTS search_fts USING fts5(
            segment_id UNINDEXED,
            content_type,
            searchable_text,
            tokenize='porter unicode61'
        );

        -- Backfill existing segments
        INSERT OR IGNORE INTO search_fts (segment_id, content_type, searchable_text)
        SELECT id, 'segment', COALESCE(llm_summary, '') || ' ' || COALESCE(dominant_category, '')
        FROM activity_segments;
        ",
    );
    if let Err(e) = fts5_result {
        tracing::warn!("FTS5 table creation skipped (extension not available): {e}");
    }

    conn.execute_batch(
        "
        -- Daily digests
        CREATE TABLE IF NOT EXISTS daily_digests (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            date TEXT NOT NULL UNIQUE,
            insight_json TEXT,
            timeline_json TEXT NOT NULL,
            statistics_json TEXT NOT NULL,
            generated_at TEXT NOT NULL
        );

        -- version record
        INSERT INTO schema_version (version) VALUES (11);
        ",
    )?;

    info!("migration V11 completed");
    Ok(())
}

fn migrate_v12(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("migration V12 execution: regime_overrides table for recalibration");

    conn.execute_batch(
        "
        -- User regime overrides for constraint-based re-clustering
        CREATE TABLE IF NOT EXISTS regime_overrides (
            override_id TEXT PRIMARY KEY,
            segment_id TEXT NOT NULL,
            original_regime_id TEXT,
            action_type TEXT NOT NULL,
            action_data TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_override_segment ON regime_overrides(segment_id);
        CREATE INDEX IF NOT EXISTS idx_override_created ON regime_overrides(created_at);

        -- version record
        INSERT INTO schema_version (version) VALUES (12);
        ",
    )?;

    info!("migration V12 completed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_all_versions() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='events'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='frames'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let has_file_path: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('frames') WHERE name='file_path'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(has_file_path, 1);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='system_metrics'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='system_metrics_hourly'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='process_snapshots'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='idle_periods'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='session_stats'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let has_window_x: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('frames') WHERE name='window_x'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(has_window_x, 1);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='tags'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='frame_tags'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='work_sessions'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='interruptions'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='focus_metrics'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='local_suggestions'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_events_sent_timestamp'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_work_sessions_state_started'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let version: u32 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, 12);

        // V9 tables
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='calibration_log'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='trigger_params_snapshots'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='regimes'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='activity_segments'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // V10 tables
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='embedding_vectors'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='weekly_digests'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // V11 tables
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='daily_digests'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // FTS5 virtual table
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='search_fts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // V12 tables
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='regime_overrides'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn migration_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap(); // execution error none
        let version: u32 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, 12);
    }
}
