//! Edge Intelligence 저장소 메서드 (V6 스키마).
//!
//! 작업 세션, 인터럽션, 집중도 메트릭, 로컬 제안 관련 스토리지.

use chrono::{DateTime, Utc};
use oneshim_core::error::CoreError;
use oneshim_core::models::work_session::{
    AppCategory, FocusMetrics, Interruption, LocalSuggestion, SessionState, WorkSession,
};
use tracing::debug;

use super::{
    FocusInterruptionRecord, FocusWorkSessionRecord, LocalSuggestionRecord, SqliteStorage,
};

impl SqliteStorage {
    // --------------------------------------------------------
    // 작업 세션
    // --------------------------------------------------------

    /// 새 작업 세션 시작
    pub fn start_work_session(
        &self,
        primary_app: &str,
        category: AppCategory,
    ) -> Result<WorkSession, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let now = Utc::now();
        let category_str = format!("{:?}", category);

        conn.execute(
            "INSERT INTO work_sessions (started_at, primary_app, category, state)
             VALUES (?1, ?2, ?3, 'active')",
            rusqlite::params![now.to_rfc3339(), primary_app, category_str],
        )
        .map_err(|e| CoreError::Internal(format!("작업 세션 시작 실패: {e}")))?;

        let id = conn.last_insert_rowid();
        debug!(
            "작업 세션 시작: id={}, app={}, category={:?}",
            id, primary_app, category
        );

        Ok(WorkSession {
            id,
            started_at: now,
            ended_at: None,
            primary_app: primary_app.to_string(),
            category,
            state: SessionState::Active,
            interruption_count: 0,
            deep_work_secs: 0,
            duration_secs: 0,
        })
    }

    /// 진행 중인 작업 세션 조회
    pub fn get_active_work_session(&self) -> Result<Option<WorkSession>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let result = conn.query_row(
            "SELECT id, started_at, primary_app, category, interruption_count, deep_work_secs, duration_secs
             FROM work_sessions WHERE state = 'active' ORDER BY id DESC LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, u64>(5)?,
                    row.get::<_, u64>(6)?,
                ))
            },
        );

        match result {
            Ok((
                id,
                started_str,
                primary_app,
                category_str,
                interruption_count,
                deep_work_secs,
                duration_secs,
            )) => {
                let started_at = DateTime::parse_from_rfc3339(&started_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                let category = Self::parse_app_category(&category_str);

                Ok(Some(WorkSession {
                    id,
                    started_at,
                    ended_at: None,
                    primary_app,
                    category,
                    state: SessionState::Active,
                    interruption_count,
                    deep_work_secs,
                    duration_secs,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(CoreError::Internal(format!("작업 세션 조회 실패: {e}"))),
        }
    }

    /// 작업 세션 종료
    ///
    /// RETURNING clause로 SELECT+UPDATE를 1개 쿼리로 최적화 (N+1 제거)
    pub fn end_work_session(&self, session_id: i64) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let now = Utc::now();
        let now_str = now.to_rfc3339();

        // RETURNING clause로 duration_secs를 한 번에 계산 + 반환
        // julianday 차이 * 86400 = 초 단위 기간
        let duration_secs: i64 = conn
            .query_row(
                "UPDATE work_sessions
                 SET ended_at = ?1,
                     state = 'completed',
                     duration_secs = CAST((julianday(?1) - julianday(started_at)) * 86400 AS INTEGER)
                 WHERE id = ?2
                 RETURNING duration_secs",
                rusqlite::params![now_str, session_id],
                |row| row.get(0),
            )
            .map_err(|e| CoreError::Internal(format!("작업 세션 종료 실패: {e}")))?;

        debug!(
            "작업 세션 종료: id={}, duration={}초",
            session_id, duration_secs
        );
        Ok(())
    }

    /// 작업 세션 인터럽션 카운트 증가
    pub fn increment_work_session_interruption(&self, session_id: i64) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        conn.execute(
            "UPDATE work_sessions SET interruption_count = interruption_count + 1 WHERE id = ?1",
            rusqlite::params![session_id],
        )
        .map_err(|e| CoreError::Internal(format!("인터럽션 카운트 증가 실패: {e}")))?;

        Ok(())
    }

    /// 작업 세션 deep_work_secs 누적
    pub fn add_deep_work_secs(&self, session_id: i64, secs: u64) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        conn.execute(
            "UPDATE work_sessions SET deep_work_secs = deep_work_secs + ?1 WHERE id = ?2",
            rusqlite::params![secs as i64, session_id],
        )
        .map_err(|e| CoreError::Internal(format!("deep_work_secs 증가 실패: {e}")))?;

        Ok(())
    }

    // --------------------------------------------------------
    // 작업 세션 집계 쿼리
    // --------------------------------------------------------

    /// 날짜 범위 내 앱별 작업시간 집계
    ///
    /// work_sessions 테이블에서 completed 세션의 duration_secs를 앱별로 합산.
    /// 반환: Vec<(app_name, total_duration_secs)>
    pub fn get_app_durations_by_date(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<(String, i64)>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT primary_app, SUM(duration_secs) as total_secs
                 FROM work_sessions
                 WHERE state = 'completed'
                   AND started_at >= ?1 AND started_at < ?2
                 GROUP BY primary_app
                 ORDER BY total_secs DESC",
            )
            .map_err(|e| CoreError::Internal(format!("SQL 준비 실패: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![from, to], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| CoreError::Internal(format!("쿼리 실패: {e}")))?;

        let result: Vec<_> = rows.flatten().collect();

        Ok(result)
    }

    /// 날짜 범위 내 일별 총 활동시간 집계
    ///
    /// work_sessions 테이블에서 completed 세션의 duration_secs를 날짜별로 합산.
    /// 반환: Vec<(date_str, total_active_secs)>
    pub fn get_daily_active_secs(
        &self,
        from: &str,
        to: &str,
    ) -> Result<Vec<(String, i64)>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT DATE(started_at) as day, SUM(duration_secs) as total_secs
                 FROM work_sessions
                 WHERE state = 'completed'
                   AND started_at >= ?1 AND started_at < ?2
                 GROUP BY day
                 ORDER BY day",
            )
            .map_err(|e| CoreError::Internal(format!("SQL 준비 실패: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![from, to], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|e| CoreError::Internal(format!("쿼리 실패: {e}")))?;

        let result: Vec<_> = rows.flatten().collect();

        Ok(result)
    }

    // --------------------------------------------------------
    // 인터럽션
    // --------------------------------------------------------

    /// 인터럽션 기록
    pub fn record_interruption(&self, interruption: &Interruption) -> Result<i64, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        conn.execute(
            "INSERT INTO interruptions (interrupted_at, from_app, from_category, to_app, to_category, snapshot_frame_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                interruption.interrupted_at.to_rfc3339(),
                interruption.from_app,
                format!("{:?}", interruption.from_category),
                interruption.to_app,
                format!("{:?}", interruption.to_category),
                interruption.snapshot_frame_id,
            ],
        )
        .map_err(|e| CoreError::Internal(format!("인터럽션 기록 실패: {e}")))?;

        let id = conn.last_insert_rowid();
        debug!(
            "인터럽션 기록: {} → {}",
            interruption.from_app, interruption.to_app
        );
        Ok(id)
    }

    /// 인터럽션 복귀 기록
    pub fn record_interruption_resume(
        &self,
        interruption_id: i64,
        resumed_to_app: &str,
    ) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        conn.execute(
            "UPDATE interruptions SET resumed_at = ?1, resumed_to_app = ?2 WHERE id = ?3",
            rusqlite::params![Utc::now().to_rfc3339(), resumed_to_app, interruption_id],
        )
        .map_err(|e| CoreError::Internal(format!("인터럽션 복귀 기록 실패: {e}")))?;

        Ok(())
    }

    /// 최근 미복귀 인터럽션 조회
    pub fn get_pending_interruption(&self) -> Result<Option<Interruption>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let result = conn.query_row(
            "SELECT id, interrupted_at, from_app, from_category, to_app, to_category, snapshot_frame_id
             FROM interruptions WHERE resumed_at IS NULL ORDER BY id DESC LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                ))
            },
        );

        match result {
            Ok((
                id,
                interrupted_at_str,
                from_app,
                from_category_str,
                to_app,
                to_category_str,
                snapshot_frame_id,
            )) => {
                let interrupted_at = DateTime::parse_from_rfc3339(&interrupted_at_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                Ok(Some(Interruption {
                    id,
                    interrupted_at,
                    from_app,
                    from_category: Self::parse_app_category(&from_category_str),
                    to_app,
                    to_category: Self::parse_app_category(&to_category_str),
                    snapshot_frame_id,
                    resumed_at: None,
                    resumed_to_app: None,
                    duration_secs: None,
                }))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(CoreError::Internal(format!("인터럽션 조회 실패: {e}"))),
        }
    }

    // --------------------------------------------------------
    // 집중도 메트릭
    // --------------------------------------------------------

    /// 오늘 집중도 메트릭 조회 또는 생성
    pub fn get_or_create_today_focus_metrics(&self) -> Result<FocusMetrics, CoreError> {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        self.get_or_create_focus_metrics(&today)
    }

    /// 특정 날짜 집중도 메트릭 조회 또는 생성
    pub fn get_or_create_focus_metrics(&self, date: &str) -> Result<FocusMetrics, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        // 먼저 존재 여부 확인
        let result = conn.query_row(
            "SELECT total_active_secs, deep_work_secs, communication_secs, context_switches,
                    interruption_count, avg_focus_duration_secs, max_focus_duration_secs, focus_score
             FROM focus_metrics WHERE date = ?1",
            rusqlite::params![date],
            |row| {
                Ok((
                    row.get::<_, u64>(0)?,
                    row.get::<_, u64>(1)?,
                    row.get::<_, u64>(2)?,
                    row.get::<_, u32>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, u64>(5)?,
                    row.get::<_, u64>(6)?,
                    row.get::<_, f32>(7)?,
                ))
            },
        );

        // 날짜 파싱해서 period_start/end 설정
        let (period_start, period_end) = Self::date_to_period_range(date);

        match result {
            Ok((
                total_active_secs,
                deep_work_secs,
                communication_secs,
                context_switches,
                interruption_count,
                avg_focus_duration_secs,
                max_focus_duration_secs,
                focus_score,
            )) => Ok(FocusMetrics {
                period_start,
                period_end,
                total_active_secs,
                deep_work_secs,
                communication_secs,
                context_switches,
                interruption_count,
                avg_focus_duration_secs,
                max_focus_duration_secs,
                focus_score,
            }),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // 없으면 새로 생성
                conn.execute(
                    "INSERT INTO focus_metrics (date) VALUES (?1)",
                    rusqlite::params![date],
                )
                .map_err(|e| CoreError::Internal(format!("집중도 메트릭 생성 실패: {e}")))?;

                Ok(FocusMetrics::new(period_start, period_end))
            }
            Err(e) => Err(CoreError::Internal(format!("집중도 메트릭 조회 실패: {e}"))),
        }
    }

    /// 집중도 메트릭 업데이트
    pub fn update_focus_metrics(
        &self,
        date: &str,
        metrics: &FocusMetrics,
    ) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        conn.execute(
            "UPDATE focus_metrics SET
                total_active_secs = ?1,
                deep_work_secs = ?2,
                communication_secs = ?3,
                context_switches = ?4,
                interruption_count = ?5,
                avg_focus_duration_secs = ?6,
                max_focus_duration_secs = ?7,
                focus_score = ?8,
                updated_at = datetime('now')
             WHERE date = ?9",
            rusqlite::params![
                metrics.total_active_secs as i64,
                metrics.deep_work_secs as i64,
                metrics.communication_secs as i64,
                metrics.context_switches as i64,
                metrics.interruption_count as i64,
                metrics.avg_focus_duration_secs as i64,
                metrics.max_focus_duration_secs as i64,
                metrics.focus_score,
                date,
            ],
        )
        .map_err(|e| CoreError::Internal(format!("집중도 메트릭 업데이트 실패: {e}")))?;

        debug!(
            "집중도 메트릭 업데이트: date={}, score={:.2}",
            date, metrics.focus_score
        );
        Ok(())
    }

    /// 집중도 메트릭 증분 업데이트
    pub fn increment_focus_metrics(
        &self,
        date: &str,
        total_active_secs: u64,
        deep_work_secs: u64,
        communication_secs: u64,
        context_switches: u32,
        interruption_count: u32,
    ) -> Result<(), CoreError> {
        // 먼저 레코드가 존재하는지 확인 (없으면 생성)
        let _ = self.get_or_create_focus_metrics(date)?;

        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        conn.execute(
            "UPDATE focus_metrics SET
                total_active_secs = total_active_secs + ?1,
                deep_work_secs = deep_work_secs + ?2,
                communication_secs = communication_secs + ?3,
                context_switches = context_switches + ?4,
                interruption_count = interruption_count + ?5,
                updated_at = datetime('now')
             WHERE date = ?6",
            rusqlite::params![
                total_active_secs as i64,
                deep_work_secs as i64,
                communication_secs as i64,
                context_switches as i64,
                interruption_count as i64,
                date,
            ],
        )
        .map_err(|e| CoreError::Internal(format!("집중도 메트릭 증분 업데이트 실패: {e}")))?;

        Ok(())
    }

    /// 최근 N일 집중도 메트릭 조회
    pub fn get_recent_focus_metrics(
        &self,
        days: usize,
    ) -> Result<Vec<(String, FocusMetrics)>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT date, total_active_secs, deep_work_secs, communication_secs, context_switches,
                        interruption_count, avg_focus_duration_secs, max_focus_duration_secs, focus_score
                 FROM focus_metrics ORDER BY date DESC LIMIT ?1",
            )
            .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![days as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u64>(1)?,
                    row.get::<_, u64>(2)?,
                    row.get::<_, u64>(3)?,
                    row.get::<_, u32>(4)?,
                    row.get::<_, u32>(5)?,
                    row.get::<_, u64>(6)?,
                    row.get::<_, u64>(7)?,
                    row.get::<_, f32>(8)?,
                ))
            })
            .map_err(|e| CoreError::Internal(format!("쿼리 실행 실패: {e}")))?;

        let mut results = Vec::new();
        for row in rows {
            let (
                date,
                total_active_secs,
                deep_work_secs,
                communication_secs,
                context_switches,
                interruption_count,
                avg_focus_duration_secs,
                max_focus_duration_secs,
                focus_score,
            ) = row.map_err(|e| CoreError::Internal(format!("행 읽기 실패: {e}")))?;

            let (period_start, period_end) = Self::date_to_period_range(&date);

            results.push((
                date,
                FocusMetrics {
                    period_start,
                    period_end,
                    total_active_secs,
                    deep_work_secs,
                    communication_secs,
                    context_switches,
                    interruption_count,
                    avg_focus_duration_secs,
                    max_focus_duration_secs,
                    focus_score,
                },
            ));
        }

        Ok(results)
    }

    pub fn list_work_sessions(
        &self,
        from: &str,
        to: &str,
        limit: usize,
    ) -> Result<Vec<FocusWorkSessionRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, started_at, ended_at, primary_app, category, state,
                        interruption_count, deep_work_secs, duration_secs
                 FROM work_sessions
                 WHERE started_at >= ?1 AND started_at <= ?2
                 ORDER BY started_at DESC
                 LIMIT ?3",
            )
            .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![from, to, limit as i64], |row| {
                Ok(FocusWorkSessionRecord {
                    id: row.get(0)?,
                    started_at: row.get(1)?,
                    ended_at: row.get(2)?,
                    primary_app: row.get(3)?,
                    category: row.get(4)?,
                    state: row.get(5)?,
                    interruption_count: row.get(6)?,
                    deep_work_secs: row.get(7)?,
                    duration_secs: row.get(8)?,
                })
            })
            .map_err(|e| CoreError::Internal(format!("쿼리 실행 실패: {e}")))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| CoreError::Internal(format!("행 읽기 실패: {e}")))?);
        }
        Ok(records)
    }

    pub fn list_interruptions(
        &self,
        from: &str,
        to: &str,
        limit: usize,
    ) -> Result<Vec<FocusInterruptionRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, interrupted_at, from_app, from_category, to_app, to_category,
                        resumed_at, resumed_to_app,
                        CASE WHEN resumed_at IS NOT NULL
                             THEN CAST((julianday(resumed_at) - julianday(interrupted_at)) * 86400 AS INTEGER)
                             ELSE NULL END as duration_secs
                 FROM interruptions
                 WHERE interrupted_at >= ?1 AND interrupted_at <= ?2
                 ORDER BY interrupted_at DESC
                 LIMIT ?3",
            )
            .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![from, to, limit as i64], |row| {
                Ok(FocusInterruptionRecord {
                    id: row.get(0)?,
                    interrupted_at: row.get(1)?,
                    from_app: row.get(2)?,
                    from_category: row.get(3)?,
                    to_app: row.get(4)?,
                    to_category: row.get(5)?,
                    resumed_at: row.get(6)?,
                    resumed_to_app: row.get(7)?,
                    duration_secs: row.get::<_, Option<i64>>(8)?.map(|v| v as u64),
                })
            })
            .map_err(|e| CoreError::Internal(format!("쿼리 실행 실패: {e}")))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| CoreError::Internal(format!("행 읽기 실패: {e}")))?);
        }
        Ok(records)
    }

    pub fn list_recent_local_suggestions(
        &self,
        cutoff: &str,
        limit: usize,
    ) -> Result<Vec<LocalSuggestionRecord>, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, suggestion_type, payload, created_at, shown_at, dismissed_at, acted_at
                 FROM local_suggestions
                 WHERE created_at >= ?1
                 ORDER BY created_at DESC
                 LIMIT ?2",
            )
            .map_err(|e| CoreError::Internal(format!("쿼리 준비 실패: {e}")))?;

        let rows = stmt
            .query_map(rusqlite::params![cutoff, limit as i64], |row| {
                let payload_str: String = row.get(2)?;
                let payload: serde_json::Value =
                    serde_json::from_str(&payload_str).unwrap_or(serde_json::json!({}));

                Ok(LocalSuggestionRecord {
                    id: row.get(0)?,
                    suggestion_type: row.get(1)?,
                    payload,
                    created_at: row.get(3)?,
                    shown_at: row.get(4)?,
                    dismissed_at: row.get(5)?,
                    acted_at: row.get(6)?,
                })
            })
            .map_err(|e| CoreError::Internal(format!("쿼리 실행 실패: {e}")))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row.map_err(|e| CoreError::Internal(format!("행 읽기 실패: {e}")))?);
        }
        Ok(records)
    }

    // --------------------------------------------------------
    // 로컬 제안
    // --------------------------------------------------------

    /// 로컬 제안 저장
    pub fn save_local_suggestion(&self, suggestion: &LocalSuggestion) -> Result<i64, CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        let (suggestion_type, payload) = Self::serialize_suggestion(suggestion);

        conn.execute(
            "INSERT INTO local_suggestions (suggestion_type, payload) VALUES (?1, ?2)",
            rusqlite::params![suggestion_type, payload],
        )
        .map_err(|e| CoreError::Internal(format!("로컬 제안 저장 실패: {e}")))?;

        let id = conn.last_insert_rowid();
        debug!("로컬 제안 저장: id={}, type={}", id, suggestion_type);
        Ok(id)
    }

    /// 로컬 제안 표시 완료 기록
    pub fn mark_suggestion_shown(&self, suggestion_id: i64) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        conn.execute(
            "UPDATE local_suggestions SET shown_at = datetime('now') WHERE id = ?1",
            rusqlite::params![suggestion_id],
        )
        .map_err(|e| CoreError::Internal(format!("제안 표시 기록 실패: {e}")))?;

        Ok(())
    }

    /// 로컬 제안 무시 기록
    pub fn mark_suggestion_dismissed(&self, suggestion_id: i64) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        conn.execute(
            "UPDATE local_suggestions SET dismissed_at = datetime('now') WHERE id = ?1",
            rusqlite::params![suggestion_id],
        )
        .map_err(|e| CoreError::Internal(format!("제안 무시 기록 실패: {e}")))?;

        Ok(())
    }

    /// 로컬 제안 실행 기록
    pub fn mark_suggestion_acted(&self, suggestion_id: i64) -> Result<(), CoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("잠금 획득 실패: {e}")))?;

        conn.execute(
            "UPDATE local_suggestions SET acted_at = datetime('now') WHERE id = ?1",
            rusqlite::params![suggestion_id],
        )
        .map_err(|e| CoreError::Internal(format!("제안 실행 기록 실패: {e}")))?;

        Ok(())
    }

    // --------------------------------------------------------
    // 유틸리티
    // --------------------------------------------------------

    /// 날짜 문자열(YYYY-MM-DD)을 해당 날짜의 시작과 끝 DateTime으로 변환
    pub(super) fn date_to_period_range(date: &str) -> (DateTime<Utc>, DateTime<Utc>) {
        use chrono::NaiveDate;

        if let Ok(naive_date) = NaiveDate::parse_from_str(date, "%Y-%m-%d") {
            let start = naive_date
                .and_hms_opt(0, 0, 0)
                .map(|dt| dt.and_utc())
                .unwrap_or_else(Utc::now);
            let end = naive_date
                .and_hms_opt(23, 59, 59)
                .map(|dt| dt.and_utc())
                .unwrap_or_else(Utc::now);
            (start, end)
        } else {
            // 파싱 실패 시 현재 날짜 사용
            let now = Utc::now();
            let start = now
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .map(|dt| dt.and_utc())
                .unwrap_or(now);
            let end = now
                .date_naive()
                .and_hms_opt(23, 59, 59)
                .map(|dt| dt.and_utc())
                .unwrap_or(now);
            (start, end)
        }
    }

    /// 카테고리 문자열 파싱
    pub(crate) fn parse_app_category(s: &str) -> AppCategory {
        match s {
            "Communication" => AppCategory::Communication,
            "Development" => AppCategory::Development,
            "Documentation" => AppCategory::Documentation,
            "Browser" => AppCategory::Browser,
            "Design" => AppCategory::Design,
            "Media" => AppCategory::Media,
            "System" => AppCategory::System,
            _ => AppCategory::Other,
        }
    }

    /// 제안 직렬화
    fn serialize_suggestion(suggestion: &LocalSuggestion) -> (String, String) {
        match suggestion {
            LocalSuggestion::NeedFocusTime {
                communication_ratio,
                suggested_focus_mins,
            } => (
                "NeedFocusTime".to_string(),
                serde_json::json!({
                    "communication_ratio": communication_ratio,
                    "suggested_focus_mins": suggested_focus_mins,
                })
                .to_string(),
            ),
            LocalSuggestion::TakeBreak {
                continuous_work_mins,
            } => (
                "TakeBreak".to_string(),
                serde_json::json!({
                    "continuous_work_mins": continuous_work_mins,
                })
                .to_string(),
            ),
            LocalSuggestion::RestoreContext {
                interrupted_app,
                interrupted_at,
                snapshot_frame_id,
            } => (
                "RestoreContext".to_string(),
                serde_json::json!({
                    "interrupted_app": interrupted_app,
                    "interrupted_at": interrupted_at.to_rfc3339(),
                    "snapshot_frame_id": snapshot_frame_id,
                })
                .to_string(),
            ),
            LocalSuggestion::PatternDetected {
                pattern_description,
                confidence,
            } => (
                "PatternDetected".to_string(),
                serde_json::json!({
                    "pattern_description": pattern_description,
                    "confidence": confidence,
                })
                .to_string(),
            ),
            LocalSuggestion::ExcessiveCommunication {
                today_communication_mins,
                avg_communication_mins,
            } => (
                "ExcessiveCommunication".to_string(),
                serde_json::json!({
                    "today_communication_mins": today_communication_mins,
                    "avg_communication_mins": avg_communication_mins,
                })
                .to_string(),
            ),
        }
    }
}
