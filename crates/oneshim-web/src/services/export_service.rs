use axum::http::header;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Duration, Utc};
use oneshim_api_contracts::export::{
    EventExportRecord, ExportQuery, FrameExportRecord, MetricExportRecord,
};
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
