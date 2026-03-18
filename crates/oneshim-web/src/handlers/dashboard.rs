use axum::extract::{Query, State};
use axum::Json;
use chrono::{NaiveDate, Utc};
use serde::Deserialize;
use tracing::{debug, warn};

use oneshim_core::models::daily_digest::{
    ContentBrief, DailyDigest, DailyStatistics, DayComparison, TimelineEntry,
};
use oneshim_core::models::storage_records::SegmentSummaryRecord;
use oneshim_core::models::tiered_memory::WorkType;

use crate::error::ApiError;
use crate::AppState;

/// Query parameters for the dashboard day endpoint.
#[derive(Debug, Deserialize)]
pub struct DashboardDayQuery {
    /// Date in YYYY-MM-DD format. Defaults to today.
    pub date: Option<String>,
}

/// GET /api/dashboard/day?date=YYYY-MM-DD — daily timetable + insight.
pub async fn get_dashboard_day(
    State(state): State<AppState>,
    Query(params): Query<DashboardDayQuery>,
) -> Result<Json<DailyDigest>, ApiError> {
    let date_str = params
        .date
        .unwrap_or_else(|| Utc::now().format("%Y-%m-%d").to_string());
    debug!("GET /api/dashboard/day date={}", date_str);

    // Validate date format
    let _date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
        .map_err(|e| ApiError::BadRequest(format!("Invalid date format: {e}")))?;

    // 1. Check cache
    if let Some(cached) = state
        .storage
        .get_daily_digest(&date_str)
        .map_err(|e| ApiError::Internal(format!("Failed to get daily digest: {e}")))?
    {
        return Ok(Json(cached));
    }

    // 2. Generate from segments
    let segment_records = state
        .storage
        .get_segments_for_date(&date_str)
        .map_err(|e| ApiError::Internal(format!("Failed to get segments: {e}")))?;

    let digest = build_daily_digest_from_records(&segment_records, _date, &state);

    // 3. Cache the result
    if let Err(e) = state.storage.save_daily_digest(&digest) {
        warn!("Failed to cache daily digest: {e}");
    }

    Ok(Json(digest))
}

/// Build a DailyDigest from raw SegmentSummaryRecords.
///
/// This is a lightweight version that does not require the analysis crate.
/// LLM insight generation is left to the scheduler (Task 15).
fn build_daily_digest_from_records(
    records: &[SegmentSummaryRecord],
    date: NaiveDate,
    state: &AppState,
) -> DailyDigest {
    let timeline: Vec<TimelineEntry> = records
        .iter()
        .map(|r| {
            let regime_label = r
                .regime_id
                .clone()
                .unwrap_or_else(|| r.dominant_category.clone());
            let regime_color = regime_color(&regime_label).to_string();
            let content_summary = parse_content_briefs(&r.content_activities_json);
            let dominant_app = parse_dominant_app(&r.app_breakdown);

            TimelineEntry {
                segment_id: r.segment_id.clone(),
                start_time: r.start_time.parse().unwrap_or_else(|_| Utc::now()),
                end_time: r.end_time.parse().unwrap_or_else(|_| Utc::now()),
                duration_mins: (r.duration_secs / 60) as u32,
                regime_label,
                regime_color,
                dominant_app,
                content_summary,
                annotation: None,
            }
        })
        .collect();

    let statistics = compute_statistics(records, state, &date);

    DailyDigest {
        date,
        insight: None, // Filled by daily insight generator via scheduler
        timeline,
        statistics,
        generated_at: Utc::now(),
    }
}

/// Map a regime label to a display color hex.
fn regime_color(label: &str) -> &'static str {
    if label.contains("Deep Focus") || label.contains("Development") {
        "#3B82F6"
    } else if label.contains("Communication") {
        "#F59E0B"
    } else if label.contains("Research") {
        "#10B981"
    } else if label.contains("Meeting") {
        "#8B5CF6"
    } else if label.contains("Idle") {
        "#E5E7EB"
    } else {
        "#6B7280"
    }
}

/// Parse content activities JSON into ContentBrief list (top 3).
fn parse_content_briefs(json_str: &str) -> Vec<ContentBrief> {
    #[derive(serde::Deserialize)]
    struct RawActivity {
        content_label: Option<String>,
        duration_secs: Option<u64>,
        work_type: Option<String>,
    }

    let activities: Vec<RawActivity> = serde_json::from_str(json_str).unwrap_or_default();
    let mut sorted = activities;
    sorted.sort_by(|a, b| {
        b.duration_secs
            .unwrap_or(0)
            .cmp(&a.duration_secs.unwrap_or(0))
    });

    sorted
        .into_iter()
        .take(3)
        .map(|a| ContentBrief {
            content: a.content_label.unwrap_or_default(),
            work_type: parse_work_type(a.work_type.as_deref()),
            mins: (a.duration_secs.unwrap_or(0) / 60) as u32,
        })
        .collect()
}

/// Parse work type string to enum.
fn parse_work_type(s: Option<&str>) -> WorkType {
    match s {
        Some("ACTIVE_CODING") => WorkType::ActiveCoding,
        Some("CODE_REVIEW") => WorkType::CodeReview,
        Some("ACTIVE_MEETING") => WorkType::ActiveMeeting,
        Some("PASSIVE_MEETING") => WorkType::PassiveMeeting,
        Some("BROWSING") => WorkType::Browsing,
        _ => WorkType::Unknown,
    }
}

/// Parse app breakdown JSON to find dominant app.
fn parse_dominant_app(json_str: &str) -> String {
    let breakdown: std::collections::HashMap<String, u64> =
        serde_json::from_str(json_str).unwrap_or_default();
    breakdown
        .into_iter()
        .max_by_key(|(_, dur)| *dur)
        .map(|(app, _)| app)
        .unwrap_or_default()
}

/// Compute aggregate statistics from segment records.
fn compute_statistics(
    records: &[SegmentSummaryRecord],
    state: &AppState,
    date: &NaiveDate,
) -> DailyStatistics {
    let deep_work_hours: f32 = records
        .iter()
        .filter(|r| is_deep_work(r))
        .map(|r| r.duration_secs as f32 / 3600.0)
        .sum();

    let communication_hours: f32 = records
        .iter()
        .filter(|r| is_communication(r))
        .map(|r| r.duration_secs as f32 / 3600.0)
        .sum();

    let meeting_hours: f32 = records
        .iter()
        .filter(|r| is_meeting(r))
        .map(|r| r.duration_secs as f32 / 3600.0)
        .sum();

    // Context switches: count transitions between different regime labels
    let context_switches = if records.len() > 1 {
        records
            .windows(2)
            .filter(|w| {
                let a = w[0].regime_id.as_deref().unwrap_or(&w[0].dominant_category);
                let b = w[1].regime_id.as_deref().unwrap_or(&w[1].dominant_category);
                a != b
            })
            .count() as u32
    } else {
        0
    };

    // Longest focus block
    let (longest_focus_mins, longest_focus_content) = records
        .iter()
        .filter(|r| is_deep_work(r))
        .map(|r| {
            let mins = (r.duration_secs / 60) as u32;
            let content = parse_top_content(&r.content_activities_json);
            (mins, content)
        })
        .max_by_key(|(mins, _)| *mins)
        .unwrap_or((0, String::new()));

    // Regime distribution as percentage
    let total_secs: u64 = records.iter().map(|r| r.duration_secs).sum();
    let regime_distribution = if total_secs > 0 {
        let mut dur_by_regime: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        for r in records {
            let label = r
                .regime_id
                .clone()
                .unwrap_or_else(|| r.dominant_category.clone());
            *dur_by_regime.entry(label).or_default() += r.duration_secs;
        }
        dur_by_regime
            .into_iter()
            .map(|(label, secs)| {
                let pct = (secs as f64 / total_secs as f64 * 100.0).round() as u32;
                (label, pct)
            })
            .collect()
    } else {
        std::collections::HashMap::new()
    };

    // Load previous day for comparison
    let prev_date = date
        .pred_opt()
        .unwrap_or(*date)
        .format("%Y-%m-%d")
        .to_string();
    let comparison = state
        .storage
        .get_daily_digest(&prev_date)
        .ok()
        .flatten()
        .map(|prev| DayComparison {
            deep_work_delta: deep_work_hours - prev.statistics.deep_work_hours,
            communication_delta: communication_hours - prev.statistics.communication_hours,
            context_switch_delta: context_switches as i32 - prev.statistics.context_switches as i32,
        });

    DailyStatistics {
        deep_work_hours,
        communication_hours,
        meeting_hours,
        context_switches,
        longest_focus_mins,
        longest_focus_content,
        regime_distribution,
        comparison,
    }
}

/// Parse top content label from content activities JSON.
fn parse_top_content(json_str: &str) -> String {
    #[derive(serde::Deserialize)]
    struct RawActivity {
        content_label: Option<String>,
        duration_secs: Option<u64>,
    }

    let activities: Vec<RawActivity> = serde_json::from_str(json_str).unwrap_or_default();
    activities
        .into_iter()
        .max_by_key(|a| a.duration_secs.unwrap_or(0))
        .and_then(|a| a.content_label)
        .unwrap_or_default()
}

fn is_deep_work(r: &SegmentSummaryRecord) -> bool {
    let label = r.regime_id.as_deref().unwrap_or(&r.dominant_category);
    label.contains("Deep Focus")
        || label.contains("Development")
        || r.dominant_category == "Development"
}

fn is_communication(r: &SegmentSummaryRecord) -> bool {
    let label = r.regime_id.as_deref().unwrap_or(&r.dominant_category);
    label.contains("Communication") || r.dominant_category == "Communication"
}

fn is_meeting(r: &SegmentSummaryRecord) -> bool {
    let label = r.regime_id.as_deref().unwrap_or(&r.dominant_category);
    label.contains("Meeting") || r.dominant_category == "Meeting"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_work_type_variants() {
        assert_eq!(
            parse_work_type(Some("ACTIVE_CODING")),
            WorkType::ActiveCoding
        );
        assert_eq!(parse_work_type(Some("CODE_REVIEW")), WorkType::CodeReview);
        assert_eq!(parse_work_type(None), WorkType::Unknown);
    }

    #[test]
    fn regime_color_mapping() {
        assert_eq!(regime_color("Deep Focus"), "#3B82F6");
        assert_eq!(regime_color("Communication"), "#F59E0B");
        assert_eq!(regime_color("Research"), "#10B981");
        assert_eq!(regime_color("Meeting"), "#8B5CF6");
        assert_eq!(regime_color("Idle"), "#E5E7EB");
        assert_eq!(regime_color("Unknown"), "#6B7280");
    }

    #[test]
    fn parse_dominant_app_from_json() {
        let json = r#"{"VSCode": 2400, "Terminal": 1200}"#;
        assert_eq!(parse_dominant_app(json), "VSCode");
    }

    #[test]
    fn parse_dominant_app_empty() {
        assert!(parse_dominant_app("{}").is_empty());
        assert!(parse_dominant_app("").is_empty());
    }

    #[test]
    fn parse_content_briefs_top3() {
        let json = r#"[
            {"content_label": "a.rs", "duration_secs": 100, "work_type": "ACTIVE_CODING"},
            {"content_label": "b.rs", "duration_secs": 200, "work_type": "ACTIVE_CODING"},
            {"content_label": "c.rs", "duration_secs": 300, "work_type": "CODE_REVIEW"},
            {"content_label": "d.rs", "duration_secs": 400, "work_type": "ACTIVE_CODING"}
        ]"#;
        let briefs = parse_content_briefs(json);
        assert_eq!(briefs.len(), 3);
        assert_eq!(briefs[0].content, "d.rs");
        assert_eq!(briefs[1].content, "c.rs");
        assert_eq!(briefs[2].content, "b.rs");
    }

    #[test]
    fn parse_content_briefs_empty_json() {
        let briefs = parse_content_briefs("");
        assert!(briefs.is_empty());
    }

    #[test]
    fn dashboard_day_query_defaults() {
        let json = r#"{}"#;
        let query: DashboardDayQuery = serde_json::from_str(json).unwrap();
        assert!(query.date.is_none());
    }

    #[test]
    fn dashboard_day_query_with_date() {
        let json = r#"{"date": "2026-03-18"}"#;
        let query: DashboardDayQuery = serde_json::from_str(json).unwrap();
        assert_eq!(query.date.as_deref(), Some("2026-03-18"));
    }
}
