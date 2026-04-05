//! Integration insight producer, source, and checkpoint ports.

use async_trait::async_trait;

use crate::error::CoreError;
use crate::models::integration::IntegrationInsightCandidate;
use crate::models::storage_records::LocalSuggestionRecord;

#[async_trait]
pub trait IntegrationInsightProducerPort: Send + Sync {
    /// Collect locally derived insight candidates and enqueue them for outbound delivery.
    async fn produce_pending(&self) -> Result<usize, CoreError>;
}

#[async_trait]
pub trait IntegrationInsightSourcePort: Send + Sync {
    /// Stable namespace used for durable checkpoint storage.
    fn checkpoint_namespace(&self) -> &'static str;

    /// Return locally derived outbound insight candidates after the checkpoint cursor.
    ///
    /// Implementations must return candidates in stable ascending cursor order so
    /// the producer can safely persist progress after each successful enqueue.
    async fn list_candidates_after(
        &self,
        after_cursor: Option<String>,
        limit: usize,
    ) -> Result<Vec<IntegrationInsightCandidate>, CoreError>;
}

#[async_trait]
pub trait LocalSuggestionQueryPort: Send + Sync {
    /// List locally derived focus suggestions in ascending id order after the given id.
    async fn list_local_suggestions_after(
        &self,
        after_id: Option<i64>,
        limit: usize,
    ) -> Result<Vec<LocalSuggestionRecord>, CoreError>;
}

#[async_trait]
pub trait IntegrationCheckpointStorePort: Send + Sync {
    /// Load a producer-specific checkpoint cursor.
    async fn load_checkpoint(&self, namespace: &str) -> Result<Option<String>, CoreError>;

    /// Persist a producer-specific checkpoint cursor.
    async fn store_checkpoint(&self, namespace: &str, cursor: String) -> Result<(), CoreError>;
}
