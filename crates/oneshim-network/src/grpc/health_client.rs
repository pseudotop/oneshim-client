//! gRPC Health Check 클라이언트
//!
//! 표준 gRPC Health Check Protocol (grpc.health.v1)을 사용하여
//! 서버 및 개별 서비스의 상태를 확인합니다.
//!
//! ## 사용 예시
//!
//! ```rust,ignore
//! use oneshim_network::grpc::{GrpcHealthClient, GrpcConfig};
//!
//! let config = GrpcConfig::default();
//! let mut client = GrpcHealthClient::connect(config).await?;
//!
//! // 전체 서버 상태 확인
//! let status = client.check("").await?;
//! println!("서버 상태: {:?}", status);
//!
//! // 특정 서비스 상태 확인
//! let auth_status = client
//!     .check("oneshim.v1.auth.AuthenticationService")
//!     .await?;
//! println!("인증 서비스 상태: {:?}", auth_status);
//! ```

use tonic::transport::Channel;
use tonic_health::pb::health_client::HealthClient;
use tonic_health::pb::HealthCheckRequest;
use tracing::{debug, error, info};

use oneshim_core::error::CoreError;

use super::{map_grpc_status_error, GrpcConfig};

/// gRPC 서비스 상태
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServingStatus {
    /// 알 수 없음
    Unknown,
    /// 서비스 중
    Serving,
    /// 서비스 중지
    NotServing,
    /// 서비스 타입 (Watch RPC용)
    ServiceUnknown,
}

impl From<i32> for ServingStatus {
    fn from(value: i32) -> Self {
        match value {
            0 => ServingStatus::Unknown,
            1 => ServingStatus::Serving,
            2 => ServingStatus::NotServing,
            3 => ServingStatus::ServiceUnknown,
            _ => ServingStatus::Unknown,
        }
    }
}

impl std::fmt::Display for ServingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServingStatus::Unknown => write!(f, "UNKNOWN"),
            ServingStatus::Serving => write!(f, "SERVING"),
            ServingStatus::NotServing => write!(f, "NOT_SERVING"),
            ServingStatus::ServiceUnknown => write!(f, "SERVICE_UNKNOWN"),
        }
    }
}

/// 서비스 상태 정보
#[derive(Debug, Clone)]
pub struct ServiceHealth {
    /// 서비스 이름 (빈 문자열이면 전체 서버)
    pub service: String,
    /// 서비스 상태
    pub status: ServingStatus,
}

/// gRPC Health Check 클라이언트
///
/// 표준 gRPC Health Check Protocol을 사용하여 서버 상태를 확인합니다.
pub struct GrpcHealthClient {
    client: HealthClient<Channel>,
    #[allow(dead_code)]
    config: GrpcConfig,
}

impl GrpcHealthClient {
    /// gRPC 서버에 연결 (포트 fallback 지원)
    ///
    /// # Arguments
    ///
    /// * `config` - gRPC 설정
    ///
    /// # Returns
    ///
    /// 연결된 Health Check 클라이언트
    pub async fn connect(config: GrpcConfig) -> Result<Self, CoreError> {
        let endpoints = config.all_endpoints();
        let mut last_error = None;

        for endpoint_url in &endpoints {
            debug!("gRPC Health 클라이언트 연결 시도: {}", endpoint_url);

            match config.connect_channel(endpoint_url).await {
                Ok(channel) => {
                    info!("gRPC Health 클라이언트 연결: {}", endpoint_url);
                    let client = HealthClient::new(channel);
                    return Ok(Self { client, config });
                }
                Err(e) => {
                    debug!(
                        "gRPC Health 연결 실패, 다음 포트 시도: {} - {}",
                        endpoint_url, e
                    );
                    last_error = Some(e);
                }
            }
        }

        // 모든 포트 시도 실패
        error!("모든 gRPC 엔드포인트 연결 실패: {:?}", endpoints);
        Err(last_error.unwrap_or_else(|| CoreError::Network("gRPC 엔드포인트 없음".to_string())))
    }

    /// 서비스 상태 확인
    ///
    /// # Arguments
    ///
    /// * `service` - 서비스 이름 (빈 문자열이면 전체 서버 상태)
    ///
    /// # Returns
    ///
    /// 서비스 상태
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // 전체 서버 상태
    /// let status = client.check("").await?;
    ///
    /// // 특정 서비스 상태
    /// let status = client.check("oneshim.v1.auth.AuthenticationService").await?;
    /// ```
    pub async fn check(&mut self, service: &str) -> Result<ServingStatus, CoreError> {
        debug!(
            "Health check: {}",
            if service.is_empty() {
                "<server>"
            } else {
                service
            }
        );

        let request = tonic::Request::new(HealthCheckRequest {
            service: service.to_string(),
        });

        let response = self.client.check(request).await.map_err(|status| {
            error!("Health check 실패: {} - {}", service, status);
            map_grpc_status_error("grpc health check failed", status)
        })?;

        let status = ServingStatus::from(response.into_inner().status);
        debug!("Health check 결과: {} -> {:?}", service, status);

        Ok(status)
    }

    /// 여러 서비스 상태 일괄 확인
    ///
    /// # Arguments
    ///
    /// * `services` - 확인할 서비스 이름 목록
    ///
    /// # Returns
    ///
    /// 서비스별 상태 목록
    pub async fn check_all(&mut self, services: &[&str]) -> Vec<ServiceHealth> {
        let mut results = Vec::with_capacity(services.len());

        for service in services {
            let status = match self.check(service).await {
                Ok(status) => status,
                Err(_) => ServingStatus::Unknown,
            };

            results.push(ServiceHealth {
                service: service.to_string(),
                status,
            });
        }

        results
    }

    /// ONESHIM 서비스 상태 확인
    ///
    /// 모든 ONESHIM gRPC 서비스의 상태를 확인합니다.
    ///
    /// # Returns
    ///
    /// 서비스별 상태 목록
    pub async fn check_oneshim_services(&mut self) -> Vec<ServiceHealth> {
        let services = [
            "", // 전체 서버
            "oneshim.v1.auth.AuthenticationService",
            "oneshim.v1.auth.SessionService",
            "oneshim.v1.user_context.UserContextService",
        ];

        self.check_all(&services).await
    }

    /// 서버가 정상인지 확인
    ///
    /// # Returns
    ///
    /// 서버가 SERVING 상태이면 true
    pub async fn is_healthy(&mut self) -> bool {
        matches!(self.check("").await, Ok(ServingStatus::Serving))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serving_status_from_i32() {
        assert_eq!(ServingStatus::from(0), ServingStatus::Unknown);
        assert_eq!(ServingStatus::from(1), ServingStatus::Serving);
        assert_eq!(ServingStatus::from(2), ServingStatus::NotServing);
        assert_eq!(ServingStatus::from(3), ServingStatus::ServiceUnknown);
        assert_eq!(ServingStatus::from(99), ServingStatus::Unknown);
    }

    #[test]
    fn test_serving_status_display() {
        assert_eq!(ServingStatus::Unknown.to_string(), "UNKNOWN");
        assert_eq!(ServingStatus::Serving.to_string(), "SERVING");
        assert_eq!(ServingStatus::NotServing.to_string(), "NOT_SERVING");
        assert_eq!(ServingStatus::ServiceUnknown.to_string(), "SERVICE_UNKNOWN");
    }

    #[test]
    fn test_service_health_clone() {
        let health = ServiceHealth {
            service: "test.Service".to_string(),
            status: ServingStatus::Serving,
        };
        let cloned = health.clone();
        assert_eq!(cloned.service, "test.Service");
        assert_eq!(cloned.status, ServingStatus::Serving);
    }
}
