use axum::{
    extract::{Query, State},
    response::Response,
};
use oneshim_api_contracts::export::ExportQuery;
#[cfg(test)]
use oneshim_api_contracts::export::{EventExportRecord, MetricExportRecord};

use crate::error::ApiError;
#[cfg(test)]
use crate::services::export_service::records_to_csv;
use crate::services::export_service::ExportQueryService;
use crate::services::web_contexts::StorageWebContext;

pub async fn export_metrics(
    State(context): State<StorageWebContext>,
    Query(params): Query<ExportQuery>,
) -> Result<Response, ApiError> {
    ExportQueryService::new(context).export_metrics(&params)
}

pub async fn export_events(
    State(context): State<StorageWebContext>,
    Query(params): Query<ExportQuery>,
) -> Result<Response, ApiError> {
    ExportQueryService::new(context).export_events(&params)
}

pub async fn export_frames(
    State(context): State<StorageWebContext>,
    Query(params): Query<ExportQuery>,
) -> Result<Response, ApiError> {
    ExportQueryService::new(context).export_frames(&params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_escapes_special_chars() {
        let records = vec![EventExportRecord {
            event_id: "1".to_string(),
            event_type: "context".to_string(),
            timestamp: "2024-01-30T10:00:00Z".to_string(),
            app_name: Some("VS Code".to_string()),
            window_title: Some("file.rs, modified".to_string()), // includes comma
        }];
        let csv = records_to_csv(&records).unwrap();
        assert!(csv.contains("\"file.rs, modified\"")); // quoted field
    }

    #[test]
    fn empty_records_returns_empty_csv() {
        let records: Vec<MetricExportRecord> = vec![];
        let csv = records_to_csv(&records).unwrap();
        assert!(csv.is_empty());
    }

    #[test]
    fn default_format_is_json() {
        let query: ExportQuery = serde_json::from_str("{}").unwrap();
        assert_eq!(query.format, "json");
    }
}
