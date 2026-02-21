//! API 핸들러 모듈.

pub mod automation;
pub mod backup;
pub mod data;
pub mod events;
pub mod export;
pub mod focus;
pub mod frames;
pub mod idle;
pub mod metrics;
pub mod processes;
pub mod reports;
pub mod search;
pub mod sessions;
pub mod settings;
pub mod stats;
pub mod stream;
pub mod tags;
pub mod timeline;
pub mod update;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// 시간 범위 쿼리 파라미터
#[derive(Debug, Deserialize)]
pub struct TimeRangeQuery {
    /// 시작 시각 (RFC3339, 기본: 24시간 전)
    pub from: Option<String>,
    /// 종료 시각 (RFC3339, 기본: 현재)
    pub to: Option<String>,
    /// 최대 조회 개수 (기본: 100)
    pub limit: Option<usize>,
    /// 건너뛸 개수 (기본: 0)
    pub offset: Option<usize>,
}

/// 페이지네이션 메타데이터
#[derive(Debug, Serialize)]
pub struct PaginationMeta {
    /// 전체 항목 수
    pub total: u64,
    /// 현재 오프셋
    pub offset: usize,
    /// 요청한 limit
    pub limit: usize,
    /// 다음 페이지 존재 여부
    pub has_more: bool,
}

/// 페이지네이션된 응답 구조
#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    /// 데이터 목록
    pub data: Vec<T>,
    /// 페이지네이션 메타데이터
    pub pagination: PaginationMeta,
}

impl TimeRangeQuery {
    /// 기본값이 적용된 시작 시각
    pub fn from_datetime(&self) -> DateTime<Utc> {
        self.from
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|| Utc::now() - Duration::hours(24))
    }

    /// 기본값이 적용된 종료 시각
    pub fn to_datetime(&self) -> DateTime<Utc> {
        self.to
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now)
    }

    /// 기본값이 적용된 제한 개수
    pub fn limit_or_default(&self) -> usize {
        self.limit.unwrap_or(100)
    }

    /// 기본값이 적용된 오프셋
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
