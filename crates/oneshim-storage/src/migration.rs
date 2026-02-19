//! 스키마 마이그레이션.
//!
//! 버전 기반 SQLite 스키마 관리.

use rusqlite::Connection;
use tracing::{debug, info};

/// 현재 스키마 버전
const CURRENT_VERSION: u32 = 7;

/// 스키마 마이그레이션 실행
pub fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    // schema_version 테이블 생성
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;

    let current = get_version(conn)?;
    info!("현재 스키마 버전: {current}, 목표: {CURRENT_VERSION}");

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

    Ok(())
}

/// 현재 스키마 버전 조회
fn get_version(conn: &Connection) -> Result<u32, rusqlite::Error> {
    let result: Result<u32, _> = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get(0),
    );
    result.or(Ok(0))
}

/// V1: events + frames 테이블 생성
fn migrate_v1(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("마이그레이션 V1 실행: events + frames 테이블");

    conn.execute_batch(
        "
        -- 이벤트 저장 테이블
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

        -- 프레임 인덱스 테이블
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

        -- 버전 기록
        INSERT INTO schema_version (version) VALUES (1);
        ",
    )?;

    info!("마이그레이션 V1 완료");
    Ok(())
}

/// V2: frames 테이블에 file_path 컬럼 추가
fn migrate_v2(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("마이그레이션 V2 실행: frames.file_path 컬럼 추가");

    conn.execute_batch(
        "
        -- frames 테이블에 파일 경로 컬럼 추가
        ALTER TABLE frames ADD COLUMN file_path TEXT;

        -- 파일 경로 인덱스
        CREATE INDEX IF NOT EXISTS idx_frames_file_path ON frames(file_path);

        -- 버전 기록
        INSERT INTO schema_version (version) VALUES (2);
        ",
    )?;

    info!("마이그레이션 V2 완료");
    Ok(())
}

/// V3: system_metrics + system_metrics_hourly 테이블 생성
fn migrate_v3(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("마이그레이션 V3 실행: system_metrics 테이블");

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

        -- 버전 기록
        INSERT INTO schema_version (version) VALUES (3);
        ",
    )?;

    info!("마이그레이션 V3 완료");
    Ok(())
}

/// V4: process_snapshots, idle_periods, session_stats 테이블 + frames window bounds
fn migrate_v4(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("마이그레이션 V4 실행: process/idle/session 테이블");

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

        -- 유휴 기간
        CREATE TABLE IF NOT EXISTS idle_periods (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            start_time TEXT NOT NULL,
            end_time TEXT,
            duration_secs INTEGER,
            created_at TEXT DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_idle_start ON idle_periods(start_time);

        -- 세션 통계
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

        -- frames 테이블에 창 위치 컬럼 추가
        ALTER TABLE frames ADD COLUMN window_x INTEGER;
        ALTER TABLE frames ADD COLUMN window_y INTEGER;
        ALTER TABLE frames ADD COLUMN window_width INTEGER;
        ALTER TABLE frames ADD COLUMN window_height INTEGER;

        -- 버전 기록
        INSERT INTO schema_version (version) VALUES (4);
        ",
    )?;

    info!("마이그레이션 V4 완료");
    Ok(())
}

/// V5: tags + frame_tags 테이블 생성 (태그/주석 기능)
fn migrate_v5(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("마이그레이션 V5 실행: tags + frame_tags 테이블");

    conn.execute_batch(
        "
        -- 태그 테이블
        CREATE TABLE IF NOT EXISTS tags (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            color TEXT NOT NULL DEFAULT '#3b82f6',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_tags_name ON tags(name);

        -- 프레임-태그 연결 테이블
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

        -- 버전 기록
        INSERT INTO schema_version (version) VALUES (5);
        ",
    )?;

    info!("마이그레이션 V5 완료");
    Ok(())
}

/// V6: work_sessions, interruptions, focus_metrics 테이블 (Edge Intelligence)
fn migrate_v6(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("마이그레이션 V6 실행: Edge Intelligence 테이블");

    conn.execute_batch(
        "
        -- 작업 세션 테이블 (앱 카테고리별 집중 시간 추적)
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

        -- 인터럽션 테이블 (앱 전환 컨텍스트 추적)
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

        -- 집중도 메트릭 테이블 (일별 집계)
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

        -- 로컬 제안 테이블 (클라이언트 단독 제안)
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

        -- 버전 기록
        INSERT INTO schema_version (version) VALUES (6);
        ",
    )?;

    info!("마이그레이션 V6 완료");
    Ok(())
}

/// V7: 복합 인덱스 추가 (성능 최적화)
fn migrate_v7(conn: &Connection) -> Result<(), rusqlite::Error> {
    debug!("마이그레이션 V7 실행: 복합 인덱스 성능 최적화");

    conn.execute_batch(
        "
        -- events: 전송되지 않은 이벤트 조회 최적화 (is_sent=0 AND timestamp 정렬)
        CREATE INDEX IF NOT EXISTS idx_events_sent_timestamp ON events(is_sent, timestamp);

        -- work_sessions: 활성 세션 조회 최적화 (state='active' AND started_at)
        CREATE INDEX IF NOT EXISTS idx_work_sessions_state_started ON work_sessions(state, started_at);

        -- interruptions: 미복귀 인터럽션 조회 최적화 (resumed_at IS NULL)
        CREATE INDEX IF NOT EXISTS idx_interruptions_not_resumed ON interruptions(resumed_at)
            WHERE resumed_at IS NULL;

        -- focus_metrics: 날짜 범위 조회 최적화
        CREATE INDEX IF NOT EXISTS idx_focus_metrics_date_score ON focus_metrics(date, focus_score);

        -- local_suggestions: 미확인 제안 조회 최적화
        CREATE INDEX IF NOT EXISTS idx_suggestions_pending ON local_suggestions(shown_at, acted_at, dismissed_at)
            WHERE shown_at IS NULL OR (acted_at IS NULL AND dismissed_at IS NULL);

        -- 버전 기록
        INSERT INTO schema_version (version) VALUES (7);
        ",
    )?;

    info!("마이그레이션 V7 완료");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_all_versions() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        // events 테이블 존재 확인
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='events'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // frames 테이블 존재 확인
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='frames'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // V2: file_path 컬럼 존재 확인
        let has_file_path: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('frames') WHERE name='file_path'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(has_file_path, 1);

        // V3: system_metrics 테이블 존재 확인
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='system_metrics'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // V3: system_metrics_hourly 테이블 존재 확인
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='system_metrics_hourly'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // V4: process_snapshots 테이블 존재 확인
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='process_snapshots'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // V4: idle_periods 테이블 존재 확인
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='idle_periods'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // V4: session_stats 테이블 존재 확인
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='session_stats'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // V4: frames 테이블에 window bounds 컬럼 존재 확인
        let has_window_x: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('frames') WHERE name='window_x'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(has_window_x, 1);

        // V5: tags 테이블 존재 확인
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='tags'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // V5: frame_tags 테이블 존재 확인
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='frame_tags'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // V6: work_sessions 테이블 존재 확인
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='work_sessions'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // V6: interruptions 테이블 존재 확인
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='interruptions'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // V6: focus_metrics 테이블 존재 확인
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='focus_metrics'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // V6: local_suggestions 테이블 존재 확인
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='local_suggestions'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // V7: 복합 인덱스 존재 확인
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

        // 최종 버전 확인
        let version: u32 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, 7);
    }

    #[test]
    fn migration_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap(); // 두 번 실행해도 에러 없음

        let version: u32 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, 7);
    }
}
