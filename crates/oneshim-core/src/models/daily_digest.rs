use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::tiered_memory::WorkType;

/// Aggregated daily summary containing timeline, statistics, and LLM insight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyDigest {
    pub date: NaiveDate,
    pub insight: Option<DailyInsight>,
    pub timeline: Vec<TimelineEntry>,
    pub statistics: DailyStatistics,
    pub generated_at: DateTime<Utc>,
}

/// LLM-generated narrative and key highlights for the day.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyInsight {
    pub narrative: String,
    pub highlights: Vec<DigestHighlight>,
}

/// A single highlight within a daily digest (achievement, warning, or suggestion).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestHighlight {
    pub highlight_type: HighlightType,
    pub text: String,
    pub segment_id: Option<String>,
}

/// Classification of a digest highlight.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HighlightType {
    Achievement,
    Warning,
    Suggestion,
}

/// A single time block in the daily timetable view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub segment_id: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration_mins: u32,
    pub regime_label: String,
    pub regime_color: String,
    pub dominant_app: String,
    pub content_summary: Vec<ContentBrief>,
    pub annotation: Option<DigestHighlight>,
}

/// Brief description of work content within a timeline entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBrief {
    pub content: String,
    pub work_type: WorkType,
    pub mins: u32,
}

/// Aggregate statistics for a single day.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DailyStatistics {
    pub deep_work_hours: f32,
    pub communication_hours: f32,
    pub meeting_hours: f32,
    pub context_switches: u32,
    pub longest_focus_mins: u32,
    pub longest_focus_content: String,
    pub regime_distribution: HashMap<String, u32>,
    pub comparison: Option<DayComparison>,
}

/// Delta comparison against a previous day (or rolling average).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayComparison {
    pub deep_work_delta: f32,
    pub communication_delta: f32,
    pub context_switch_delta: i32,
}

// ── Shared classification helpers ─────────────────────────────
// Canonical implementations used by both oneshim-analysis (DailyDigestGenerator)
// and oneshim-web (dashboard handler) to eliminate logic duplication.

/// Regime color for timetable display.
pub fn regime_color(label: &str) -> &'static str {
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

/// Check if a segment represents deep work.
pub fn is_deep_work(regime_id: Option<&str>, dominant_category: &str) -> bool {
    regime_id.map_or(false, |r| r.contains("Deep Focus") || r.contains("Development"))
        || dominant_category == "Development"
}

/// Check if a segment represents communication.
pub fn is_communication(regime_id: Option<&str>, dominant_category: &str) -> bool {
    regime_id.map_or(false, |r| r.contains("Communication"))
        || dominant_category == "Communication"
}

/// Check if a segment represents a meeting.
pub fn is_meeting(dominant_category: &str) -> bool {
    dominant_category == "Meeting"
        || dominant_category.contains("Zoom")
        || dominant_category.contains("Meet")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn daily_digest_serde_roundtrip() {
        let digest = DailyDigest {
            date: Utc::now().date_naive(),
            insight: Some(DailyInsight {
                narrative: "Great focus day".to_string(),
                highlights: vec![DigestHighlight {
                    highlight_type: HighlightType::Achievement,
                    text: "2h deep work block".to_string(),
                    segment_id: Some("seg-001".to_string()),
                }],
            }),
            timeline: vec![],
            statistics: DailyStatistics::default(),
            generated_at: Utc::now(),
        };
        let json = serde_json::to_string(&digest).unwrap();
        let back: DailyDigest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.date, digest.date);
        assert!(back.insight.is_some());
    }

    #[test]
    fn highlight_type_serde() {
        let ht = HighlightType::Achievement;
        let json = serde_json::to_string(&ht).unwrap();
        assert_eq!(json, "\"ACHIEVEMENT\"");
        let back: HighlightType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, HighlightType::Achievement);
    }

    #[test]
    fn regime_color_mapping() {
        assert_eq!(regime_color("Deep Focus"), "#3B82F6");
        assert_eq!(regime_color("Development"), "#3B82F6");
        assert_eq!(regime_color("Communication"), "#F59E0B");
        assert_eq!(regime_color("Research"), "#10B981");
        assert_eq!(regime_color("Meeting"), "#8B5CF6");
        assert_eq!(regime_color("Idle"), "#E5E7EB");
        assert_eq!(regime_color("Unknown"), "#6B7280");
    }

    #[test]
    fn classification_helpers() {
        assert!(is_deep_work(Some("Deep Focus"), "Development"));
        assert!(is_deep_work(None, "Development"));
        assert!(!is_deep_work(Some("Communication"), "Communication"));

        assert!(is_communication(Some("Communication"), "Other"));
        assert!(is_communication(None, "Communication"));
        assert!(!is_communication(Some("Deep Focus"), "Development"));

        assert!(is_meeting("Meeting"));
        assert!(is_meeting("Zoom Call"));
        assert!(is_meeting("Google Meet"));
        assert!(!is_meeting("Development"));
    }

    #[test]
    fn day_comparison_serde() {
        let cmp = DayComparison {
            deep_work_delta: 0.5,
            communication_delta: -0.2,
            context_switch_delta: -3,
        };
        let json = serde_json::to_string(&cmp).unwrap();
        let back: DayComparison = serde_json::from_str(&json).unwrap();
        assert!((back.deep_work_delta - 0.5).abs() < f32::EPSILON);
    }
}
