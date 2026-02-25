pub mod automation;
pub mod backup;
pub mod data;
pub mod events;
pub mod export;
pub mod focus;
pub mod frames;
pub mod idle;
pub mod metrics;
pub mod onboarding;
pub mod processes;
pub mod reports;
pub mod search;
pub mod sessions;
pub mod settings;
pub mod stats;
pub mod stream;
pub mod support;
pub mod tags;
pub mod timeline;
pub mod update;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct TimeRangeQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct PaginationMeta {
    pub total: u64,
    pub offset: usize,
    pub limit: usize,
    pub has_more: bool,
}

#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub data: Vec<T>,
    pub pagination: PaginationMeta,
}

impl TimeRangeQuery {
    pub fn from_datetime(&self) -> DateTime<Utc> {
        self.from
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|| Utc::now() - Duration::hours(24))
    }

    pub fn to_datetime(&self) -> DateTime<Utc> {
        self.to
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now)
    }

    pub fn limit_or_default(&self) -> usize {
        self.limit.unwrap_or(100)
    }

    pub fn offset_or_default(&self) -> usize {
        self.offset.unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_range_defaults() {
        let query = TimeRangeQuery {
            from: None,
            to: None,
            limit: None,
            offset: None,
        };

        let now = Utc::now();
        assert!(query.from_datetime() < now);
        assert!(query.to_datetime() <= now + Duration::seconds(1));
        assert_eq!(query.limit_or_default(), 100);
        assert_eq!(query.offset_or_default(), 0);
    }

    #[test]
    fn time_range_custom() {
        let query = TimeRangeQuery {
            from: Some("2024-01-01T00:00:00Z".to_string()),
            to: Some("2024-01-02T00:00:00Z".to_string()),
            limit: Some(50),
            offset: Some(10),
        };

        assert_eq!(query.limit_or_default(), 50);
        assert_eq!(query.offset_or_default(), 10);
        assert_eq!(
            query.from_datetime().to_rfc3339(),
            "2024-01-01T00:00:00+00:00"
        );
    }

    #[test]
    fn pagination_meta_has_more() {
        let meta = PaginationMeta {
            total: 100,
            offset: 0,
            limit: 50,
            has_more: true,
        };
        assert!(meta.has_more);
        assert_eq!(meta.total, 100);
    }
}
