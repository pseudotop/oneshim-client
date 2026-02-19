//! 리포트 API 핸들러.
//!
//! 주간/월간 활동 리포트 생성 기능 제공.

use axum::extract::{Query, State};
use axum::Json;
use chrono::{DateTime, Duration, NaiveDate, Timelike, Utc};
use oneshim_core::ports::storage::{MetricsStorage, StorageService};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::ApiError;
use crate::AppState;

/// 리포트 기간 타입
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ReportPeriod {
    #[default]
    Week,
    Month,
    Custom,
}

/// 리포트 쿼리 파라미터
#[derive(Debug, Deserialize)]
pub struct ReportQuery {
    /// 리포트 기간 (week, month, custom)
    #[serde(default)]
    pub period: ReportPeriod,
    /// 시작 날짜 (YYYY-MM-DD, custom 기간용)
    pub from: Option<String>,
    /// 종료 날짜 (YYYY-MM-DD, custom 기간용)
    pub to: Option<String>,
}

/// 일별 통계
#[derive(Debug, Serialize, Clone)]
pub struct DailyStat {
    /// 날짜 (YYYY-MM-DD)
    pub date: String,
    /// 활동 시간 (초)
    pub active_secs: u64,
    /// 유휴 시간 (초)
    pub idle_secs: u64,
    /// 캡처 수
    pub captures: u64,
    /// 이벤트 수
    pub events: u64,
    /// 평균 CPU (%)
    pub cpu_avg: f64,
    /// 평균 메모리 (%)
    pub memory_avg: f64,
}

/// 앱별 사용 통계
#[derive(Debug, Serialize, Clone)]
pub struct AppStat {
    /// 앱 이름
    pub name: String,
    /// 총 사용 시간 (초)
    pub duration_secs: u64,
    /// 이벤트 수
    pub events: u64,
    /// 캡처 수
    pub captures: u64,
    /// 비율 (%)
    pub percentage: f64,
}

/// 시간대별 활동 통계
#[derive(Debug, Serialize, Clone)]
pub struct HourlyActivity {
    /// 시간 (0-23)
    pub hour: u8,
    /// 활동량
    pub activity: u64,
}

/// 생산성 지표
#[derive(Debug, Serialize, Clone)]
pub struct ProductivityMetrics {
    /// 생산성 점수 (0-100)
    pub score: f64,
    /// 활동/전체 비율 (%)
    pub active_ratio: f64,
    /// 가장 생산적인 시간대 (시작 시간)
    pub peak_hour: u8,
    /// 가장 많이 사용한 앱
    pub top_app: String,
    /// 전주 대비 변화 (%)
    pub trend: f64,
}

/// 리포트 응답
#[derive(Debug, Serialize)]
pub struct ReportResponse {
    /// 리포트 제목
    pub title: String,
    /// 시작 날짜
    pub from_date: String,
    /// 종료 날짜
    pub to_date: String,
    /// 기간 (일수)
    pub days: u32,
    /// 총 활동 시간 (초)
    pub total_active_secs: u64,
    /// 총 유휴 시간 (초)
    pub total_idle_secs: u64,
    /// 총 캡처 수
    pub total_captures: u64,
    /// 총 이벤트 수
    pub total_events: u64,
    /// 평균 CPU (%)
    pub avg_cpu: f64,
    /// 평균 메모리 (%)
    pub avg_memory: f64,
    /// 일별 통계
    pub daily_stats: Vec<DailyStat>,
    /// 앱별 사용 통계 (상위 10개)
    pub app_stats: Vec<AppStat>,
    /// 시간대별 활동
    pub hourly_activity: Vec<HourlyActivity>,
    /// 생산성 지표
    pub productivity: ProductivityMetrics,
}

/// GET /api/reports - 활동 리포트 생성
pub async fn generate_report(
    State(state): State<AppState>,
    Query(params): Query<ReportQuery>,
) -> Result<Json<ReportResponse>, ApiError> {
    let now = Utc::now();

    // 날짜 범위 계산
    let (from, to, title) = match params.period {
        ReportPeriod::Week => {
            let to = now;
            let from = to - Duration::days(7);
            (from, to, "주간 활동 리포트".to_string())
        }
        ReportPeriod::Month => {
            let to = now;
            let from = to - Duration::days(30);
            (from, to, "월간 활동 리포트".to_string())
        }
        ReportPeriod::Custom => {
            let from_str = params
                .from
                .ok_or_else(|| ApiError::BadRequest("from 날짜가 필요합니다".to_string()))?;
            let to_str = params
                .to
                .ok_or_else(|| ApiError::BadRequest("to 날짜가 필요합니다".to_string()))?;

            let from_date = NaiveDate::parse_from_str(&from_str, "%Y-%m-%d")
                .map_err(|_| ApiError::BadRequest(format!("잘못된 시작 날짜: {from_str}")))?;
            let to_date = NaiveDate::parse_from_str(&to_str, "%Y-%m-%d")
                .map_err(|_| ApiError::BadRequest(format!("잘못된 종료 날짜: {to_str}")))?;

            let from = from_date
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| ApiError::Internal("시간 변환 실패: 00:00:00".to_string()))?
                .and_utc();
            let to = to_date
                .and_hms_opt(23, 59, 59)
                .ok_or_else(|| ApiError::Internal("시간 변환 실패: 23:59:59".to_string()))?
                .and_utc();

            (from, to, format!("활동 리포트 ({from_str} ~ {to_str})"))
        }
    };

    let days = ((to - from).num_days() as u32).max(1);

    // 데이터 조회
    let metrics = state.storage.get_metrics(from, to, 100000).await?;
    let events = state.storage.get_events(from, to, 100000).await?;
    let frames = state.storage.get_frames(from, to, 100000)?;
    let idle_periods = state.storage.get_idle_periods(from, to).await?;

    // 총계 계산
    let total_captures = frames.len() as u64;
    let total_events = events.len() as u64;
    let total_idle_secs: u64 = idle_periods.iter().filter_map(|p| p.duration_secs).sum();

    // CPU/메모리 평균
    let (cpu_sum, mem_sum, met_count) = metrics.iter().fold((0.0f64, 0.0f64, 0u64), |acc, m| {
        let mem_pct = if m.memory_total > 0 {
            (m.memory_used as f64 / m.memory_total as f64) * 100.0
        } else {
            0.0
        };
        (acc.0 + m.cpu_usage as f64, acc.1 + mem_pct, acc.2 + 1)
    });
    let avg_cpu = if met_count > 0 {
        cpu_sum / met_count as f64
    } else {
        0.0
    };
    let avg_memory = if met_count > 0 {
        mem_sum / met_count as f64
    } else {
        0.0
    };

    // 일별 통계 집계
    let mut daily_map: HashMap<String, DailyStat> = HashMap::new();
    let mut current = from;
    while current < to {
        let date_str = current.format("%Y-%m-%d").to_string();
        daily_map.insert(
            date_str.clone(),
            DailyStat {
                date: date_str,
                active_secs: 0,
                idle_secs: 0,
                captures: 0,
                events: 0,
                cpu_avg: 0.0,
                memory_avg: 0.0,
            },
        );
        current += Duration::days(1);
    }

    // 이벤트별 일별 집계
    for event in &events {
        let ts = match event {
            oneshim_core::models::event::Event::User(e) => e.timestamp,
            oneshim_core::models::event::Event::Context(e) => e.timestamp,
            oneshim_core::models::event::Event::System(e) => e.timestamp,
            oneshim_core::models::event::Event::Input(e) => e.timestamp,
            oneshim_core::models::event::Event::Process(e) => e.timestamp,
            oneshim_core::models::event::Event::Window(e) => e.timestamp,
        };
        let date_str = ts.format("%Y-%m-%d").to_string();
        if let Some(stat) = daily_map.get_mut(&date_str) {
            stat.events += 1;
        }
    }

    // work_sessions 기반 일별 실제 활동시간 적용 (Fallback: events * 5)
    {
        let from_rfc = from.to_rfc3339();
        let to_rfc = to.to_rfc3339();
        if let Ok(daily_active) = state.storage.get_daily_active_secs(&from_rfc, &to_rfc) {
            for (day, secs) in &daily_active {
                if let Some(stat) = daily_map.get_mut(day) {
                    stat.active_secs = *secs as u64;
                }
            }
        }
        // Fallback: work_sessions 데이터가 없는 날은 events * 5
        for stat in daily_map.values_mut() {
            if stat.active_secs == 0 && stat.events > 0 {
                stat.active_secs = stat.events * 5;
            }
        }
    }

    // 프레임별 일별 집계
    for frame in &frames {
        if let Ok(ts) = DateTime::parse_from_rfc3339(&frame.timestamp) {
            let date_str = ts.format("%Y-%m-%d").to_string();
            if let Some(stat) = daily_map.get_mut(&date_str) {
                stat.captures += 1;
            }
        }
    }

    // 유휴 기간 일별 집계
    for idle in &idle_periods {
        let date_str = idle.start_time.format("%Y-%m-%d").to_string();
        if let Some(stat) = daily_map.get_mut(&date_str) {
            stat.idle_secs += idle.duration_secs.unwrap_or(0);
        }
    }

    // 메트릭 일별 집계
    let mut daily_metrics: HashMap<String, (f64, f64, u64)> = HashMap::new();
    for m in &metrics {
        let date_str = m.timestamp.format("%Y-%m-%d").to_string();
        let entry = daily_metrics.entry(date_str).or_insert((0.0, 0.0, 0));
        let mem_pct = if m.memory_total > 0 {
            (m.memory_used as f64 / m.memory_total as f64) * 100.0
        } else {
            0.0
        };
        entry.0 += m.cpu_usage as f64;
        entry.1 += mem_pct;
        entry.2 += 1;
    }

    for (date, (cpu, mem, count)) in daily_metrics {
        if let Some(stat) = daily_map.get_mut(&date) {
            if count > 0 {
                stat.cpu_avg = cpu / count as f64;
                stat.memory_avg = mem / count as f64;
            }
        }
    }

    let mut daily_stats: Vec<DailyStat> = daily_map.into_values().collect();
    daily_stats.sort_by(|a, b| a.date.cmp(&b.date));

    // 앱별 사용 통계
    let mut app_map: HashMap<String, (u64, u64)> = HashMap::new(); // (events, captures)

    for event in &events {
        if let Some(app_name) = match event {
            oneshim_core::models::event::Event::User(e) => Some(e.app_name.clone()),
            oneshim_core::models::event::Event::Context(e) => Some(e.app_name.clone()),
            _ => None,
        } {
            let entry = app_map.entry(app_name).or_insert((0, 0));
            entry.0 += 1;
        }
    }

    for frame in &frames {
        let entry = app_map.entry(frame.app_name.clone()).or_insert((0, 0));
        entry.1 += 1;
    }

    // work_sessions 기반 앱별 실제 사용시간 (Fallback: event_count * 5)
    let session_app_durations: HashMap<String, i64> = {
        let from_rfc = from.to_rfc3339();
        let to_rfc = to.to_rfc3339();
        match state.storage.get_app_durations_by_date(&from_rfc, &to_rfc) {
            Ok(durations) => durations.into_iter().collect(),
            Err(_) => HashMap::new(),
        }
    };

    let mut app_stats: Vec<AppStat> = app_map
        .into_iter()
        .map(|(name, (events, captures))| {
            let duration_secs = session_app_durations
                .get(&name)
                .map(|&d| d as u64)
                .unwrap_or(events * 5);
            AppStat {
                name,
                duration_secs,
                events,
                captures,
                percentage: 0.0, // 아래에서 재계산
            }
        })
        .collect();

    let total_app_duration: u64 = app_stats.iter().map(|a| a.duration_secs).sum();
    for stat in &mut app_stats {
        stat.percentage = if total_app_duration > 0 {
            (stat.duration_secs as f64 / total_app_duration as f64) * 100.0
        } else {
            0.0
        };
    }
    app_stats.sort_by(|a, b| b.duration_secs.cmp(&a.duration_secs));
    app_stats.truncate(10);

    // 시간대별 활동
    let mut hourly: [u64; 24] = [0; 24];
    for event in &events {
        let ts = match event {
            oneshim_core::models::event::Event::User(e) => e.timestamp,
            oneshim_core::models::event::Event::Context(e) => e.timestamp,
            oneshim_core::models::event::Event::System(e) => e.timestamp,
            oneshim_core::models::event::Event::Input(e) => e.timestamp,
            oneshim_core::models::event::Event::Process(e) => e.timestamp,
            oneshim_core::models::event::Event::Window(e) => e.timestamp,
        };
        let hour = ts.hour() as usize;
        hourly[hour] += 1;
    }
    for frame in &frames {
        if let Ok(ts) = DateTime::parse_from_rfc3339(&frame.timestamp) {
            let hour = ts.hour() as usize;
            hourly[hour] += 1;
        }
    }

    let hourly_activity: Vec<HourlyActivity> = hourly
        .iter()
        .enumerate()
        .map(|(hour, &activity)| HourlyActivity {
            hour: hour as u8,
            activity,
        })
        .collect();

    // 생산성 지표 계산 — work_sessions 기반 총 활동 시간 (Fallback: events * 5)
    let total_active_secs: u64 = {
        let sum: u64 = daily_stats.iter().map(|s| s.active_secs).sum();
        if sum > 0 {
            sum
        } else {
            total_events * 5
        }
    };
    let total_time = total_active_secs + total_idle_secs;
    let active_ratio = if total_time > 0 {
        (total_active_secs as f64 / total_time as f64) * 100.0
    } else {
        0.0
    };

    let peak_hour = hourly
        .iter()
        .enumerate()
        .max_by_key(|(_, &v)| v)
        .map(|(h, _)| h as u8)
        .unwrap_or(9);

    let top_app = app_stats
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_default();

    // 전주 대비 변화 (간단히 일별 이벤트 추세로 계산)
    let trend = if daily_stats.len() >= 2 {
        let first_half: u64 = daily_stats
            .iter()
            .take(daily_stats.len() / 2)
            .map(|s| s.events)
            .sum();
        let second_half: u64 = daily_stats
            .iter()
            .skip(daily_stats.len() / 2)
            .map(|s| s.events)
            .sum();
        if first_half > 0 {
            ((second_half as f64 - first_half as f64) / first_half as f64) * 100.0
        } else {
            0.0
        }
    } else {
        0.0
    };

    // 생산성 점수 (활동 비율 기반 + 규칙적인 활동 보너스)
    let regularity_bonus =
        if daily_stats.iter().filter(|s| s.events > 0).count() >= (days as usize * 7 / 10) {
            10.0
        } else {
            0.0
        };
    let score = (active_ratio * 0.9 + regularity_bonus).min(100.0);

    let productivity = ProductivityMetrics {
        score,
        active_ratio,
        peak_hour,
        top_app,
        trend,
    };

    Ok(Json(ReportResponse {
        title,
        from_date: from.format("%Y-%m-%d").to_string(),
        to_date: to.format("%Y-%m-%d").to_string(),
        days,
        total_active_secs,
        total_idle_secs,
        total_captures,
        total_events,
        avg_cpu,
        avg_memory,
        daily_stats,
        app_stats,
        hourly_activity,
        productivity,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_period_deserializes() {
        let json = r#""week""#;
        let period: ReportPeriod = serde_json::from_str(json).unwrap();
        assert_eq!(period, ReportPeriod::Week);

        let json = r#""month""#;
        let period: ReportPeriod = serde_json::from_str(json).unwrap();
        assert_eq!(period, ReportPeriod::Month);
    }

    #[test]
    fn report_response_serializes() {
        let response = ReportResponse {
            title: "주간 리포트".to_string(),
            from_date: "2024-01-23".to_string(),
            to_date: "2024-01-30".to_string(),
            days: 7,
            total_active_secs: 28800,
            total_idle_secs: 3600,
            total_captures: 100,
            total_events: 500,
            avg_cpu: 35.5,
            avg_memory: 68.2,
            daily_stats: vec![],
            app_stats: vec![],
            hourly_activity: vec![],
            productivity: ProductivityMetrics {
                score: 85.0,
                active_ratio: 80.0,
                peak_hour: 10,
                top_app: "VS Code".to_string(),
                trend: 5.5,
            },
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("주간 리포트"));
        assert!(json.contains("productivity"));
    }

    #[test]
    fn daily_stat_serializes() {
        let stat = DailyStat {
            date: "2024-01-30".to_string(),
            active_secs: 14400,
            idle_secs: 1800,
            captures: 50,
            events: 200,
            cpu_avg: 40.0,
            memory_avg: 70.0,
        };
        let json = serde_json::to_string(&stat).unwrap();
        assert!(json.contains("2024-01-30"));
        assert!(json.contains("14400"));
    }

    #[test]
    fn app_stat_serializes() {
        let stat = AppStat {
            name: "VS Code".to_string(),
            duration_secs: 7200,
            events: 150,
            captures: 30,
            percentage: 45.5,
        };
        let json = serde_json::to_string(&stat).unwrap();
        assert!(json.contains("VS Code"));
        assert!(json.contains("45.5"));
    }
}
