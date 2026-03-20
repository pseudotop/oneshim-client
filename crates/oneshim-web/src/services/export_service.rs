use axum::http::header;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Duration, Utc};
use oneshim_api_contracts::export::{
    EventExportRecord, ExportQuery, FrameExportRecord, MetricExportRecord,
};
use oneshim_core::models::storage_records::FocusWorkSessionRecord;
use serde::Serialize;

use crate::error::ApiError;
use crate::services::export_assembler::{
    assemble_event_export_record, assemble_frame_export_record, assemble_metric_export_record,
};
use crate::services::web_contexts::StorageWebContext;

#[derive(Clone)]
pub struct ExportQueryService {
    ctx: StorageWebContext,
}

impl ExportQueryService {
    pub fn new(ctx: StorageWebContext) -> Self {
        Self { ctx }
    }

    pub fn export_metrics(&self, params: &ExportQuery) -> Result<Response, ApiError> {
        let (from, to) = resolve_export_range(params);
        let records: Vec<MetricExportRecord> = self
            .ctx
            .storage
            .list_metric_exports(&from.to_rfc3339(), &to.to_rfc3339())
            .map_err(|error| ApiError::Internal(error.to_string()))?
            .into_iter()
            .map(assemble_metric_export_record)
            .collect();

        export_response(&records, &params.format, "metrics")
    }

    pub fn export_events(&self, params: &ExportQuery) -> Result<Response, ApiError> {
        let (from, to) = resolve_export_range(params);
        let records: Vec<EventExportRecord> = self
            .ctx
            .storage
            .list_event_exports(&from.to_rfc3339(), &to.to_rfc3339())
            .map_err(|error| ApiError::Internal(error.to_string()))?
            .into_iter()
            .map(assemble_event_export_record)
            .collect();

        export_response(&records, &params.format, "events")
    }

    pub fn export_frames(&self, params: &ExportQuery) -> Result<Response, ApiError> {
        let (from, to) = resolve_export_range(params);
        let records: Vec<FrameExportRecord> = self
            .ctx
            .storage
            .list_frame_exports(&from.to_rfc3339(), &to.to_rfc3339())
            .map_err(|error| ApiError::Internal(error.to_string()))?
            .into_iter()
            .map(assemble_frame_export_record)
            .collect();

        export_response(&records, &params.format, "frames")
    }

    /// Export work sessions as iCalendar (.ics) VEVENT entries.
    pub fn export_ical(&self, params: &ExportQuery) -> Result<Response, ApiError> {
        let (from, to) = resolve_export_range(params);
        let sessions = self
            .ctx
            .storage
            .list_work_sessions(&from.to_rfc3339(), &to.to_rfc3339(), 1000)
            .map_err(|error| ApiError::Internal(error.to_string()))?;

        let ical = sessions_to_ical(&sessions);
        let now = Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("sessions_{now}.ics");

        Ok((
            [
                (header::CONTENT_TYPE, "text/calendar; charset=utf-8"),
                (
                    header::CONTENT_DISPOSITION,
                    &format!("attachment; filename=\"{filename}\""),
                ),
            ],
            ical,
        )
            .into_response())
    }

    /// Export work sessions in Toggl-compatible CSV format.
    pub fn export_toggl(&self, params: &ExportQuery) -> Result<Response, ApiError> {
        let (from, to) = resolve_export_range(params);
        let sessions = self
            .ctx
            .storage
            .list_work_sessions(&from.to_rfc3339(), &to.to_rfc3339(), 1000)
            .map_err(|error| ApiError::Internal(error.to_string()))?;

        let csv = sessions_to_toggl_csv(&sessions);
        let now = Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("sessions_toggl_{now}.csv");

        Ok((
            [
                (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
                (
                    header::CONTENT_DISPOSITION,
                    &format!("attachment; filename=\"{filename}\""),
                ),
            ],
            csv,
        )
            .into_response())
    }
}

fn resolve_export_range(params: &ExportQuery) -> (DateTime<Utc>, DateTime<Utc>) {
    let from = params
        .from
        .as_ref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|datetime| datetime.with_timezone(&Utc))
        .unwrap_or_else(|| Utc::now() - Duration::days(7));
    let to = params
        .to
        .as_ref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|datetime| datetime.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    (from, to)
}

fn export_response<T: Serialize>(
    records: &[T],
    format: &str,
    filename_prefix: &str,
) -> Result<Response, ApiError> {
    let now = Utc::now().format("%Y%m%d_%H%M%S");

    match format.to_lowercase().as_str() {
        "csv" => {
            let csv = records_to_csv(records)?;
            let filename = format!("{filename_prefix}_{now}.csv");
            Ok((
                [
                    (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
                    (
                        header::CONTENT_DISPOSITION,
                        &format!("attachment; filename=\"{filename}\""),
                    ),
                ],
                csv,
            )
                .into_response())
        }
        _ => {
            let json = serde_json::to_string_pretty(records).map_err(|error| {
                ApiError::Internal(format!("JSON serialization failed: {error}"))
            })?;
            let filename = format!("{filename_prefix}_{now}.json");
            Ok((
                [
                    (header::CONTENT_TYPE, "application/json; charset=utf-8"),
                    (
                        header::CONTENT_DISPOSITION,
                        &format!("attachment; filename=\"{filename}\""),
                    ),
                ],
                json,
            )
                .into_response())
        }
    }
}

/// Convert work sessions to iCalendar (RFC 5545) format.
///
/// Each completed session maps to a VEVENT with DTSTART/DTEND/SUMMARY.
/// Sessions without an `ended_at` timestamp are skipped.
pub(crate) fn sessions_to_ical(sessions: &[FocusWorkSessionRecord]) -> String {
    let mut buf = String::with_capacity(sessions.len() * 256);
    buf.push_str("BEGIN:VCALENDAR\r\n");
    buf.push_str("VERSION:2.0\r\n");
    buf.push_str("PRODID:-//ONESHIM//Work Sessions//EN\r\n");
    buf.push_str("CALSCALE:GREGORIAN\r\n");

    for session in sessions {
        let Some(ended_at) = &session.ended_at else {
            continue;
        };

        let dtstart = rfc3339_to_ical(&session.started_at);
        let dtend = rfc3339_to_ical(ended_at);

        buf.push_str("BEGIN:VEVENT\r\n");
        buf.push_str(&format!("UID:session-{}@oneshim\r\n", session.id));
        buf.push_str(&format!("DTSTART:{dtstart}\r\n"));
        buf.push_str(&format!("DTEND:{dtend}\r\n"));
        buf.push_str(&format!(
            "SUMMARY:{} ({})\r\n",
            session.primary_app, session.category
        ));
        buf.push_str(&format!(
            "DESCRIPTION:Duration: {}s | Deep work: {}s | Interruptions: {}\r\n",
            session.duration_secs, session.deep_work_secs, session.interruption_count
        ));
        buf.push_str("END:VEVENT\r\n");
    }

    buf.push_str("END:VCALENDAR\r\n");
    buf
}

/// Convert work sessions to Toggl-compatible CSV format.
///
/// Columns: Description, Start date, Start time, Duration, Tags
pub(crate) fn sessions_to_toggl_csv(sessions: &[FocusWorkSessionRecord]) -> String {
    let mut buf = String::from("Description,Start date,Start time,Duration,Tags\n");

    for session in sessions {
        let description = csv_escape(&format!("{} ({})", session.primary_app, session.category));
        let (start_date, start_time) = split_rfc3339_date_time(&session.started_at);
        let duration = format_toggl_duration(session.duration_secs);
        let tags = &session.category;

        buf.push_str(&format!(
            "{description},{start_date},{start_time},{duration},{tags}\n"
        ));
    }

    buf
}

/// Convert an RFC 3339 timestamp to iCal DATETIME format (yyyyMMddTHHmmssZ).
fn rfc3339_to_ical(ts: &str) -> String {
    DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.with_timezone(&Utc).format("%Y%m%dT%H%M%SZ").to_string())
        .unwrap_or_else(|_| ts.replace(['-', ':'], ""))
}

/// Split an RFC 3339 timestamp into (YYYY-MM-DD, HH:MM:SS) parts for Toggl CSV.
fn split_rfc3339_date_time(ts: &str) -> (String, String) {
    DateTime::parse_from_rfc3339(ts)
        .map(|dt| {
            let utc = dt.with_timezone(&Utc);
            (
                utc.format("%Y-%m-%d").to_string(),
                utc.format("%H:%M:%S").to_string(),
            )
        })
        .unwrap_or_else(|_| (ts.to_string(), "00:00:00".to_string()))
}

/// Format seconds as HH:MM:SS for Toggl duration column.
fn format_toggl_duration(secs: u64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

/// Escape a CSV field value: quote it if it contains comma, quote, or newline.
fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

pub(crate) fn records_to_csv<T: Serialize>(records: &[T]) -> Result<String, ApiError> {
    if records.is_empty() {
        return Ok(String::new());
    }

    let json_values: Vec<serde_json::Value> = records
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| ApiError::Internal(format!("JSON conversion failed: {error}")))?;

    let headers: Vec<String> = json_values
        .first()
        .and_then(|value| value.as_object())
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default();

    let mut csv = headers.join(",") + "\n";

    for value in &json_values {
        if let Some(object) = value.as_object() {
            let row: Vec<String> = headers
                .iter()
                .map(|header_name| {
                    object
                        .get(header_name)
                        .map(|item| match item {
                            serde_json::Value::String(value) => {
                                if value.contains(',')
                                    || value.contains('"')
                                    || value.contains('\n')
                                {
                                    format!("\"{}\"", value.replace('"', "\"\""))
                                } else {
                                    value.clone()
                                }
                            }
                            serde_json::Value::Null => String::new(),
                            other => other.to_string(),
                        })
                        .unwrap_or_default()
                })
                .collect();
            csv.push_str(&row.join(","));
            csv.push('\n');
        }
    }

    Ok(csv)
}
