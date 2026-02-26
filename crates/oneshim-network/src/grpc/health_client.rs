//!
//!
//!
//! ```rust,ignore
//! use oneshim_network::grpc::{GrpcHealthClient, GrpcConfig};
//!
//! let config = GrpcConfig::default();
//! let mut client = GrpcHealthClient::connect(config).await?;
//!
//! let status = client.check("").await?;
//!
//! let auth_status = client
//!     .check("oneshim.v1.auth.AuthenticationService")
//!     .await?;
//! ```

use tonic::transport::Channel;
use tonic_health::pb::health_client::HealthClient;
use tonic_health::pb::HealthCheckRequest;
use tracing::{debug, error, info};

use oneshim_core::error::CoreError;

use super::{map_grpc_status_error, GrpcConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServingStatus {
    Unknown,
    Serving,
    NotServing,
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

#[derive(Debug, Clone)]
pub struct ServiceHealth {
    pub service: String,
    pub status: ServingStatus,
}

///
pub struct GrpcHealthClient {
    client: HealthClient<Channel>,
    #[allow(dead_code)]
    config: GrpcConfig,
}

impl GrpcHealthClient {
    ///
    /// # Arguments
    ///
    ///
    /// # Returns
    ///
    pub async fn connect(config: GrpcConfig) -> Result<Self, CoreError> {
        let endpoints = config.all_endpoints();
        let mut last_error = None;

        for endpoint_url in &endpoints {
            debug!("gRPC Health client connection attempt: {}", endpoint_url);

            match config.connect_channel(endpoint_url).await {
                Ok(channel) => {
                    info!("gRPC Health client connection: {}", endpoint_url);
                    let client = HealthClient::new(channel);
                    return Ok(Self { client, config });
                }
                Err(e) => {
                    debug!(
                        "gRPC Health connection failure, next port attempt: {} - {}",
                        endpoint_url, e
                    );
                    last_error = Some(e);
                }
            }
        }

        error!("all gRPC endpoint connection failure: {:?}", endpoints);
        Err(last_error.unwrap_or_else(|| CoreError::Network("gRPC endpoint none".to_string())))
    }

    ///
    /// # Arguments
    ///
    ///
    /// # Returns
    ///
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let status = client.check("").await?;
    ///
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
            error!("Health check failure: {} - {}", service, status);
            map_grpc_status_error("grpc health check failed", status)
        })?;

        let status = ServingStatus::from(response.into_inner().status);
        debug!("Health check: {} -> {:?}", service, status);

        Ok(status)
    }

    ///
    /// # Arguments
    ///
    ///
    /// # Returns
    ///
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

    ///
    ///
    /// # Returns
    ///
    pub async fn check_oneshim_services(&mut self) -> Vec<ServiceHealth> {
        let services = [
            "", // server            "oneshim.v1.auth.AuthenticationService",
            "oneshim.v1.auth.SessionService",
            "oneshim.v1.user_context.UserContextService",
        ];

        self.check_all(&services).await
    }

    ///
    /// # Returns
    ///
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
