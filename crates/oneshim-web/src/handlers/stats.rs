use axum::extract::{Query, State};
use axum::Json;
use chrono::{Datelike, Duration, Timelike, Utc};
use oneshim_api_contracts::stats::{
    AppUsageEntry, AppUsageResponse, DailySummaryResponse, DateQuery, HeatmapCell, HeatmapQuery,
    HeatmapResponse,
};
use std::collections::HashMap;

use crate::error::ApiError;
use crate::AppState;

/// GET /api/stats/summary?date=YYYY-MM-DD
pub async fn get_summary(
    State(state): State<AppState>,
    Query(params): Query<DateQuery>,
) -> Result<Json<DailySummaryResponse>, ApiError> {
    let date_str = params
        .date
        .unwrap_or_else(|| Utc::now().format("%Y-%m-%d").to_string());

    let from = chrono::NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
        .map_err(|_| ApiError::BadRequest(format!("Invalid date format: {date_str}")))?
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();

    let to = from + Duration::days(1);

    let metrics = state.storage.get_metrics(from, to, 10000).await?;

    let (cpu_sum, memory_sum, count) = metrics.iter().fold((0.0f64, 0.0f64, 0u64), |acc, m| {
        let mem_percent = if m.memory_total > 0 {
            (m.memory_used as f64 / m.memory_total as f64) * 100.0
        } else {
            0.0
        };
        (acc.0 + m.cpu_usage as f64, acc.1 + mem_percent, acc.2 + 1)
    });

    let cpu_avg = if count > 0 {
        cpu_sum / count as f64
    } else {
        0.0
    };
    let memory_avg_percent = if count > 0 {
        memory_sum / count as f64
    } else {
        0.0
    };

    let idle_periods = state.storage.get_idle_periods(from, to).await?;
    let total_idle_secs: u64 = idle_periods.iter().filter_map(|p| p.duration_secs).sum();

    let events = state.storage.get_events(from, to, 100000).await?;
    let events_logged = events.len() as u64;

    let frames = state.storage.get_frames(from, to, 100000)?;
    let frames_captured = frames.len() as u64;

    let mut app_stats: HashMap<String, (u64, u64)> = HashMap::new();

    for event in &events {
        if let Some(app_name) = match event {
            oneshim_core::models::event::Event::User(e) => Some(e.app_name.clone()),
            oneshim_core::models::event::Event::Context(e) => Some(e.app_name.clone()),
            _ => None,
        } {
            let entry = app_stats.entry(app_name).or_insert((0, 0));
            entry.0 += 1;
        }
    }

    for frame in &frames {
        let entry = app_stats.entry(frame.app_name.clone()).or_insert((0, 0));
        entry.1 += 1;
    }

    let session_app_durations: HashMap<String, i64> = {
        let from_rfc = from.to_rfc3339();
        let to_rfc = to.to_rfc3339();
        match state.storage.get_app_durations_by_date(&from_rfc, &to_rfc) {
            Ok(durations) => durations.into_iter().collect(),
            Err(_) => HashMap::new(), // no session data
        }
    };

    let mut top_apps: Vec<AppUsageEntry> = app_stats
        .into_iter()
        .map(|(name, (event_count, frame_count))| {
            let duration_secs = session_app_durations
                .get(&name)
                .map(|&d| d as u64)
                .unwrap_or(event_count * 5);
            AppUsageEntry {
                name,
                duration_secs,
                event_count,
                frame_count,
            }
        })
        .collect();

    top_apps.sort_by(|a, b| b.duration_secs.cmp(&a.duration_secs));
    top_apps.truncate(10);

    let total_active_secs = {
        let from_rfc = from.to_rfc3339();
        let to_rfc = to.to_rfc3339();
        match state.storage.get_daily_active_secs(&from_rfc, &to_rfc) {
            Ok(daily) if !daily.is_empty() => daily.iter().map(|(_, s)| *s as u64).sum(),
            _ => events_logged * 5, // Fallback
        }
    };

    Ok(Json(DailySummaryResponse {
        date: date_str,
        total_active_secs,
        total_idle_secs,
        top_apps,
        cpu_avg,
        memory_avg_percent,
        frames_captured,
        events_logged,
    }))
}

/// GET /api/stats/apps?date=YYYY-MM-DD
pub async fn get_app_usage(
    State(state): State<AppState>,
    Query(params): Query<DateQuery>,
) -> Result<Json<AppUsageResponse>, ApiError> {
    let date_str = params
        .date
        .unwrap_or_else(|| Utc::now().format("%Y-%m-%d").to_string());

    let from = chrono::NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
        .map_err(|_| ApiError::BadRequest(format!("Invalid date format: {date_str}")))?
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();

    let to = from + Duration::days(1);

    let events = state.storage.get_events(from, to, 100000).await?;
    let frames = state.storage.get_frames(from, to, 100000)?;

    let mut app_stats: HashMap<String, (u64, u64)> = HashMap::new();

    for event in &events {
        if let Some(app_name) = match event {
            oneshim_core::models::event::Event::User(e) => Some(e.app_name.clone()),
            oneshim_core::models::event::Event::Context(e) => Some(e.app_name.clone()),
            _ => None,
        } {
            let entry = app_stats.entry(app_name).or_insert((0, 0));
            entry.0 += 1;
        }
    }

    for frame in &frames {
        let entry = app_stats.entry(frame.app_name.clone()).or_insert((0, 0));
        entry.1 += 1;
    }

    let session_app_durations: HashMap<String, i64> = {
        let from_rfc = from.to_rfc3339();
        let to_rfc = to.to_rfc3339();
        match state.storage.get_app_durations_by_date(&from_rfc, &to_rfc) {
            Ok(durations) => durations.into_iter().collect(),
            Err(_) => HashMap::new(),
        }
    };

    let mut apps: Vec<AppUsageEntry> = app_stats
        .into_iter()
        .map(|(name, (event_count, frame_count))| {
            let duration_secs = session_app_durations
                .get(&name)
                .map(|&d| d as u64)
                .unwrap_or(event_count * 5);
            AppUsageEntry {
                name,
                duration_secs,
                event_count,
                frame_count,
            }
        })
        .collect();

    apps.sort_by(|a, b| b.duration_secs.cmp(&a.duration_secs));

    Ok(Json(AppUsageResponse {
        date: date_str,
        apps,
    }))
}

/// GET /api/stats/heatmap?days=7
pub async fn get_heatmap(
    State(state): State<AppState>,
    Query(params): Query<HeatmapQuery>,
) -> Result<Json<HeatmapResponse>, ApiError> {
    let days = params.days.unwrap_or(7).min(30) as i64;

    let to = Utc::now();
    let from = to - Duration::days(days);

    let events = state.storage.get_events(from, to, 100000).await?;
    let frames = state.storage.get_frames(from, to, 100000)?;

    let mut grid: [[u32; 24]; 7] = [[0; 24]; 7];

    for event in &events {
        let ts = match event {
            oneshim_core::models::event::Event::User(e) => e.timestamp,
            oneshim_core::models::event::Event::Context(e) => e.timestamp,
            oneshim_core::models::event::Event::System(e) => e.timestamp,
            oneshim_core::models::event::Event::Input(e) => e.timestamp,
            oneshim_core::models::event::Event::Process(e) => e.timestamp,
            oneshim_core::models::event::Event::Window(e) => e.timestamp,
            oneshim_core::models::event::Event::Clipboard(e) => e.timestamp,
            oneshim_core::models::event::Event::FileAccess(e) => e.timestamp,
        };
        let day = (ts.weekday().num_days_from_monday()) as usize;
        let hour = ts.hour() as usize;
        if day < 7 && hour < 24 {
            grid[day][hour] += 1;
        }
    }

    for frame in &frames {
        if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&frame.timestamp) {
            let ts_utc = ts.with_timezone(&Utc);
            let day = (ts_utc.weekday().num_days_from_monday()) as usize;
            let hour = ts_utc.hour() as usize;
            if day < 7 && hour < 24 {
                grid[day][hour] += 1;
            }
        }
    }

    let mut cells = Vec::with_capacity(7 * 24);
    let mut max_value = 0u32;

    for (day, hours) in grid.iter().enumerate() {
        for (hour, &value) in hours.iter().enumerate() {
            cells.push(HeatmapCell {
                day: day as u8,
                hour: hour as u8,
                value,
            });
            if value > max_value {
                max_value = value;
            }
        }
    }

    Ok(Json(HeatmapResponse {
        from_date: from.format("%Y-%m-%d").to_string(),
        to_date: to.format("%Y-%m-%d").to_string(),
        cells,
        max_value,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_response_serializes() {
        let response = DailySummaryResponse {
            date: "2024-01-30".to_string(),
            total_active_secs: 28800,
            total_idle_secs: 3600,
            top_apps: vec![AppUsageEntry {
                name: "VS Code".to_string(),
                duration_secs: 14400,
                event_count: 2880,
                frame_count: 100,
            }],
            cpu_avg: 35.2,
            memory_avg_percent: 68.5,
            frames_captured: 1234,
            events_logged: 567,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("VS Code"));
    }

    #[test]
    fn heatmap_response_serializes() {
        let response = HeatmapResponse {
            from_date: "2024-01-23".to_string(),
            to_date: "2024-01-30".to_string(),
            cells: vec![
                HeatmapCell {
                    day: 0,
                    hour: 9,
                    value: 42,
                },
                HeatmapCell {
                    day: 0,
                    hour: 10,
                    value: 58,
                },
            ],
            max_value: 58,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"day\":0"));
        assert!(json.contains("\"hour\":9"));
        assert!(json.contains("\"value\":42"));
        assert!(json.contains("\"max_value\":58"));
    }
}
