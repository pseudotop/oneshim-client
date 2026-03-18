use async_trait::async_trait;
use oneshim_core::error::CoreError;
use oneshim_core::ports::embedding_provider::EmbeddingProvider;
use serde::Deserialize;
use tracing::{debug, warn};

/// Remote embedding adapter using OpenAI-compatible embedding API.
///
/// Sends `POST {endpoint}` with `Authorization: Bearer {api_key}` and body
/// `{"model": model, "input": [texts]}`.  Response format:
/// `{"data": [{"embedding": [f32...]}]}`.
pub struct RemoteEmbeddingProvider {
    http_client: reqwest::Client,
    endpoint: String,
    api_key: String,
    model: String,
    dimensions: usize,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

impl RemoteEmbeddingProvider {
    pub fn new(
        endpoint: String,
        api_key: String,
        model: String,
        dimensions: usize,
        timeout_secs: u64,
    ) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .unwrap_or_default();

        if api_key.is_empty() {
            warn!("RemoteEmbeddingProvider: empty API key for endpoint {endpoint}");
        }

        Self {
            http_client,
            endpoint,
            api_key,
            model,
            dimensions,
        }
    }

    async fn request_embeddings(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, CoreError> {
        let body = serde_json::json!({
            "model": self.model,
            "input": texts,
        });

        debug!(
            "RemoteEmbeddingProvider: requesting {} embeddings from {}",
            texts.len(),
            self.endpoint
        );

        let response = self
            .http_client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| CoreError::Network(format!("Embedding API request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            return Err(CoreError::Network(format!(
                "Embedding API returned {status}: {error_body}"
            )));
        }

        let parsed: EmbeddingResponse = response
            .json()
            .await
            .map_err(|e| CoreError::Network(format!("Failed to parse embedding response: {e}")))?;

        Ok(parsed.data.into_iter().map(|d| d.embedding).collect())
    }
}

#[async_trait]
impl EmbeddingProvider for RemoteEmbeddingProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, CoreError> {
        let texts = vec![text.to_string()];
        let mut results = self.request_embeddings(&texts).await?;
        results
            .pop()
            .ok_or_else(|| CoreError::Network("Embedding API returned empty data".to_string()))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, CoreError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        self.request_embeddings(texts).await
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model_id(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parse_valid_response() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "data": [
                        {"embedding": [0.1, 0.2, 0.3]},
                        {"embedding": [0.4, 0.5, 0.6]}
                    ]
                })
                .to_string(),
            )
            .create_async()
            .await;

        let provider = RemoteEmbeddingProvider::new(
            server.url(),
            "test-key".to_string(),
            "text-embedding-3-small".to_string(),
            3,
            30,
        );

        let result = provider
            .embed_batch(&["hello".to_string(), "world".to_string()])
            .await
            .unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0], vec![0.1, 0.2, 0.3]);
        assert_eq!(result[1], vec![0.4, 0.5, 0.6]);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn single_embed() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                serde_json::json!({
                    "data": [{"embedding": [1.0, 2.0, 3.0]}]
                })
                .to_string(),
            )
            .create_async()
            .await;

        let provider = RemoteEmbeddingProvider::new(
            server.url(),
            "test-key".to_string(),
            "text-embedding-3-small".to_string(),
            3,
            30,
        );

        let result = provider.embed("test text").await.unwrap();
        assert_eq!(result, vec![1.0, 2.0, 3.0]);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn handle_api_error() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("POST", "/")
            .with_status(401)
            .with_body(r#"{"error": "invalid api key"}"#)
            .create_async()
            .await;

        let provider = RemoteEmbeddingProvider::new(
            server.url(),
            "bad-key".to_string(),
            "text-embedding-3-small".to_string(),
            3,
            30,
        );

        let result = provider.embed("test").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("401"));
    }

    #[tokio::test]
    async fn empty_batch_returns_empty() {
        let provider = RemoteEmbeddingProvider::new(
            "http://unused".to_string(),
            "key".to_string(),
            "model".to_string(),
            3,
            30,
        );

        let result = provider.embed_batch(&[]).await.unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn model_id_and_dimensions() {
        let provider = RemoteEmbeddingProvider::new(
            "http://example.com".to_string(),
            "key".to_string(),
            "text-embedding-3-small".to_string(),
            1536,
            30,
        );

        assert_eq!(provider.model_id(), "text-embedding-3-small");
        assert_eq!(provider.dimensions(), 1536);
    }
}
