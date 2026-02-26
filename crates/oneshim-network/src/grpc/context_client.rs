//!

use oneshim_core::error::CoreError;
use tonic::transport::Channel;
use tracing::{debug, error, info};

use super::{map_grpc_status_error, GrpcConfig};
use crate::proto::user_context::{
    user_context_service_client::UserContextServiceClient, ContextBatchUploadRequest,
    ContextBatchUploadResponse, FeedbackType, HeartbeatRequest, HeartbeatResponse,
    ListSuggestionsRequest, ListSuggestionsResponse, SubscribeRequest, Suggestion,
    SuggestionFeedback, SuggestionType,
};

pub struct GrpcContextClient {
    client: UserContextServiceClient<Channel>,
    config: GrpcConfig,
}

impl GrpcContextClient {
    pub async fn connect(config: GrpcConfig) -> Result<Self, CoreError> {
        let endpoints = config.all_endpoints();
        let mut last_error = None;

        for endpoint_url in &endpoints {
            info!(endpoint = %endpoint_url, "gRPC context client connection attempt");

            match config.connect_channel(endpoint_url).await {
                Ok(channel) => {
                    let client = UserContextServiceClient::new(channel);
                    info!(endpoint = %endpoint_url, "gRPC context client connection completed");
                    return Ok(Self { client, config });
                }
                Err(e) => {
                    debug!(endpoint = %endpoint_url, error = %e, "gRPC connection failure, next port attempt");
                    last_error = Some(e);
                }
            }
        }

        error!(endpoints = ?endpoints, "all gRPC endpoint connection failure");
        Err(last_error.unwrap_or_else(|| CoreError::Network("gRPC endpoint none".to_string())))
    }

    ///
    pub async fn upload_batch(
        &mut self,
        request: ContextBatchUploadRequest,
    ) -> Result<ContextBatchUploadResponse, CoreError> {
        debug!("gRPC batch upload request");

        let response = self
            .client
            .upload_batch(tonic::Request::new(request))
            .await
            .map_err(|status| {
                error!(error = %status, "gRPC batch upload failure");
                map_grpc_status_error("grpc batch upload failed", status)
            })?;

        Ok(response.into_inner())
    }

    ///
    pub async fn subscribe_suggestions(
        &mut self,
        session_id: &str,
        client_id: &str,
    ) -> Result<tonic::Streaming<Suggestion>, CoreError> {
        debug!("gRPC suggestion stream subscribe request");

        let request = tonic::Request::new(SubscribeRequest {
            session_id: session_id.to_string(),
            client_id: client_id.to_string(),
            subscription_types: vec![],
        });

        let response = self
            .client
            .subscribe_suggestions(request)
            .await
            .map_err(|status| {
                error!(error = %status, "gRPC suggestion stream subscribe failure");
                map_grpc_status_error("grpc suggestion stream subscription failed", status)
            })?;

        Ok(response.into_inner())
    }

    pub async fn send_feedback(
        &mut self,
        suggestion_id: &str,
        feedback_type: FeedbackType,
        comment: Option<&str>,
    ) -> Result<(), CoreError> {
        debug!(suggestion_id = %suggestion_id, "gRPC feedback sent");

        let request = tonic::Request::new(SuggestionFeedback {
            suggestion_id: suggestion_id.to_string(),
            feedback_type: feedback_type as i32,
            timestamp: None,
            comment: comment.map(String::from),
            reason: None,
        });

        self.client.send_feedback(request).await.map_err(|status| {
            error!(error = %status, "gRPC feedback sent failure");
            map_grpc_status_error("grpc feedback submission failed", status)
        })?;

        Ok(())
    }

    pub async fn heartbeat(
        &mut self,
        session_id: &str,
        client_id: &str,
    ) -> Result<HeartbeatResponse, CoreError> {
        debug!(session_id = %session_id, "gRPC heartbeat sent");

        let request = tonic::Request::new(HeartbeatRequest {
            session_id: session_id.to_string(),
            client_id: client_id.to_string(),
            timestamp: None,
            client_state: std::collections::HashMap::new(),
        });

        let response = self.client.heartbeat(request).await.map_err(|status| {
            error!(error = %status, "gRPC heartbeat failure");
            map_grpc_status_error("grpc heartbeat failed", status)
        })?;

        Ok(response.into_inner())
    }

    ///
    pub async fn list_suggestions(
        &mut self,
        types: Vec<SuggestionType>,
        limit: i32,
    ) -> Result<ListSuggestionsResponse, CoreError> {
        debug!(limit = %limit, "gRPC suggestion list query");

        let request = tonic::Request::new(ListSuggestionsRequest {
            types: types.into_iter().map(|t| t as i32).collect(),
            min_priority: 0, // all priorities
            limit,
            active_only: true,
        });

        let response = self
            .client
            .list_suggestions(request)
            .await
            .map_err(|status| {
                error!(error = %status, "gRPC suggestion list query failure");
                map_grpc_status_error("grpc suggestion list failed", status)
            })?;

        Ok(response.into_inner())
    }

    pub fn config(&self) -> &GrpcConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_upload_request() {
        let request = ContextBatchUploadRequest {
            client_id: "client-123".to_string(),
            session_id: "session-456".to_string(),
            upload_trigger: 0, // UNSPECIFIED
            upload_timestamp: None,
            events: vec![],
            frames: vec![],
            client_stats: std::collections::HashMap::new(),
            last_sync_timestamp: None,
            sync_sequence: 1,
        };
        assert_eq!(request.client_id, "client-123");
    }
}
