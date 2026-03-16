use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use oneshim_core::error::CoreError;
use oneshim_core::models::integration::{
    InsightPacket, IntegrationAckCursor, IntegrationInboxItemStatus, IntegrationInsightAuditRecord,
    IntegrationSessionState, QueuedInsightPacket, StoredProactivePrompt,
};
use oneshim_core::ports::integration::{
    IntegrationAuditPort, IntegrationInboxStorePort, IntegrationOutboxPort,
    IntegrationSessionStorePort,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Default, Serialize, Deserialize)]
struct FileIntegrationStateRegistry {
    version: u32,
    session: Option<IntegrationSessionState>,
    outbox: Vec<QueuedInsightPacket>,
    outbox_ack_cursor: Option<IntegrationAckCursor>,
    inbox: BTreeMap<String, StoredProactivePrompt>,
    inbox_ack_cursor: Option<IntegrationAckCursor>,
    audit_records: Vec<IntegrationInsightAuditRecord>,
}

const MAX_AUDIT_RECORDS: usize = 512;

impl FileIntegrationStateRegistry {
    fn new() -> Self {
        Self {
            version: 1,
            session: None,
            outbox: Vec::new(),
            outbox_ack_cursor: None,
            inbox: BTreeMap::new(),
            inbox_ack_cursor: None,
            audit_records: Vec::new(),
        }
    }

    fn load_or_default(path: &Path) -> Result<Self, CoreError> {
        match std::fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents).map_err(|err| {
                CoreError::Internal(format!("integration state registry parse: {err}"))
            }),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::new()),
            Err(err) => Err(err.into()),
        }
    }

    fn save(&self, path: &Path) -> Result<(), CoreError> {
        let serialized = serde_json::to_string_pretty(self).map_err(|err| {
            CoreError::Internal(format!("integration state registry serialization: {err}"))
        })?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp_path = path.with_extension("tmp");
        std::fs::write(&temp_path, serialized)?;
        std::fs::rename(&temp_path, path)?;
        Ok(())
    }
}

struct FileIntegrationStateInner {
    registry_path: PathBuf,
    registry: parking_lot::Mutex<FileIntegrationStateRegistry>,
}

impl FileIntegrationStateInner {
    fn new(registry_path: PathBuf) -> Result<Self, CoreError> {
        if let Some(parent) = registry_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let registry = FileIntegrationStateRegistry::load_or_default(&registry_path)?;
        Ok(Self {
            registry_path,
            registry: parking_lot::Mutex::new(registry),
        })
    }

    fn save_registry(&self, registry: &FileIntegrationStateRegistry) -> Result<(), CoreError> {
        registry.save(&self.registry_path)
    }

    fn load_session_sync(&self) -> Option<IntegrationSessionState> {
        self.registry.lock().session.clone()
    }

    fn store_session_sync(&self, state: IntegrationSessionState) -> Result<(), CoreError> {
        let mut registry = self.registry.lock();
        registry.session = Some(state);
        self.save_registry(&registry)
    }

    fn clear_session_sync(&self) -> Result<(), CoreError> {
        let mut registry = self.registry.lock();
        registry.session = None;
        self.save_registry(&registry)
    }

    fn enqueue_outbox_sync(
        &self,
        envelope: oneshim_core::models::integration::IntegrationEnvelope,
        packet: InsightPacket,
    ) -> Result<String, CoreError> {
        let mut registry = self.registry.lock();
        let queue_id = format!("integration_queue_{}", Uuid::new_v4());
        registry.outbox.push(QueuedInsightPacket {
            queue_id: queue_id.clone(),
            envelope,
            packet,
            queued_at: Utc::now(),
        });
        self.save_registry(&registry)?;
        Ok(queue_id)
    }

    fn list_outbox_sync(&self, limit: usize) -> Vec<QueuedInsightPacket> {
        self.registry
            .lock()
            .outbox
            .iter()
            .take(limit)
            .cloned()
            .collect()
    }

    fn delete_outbox_sync(&self, queue_ids: &[String]) -> Result<(), CoreError> {
        let mut registry = self.registry.lock();
        registry
            .outbox
            .retain(|item| !queue_ids.contains(&item.queue_id));
        self.save_registry(&registry)
    }

    fn outbox_ack_cursor_sync(&self) -> Option<IntegrationAckCursor> {
        self.registry.lock().outbox_ack_cursor.clone()
    }

    fn store_outbox_ack_cursor_sync(&self, cursor: IntegrationAckCursor) -> Result<(), CoreError> {
        let mut registry = self.registry.lock();
        registry.outbox_ack_cursor = Some(cursor);
        self.save_registry(&registry)
    }

    fn upsert_inbox_sync(&self, prompts: Vec<StoredProactivePrompt>) -> Result<(), CoreError> {
        let mut registry = self.registry.lock();
        for prompt in prompts {
            if let Some(existing) = registry.inbox.get_mut(&prompt.prompt.prompt_id) {
                existing.prompt = prompt.prompt;
            } else {
                registry
                    .inbox
                    .insert(prompt.prompt.prompt_id.clone(), prompt);
            }
        }
        self.save_registry(&registry)
    }

    fn list_inbox_pending_sync(&self) -> Vec<StoredProactivePrompt> {
        let mut prompts: Vec<_> = self
            .registry
            .lock()
            .inbox
            .values()
            .filter(|prompt| prompt.status == IntegrationInboxItemStatus::Pending)
            .cloned()
            .collect();
        prompts.sort_by_key(|prompt| prompt.received_at);
        prompts
    }

    fn update_inbox_status_sync(
        &self,
        prompt_id: &str,
        status: IntegrationInboxItemStatus,
        reason: Option<String>,
    ) -> Result<(), CoreError> {
        let mut registry = self.registry.lock();
        let prompt = registry
            .inbox
            .get_mut(prompt_id)
            .ok_or_else(|| CoreError::NotFound {
                resource_type: "integration_prompt".to_string(),
                id: prompt_id.to_string(),
            })?;
        prompt.status = status;
        prompt.status_updated_at = Utc::now();
        prompt.dismiss_reason = reason;
        self.save_registry(&registry)
    }

    fn expire_inbox_sync(&self) -> Result<usize, CoreError> {
        let now = Utc::now();
        let mut expired = 0usize;
        let mut registry = self.registry.lock();
        for prompt in registry.inbox.values_mut() {
            if prompt.status == IntegrationInboxItemStatus::Pending
                && prompt
                    .prompt
                    .expires_at
                    .is_some_and(|expires_at| expires_at <= now)
            {
                prompt.status = IntegrationInboxItemStatus::Expired;
                prompt.status_updated_at = now;
                expired += 1;
            }
        }
        if expired > 0 {
            self.save_registry(&registry)?;
        }
        Ok(expired)
    }

    fn inbox_ack_cursor_sync(&self) -> Option<IntegrationAckCursor> {
        self.registry.lock().inbox_ack_cursor.clone()
    }

    fn store_inbox_ack_cursor_sync(&self, cursor: IntegrationAckCursor) -> Result<(), CoreError> {
        let mut registry = self.registry.lock();
        registry.inbox_ack_cursor = Some(cursor);
        self.save_registry(&registry)
    }

    fn record_audit_sync(&self, record: IntegrationInsightAuditRecord) -> Result<(), CoreError> {
        let mut registry = self.registry.lock();
        registry.audit_records.push(record);
        if registry.audit_records.len() > MAX_AUDIT_RECORDS {
            let overflow = registry.audit_records.len() - MAX_AUDIT_RECORDS;
            registry.audit_records.drain(0..overflow);
        }
        self.save_registry(&registry)
    }

    fn recent_audit_sync(&self, limit: usize) -> Vec<IntegrationInsightAuditRecord> {
        let registry = self.registry.lock();
        let take = limit.max(1);
        registry
            .audit_records
            .iter()
            .rev()
            .take(take)
            .cloned()
            .collect()
    }
}

#[derive(Clone)]
pub struct FileIntegrationStateStore {
    inner: Arc<FileIntegrationStateInner>,
}

impl FileIntegrationStateStore {
    pub fn new(registry_path: PathBuf) -> Result<Self, CoreError> {
        Ok(Self {
            inner: Arc::new(FileIntegrationStateInner::new(registry_path)?),
        })
    }

    pub fn session_store(&self) -> FileIntegrationSessionStore {
        FileIntegrationSessionStore {
            inner: self.inner.clone(),
        }
    }

    pub fn outbox_store(&self) -> FileIntegrationOutboxStore {
        FileIntegrationOutboxStore {
            inner: self.inner.clone(),
        }
    }

    pub fn inbox_store(&self) -> FileIntegrationInboxStore {
        FileIntegrationInboxStore {
            inner: self.inner.clone(),
        }
    }

    pub fn audit_store(&self) -> FileIntegrationAuditStore {
        FileIntegrationAuditStore {
            inner: self.inner.clone(),
        }
    }
}

#[derive(Clone)]
pub struct FileIntegrationSessionStore {
    inner: Arc<FileIntegrationStateInner>,
}

#[async_trait]
impl IntegrationSessionStorePort for FileIntegrationSessionStore {
    async fn load(&self) -> Result<Option<IntegrationSessionState>, CoreError> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || Ok(inner.load_session_sync()))
            .await
            .map_err(|err| CoreError::Internal(format!("spawn_blocking: {err}")))?
    }

    async fn store(&self, state: IntegrationSessionState) -> Result<(), CoreError> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || inner.store_session_sync(state))
            .await
            .map_err(|err| CoreError::Internal(format!("spawn_blocking: {err}")))?
    }

    async fn clear(&self) -> Result<(), CoreError> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || inner.clear_session_sync())
            .await
            .map_err(|err| CoreError::Internal(format!("spawn_blocking: {err}")))?
    }
}

#[derive(Clone)]
pub struct FileIntegrationOutboxStore {
    inner: Arc<FileIntegrationStateInner>,
}

#[async_trait]
impl IntegrationOutboxPort for FileIntegrationOutboxStore {
    async fn enqueue_insight(
        &self,
        envelope: oneshim_core::models::integration::IntegrationEnvelope,
        packet: InsightPacket,
    ) -> Result<String, CoreError> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || inner.enqueue_outbox_sync(envelope, packet))
            .await
            .map_err(|err| CoreError::Internal(format!("spawn_blocking: {err}")))?
    }

    async fn list_pending(&self, limit: usize) -> Result<Vec<QueuedInsightPacket>, CoreError> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || Ok(inner.list_outbox_sync(limit)))
            .await
            .map_err(|err| CoreError::Internal(format!("spawn_blocking: {err}")))?
    }

    async fn delete(&self, queue_ids: &[String]) -> Result<(), CoreError> {
        let inner = self.inner.clone();
        let queue_ids = queue_ids.to_vec();
        tokio::task::spawn_blocking(move || inner.delete_outbox_sync(&queue_ids))
            .await
            .map_err(|err| CoreError::Internal(format!("spawn_blocking: {err}")))?
    }

    async fn last_ack_cursor(&self) -> Result<Option<IntegrationAckCursor>, CoreError> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || Ok(inner.outbox_ack_cursor_sync()))
            .await
            .map_err(|err| CoreError::Internal(format!("spawn_blocking: {err}")))?
    }

    async fn store_ack_cursor(&self, cursor: IntegrationAckCursor) -> Result<(), CoreError> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || inner.store_outbox_ack_cursor_sync(cursor))
            .await
            .map_err(|err| CoreError::Internal(format!("spawn_blocking: {err}")))?
    }
}

#[derive(Clone)]
pub struct FileIntegrationInboxStore {
    inner: Arc<FileIntegrationStateInner>,
}

#[async_trait]
impl IntegrationInboxStorePort for FileIntegrationInboxStore {
    async fn upsert_prompts(&self, prompts: Vec<StoredProactivePrompt>) -> Result<(), CoreError> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || inner.upsert_inbox_sync(prompts))
            .await
            .map_err(|err| CoreError::Internal(format!("spawn_blocking: {err}")))?
    }

    async fn list_pending(&self) -> Result<Vec<StoredProactivePrompt>, CoreError> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || Ok(inner.list_inbox_pending_sync()))
            .await
            .map_err(|err| CoreError::Internal(format!("spawn_blocking: {err}")))?
    }

    async fn update_status(
        &self,
        prompt_id: &str,
        status: IntegrationInboxItemStatus,
        reason: Option<String>,
    ) -> Result<(), CoreError> {
        let inner = self.inner.clone();
        let prompt_id = prompt_id.to_string();
        tokio::task::spawn_blocking(move || {
            inner.update_inbox_status_sync(&prompt_id, status, reason)
        })
        .await
        .map_err(|err| CoreError::Internal(format!("spawn_blocking: {err}")))?
    }

    async fn expire_stale(&self) -> Result<usize, CoreError> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || inner.expire_inbox_sync())
            .await
            .map_err(|err| CoreError::Internal(format!("spawn_blocking: {err}")))?
    }

    async fn last_ack_cursor(&self) -> Result<Option<IntegrationAckCursor>, CoreError> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || Ok(inner.inbox_ack_cursor_sync()))
            .await
            .map_err(|err| CoreError::Internal(format!("spawn_blocking: {err}")))?
    }

    async fn store_ack_cursor(&self, cursor: IntegrationAckCursor) -> Result<(), CoreError> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || inner.store_inbox_ack_cursor_sync(cursor))
            .await
            .map_err(|err| CoreError::Internal(format!("spawn_blocking: {err}")))?
    }
}

#[derive(Clone)]
pub struct FileIntegrationAuditStore {
    inner: Arc<FileIntegrationStateInner>,
}

#[async_trait]
impl IntegrationAuditPort for FileIntegrationAuditStore {
    async fn record_insight_decision(
        &self,
        record: IntegrationInsightAuditRecord,
    ) -> Result<(), CoreError> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || inner.record_audit_sync(record))
            .await
            .map_err(|err| CoreError::Internal(format!("spawn_blocking: {err}")))?
    }

    async fn recent_insight_decisions(
        &self,
        limit: usize,
    ) -> Result<Vec<IntegrationInsightAuditRecord>, CoreError> {
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || Ok(inner.recent_audit_sync(limit)))
            .await
            .map_err(|err| CoreError::Internal(format!("spawn_blocking: {err}")))?
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use oneshim_core::models::integration::{
        InsightSourceWindow, IntegrationCapabilityScope, IntegrationEnvelope,
        IntegrationInboxItemStatus, IntegrationMessageType, IntegrationOrigin,
        IntegrationPrivacyClassification, IntegrationSessionStatus, ProactivePrompt,
        ProactivePromptCategory, ProactivePromptPriority, PromptProvenance,
    };
    use oneshim_core::ports::integration::{
        IntegrationAuditPort, IntegrationInboxStorePort, IntegrationOutboxPort,
        IntegrationSessionStorePort,
    };

    use super::*;

    fn sample_envelope() -> IntegrationEnvelope {
        IntegrationEnvelope {
            envelope_id: "env-1".to_string(),
            schema_version: "integration.envelope.v1".to_string(),
            message_type: IntegrationMessageType::InsightPacket,
            timestamp: Utc::now(),
            nonce: "nonce-1".to_string(),
            origin: IntegrationOrigin {
                device_id: "device-1".to_string(),
                workspace_id: None,
                session_id: Some("session-1".to_string()),
                source: "desktop-client".to_string(),
            },
            capability_scope: IntegrationCapabilityScope::InsightWrite,
        }
    }

    fn sample_packet(packet_id: &str) -> InsightPacket {
        InsightPacket {
            packet_id: packet_id.to_string(),
            summary: "summary".to_string(),
            derived_tags: vec!["focus".to_string()],
            source_window: InsightSourceWindow {
                started_at: Utc::now(),
                ended_at: Utc::now(),
            },
            privacy_classification: IntegrationPrivacyClassification::DerivedSummary,
            audit_reference_id: Some("audit-ref-1".to_string()),
        }
    }

    fn sample_prompt(prompt_id: &str, body: &str) -> StoredProactivePrompt {
        StoredProactivePrompt {
            prompt: ProactivePrompt {
                prompt_id: prompt_id.to_string(),
                category: ProactivePromptCategory::Reminder,
                title: "title".to_string(),
                body: body.to_string(),
                priority: ProactivePromptPriority::Medium,
                actions: Vec::new(),
                expires_at: None,
                provenance: PromptProvenance {
                    source_system: "integration-server".to_string(),
                    source_actor: None,
                    correlation_id: None,
                },
            },
            received_at: Utc::now(),
            status: IntegrationInboxItemStatus::Pending,
            status_updated_at: Utc::now(),
            dismiss_reason: None,
        }
    }

    #[tokio::test]
    async fn integration_session_store_persists_and_clears_state() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store =
            FileIntegrationStateStore::new(temp_dir.path().join("integration.json")).unwrap();
        let session_store = store.session_store();

        session_store
            .store(IntegrationSessionState {
                session_id: "session-1".to_string(),
                device_id: "device-1".to_string(),
                status: IntegrationSessionStatus::Connected,
                transport_kind: Default::default(),
                auth_scheme: Default::default(),
                connected_at: Some(Utc::now()),
                last_heartbeat_at: Some(Utc::now()),
                requested_scopes: vec![IntegrationCapabilityScope::InsightWrite],
                granted_scopes: vec![IntegrationCapabilityScope::InsightWrite],
                ack_cursors: vec![],
            })
            .await
            .unwrap();

        let reloaded =
            FileIntegrationStateStore::new(temp_dir.path().join("integration.json")).unwrap();
        let session = reloaded.session_store().load().await.unwrap().unwrap();
        assert_eq!(session.session_id, "session-1");

        reloaded.session_store().clear().await.unwrap();
        assert!(reloaded.session_store().load().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn integration_outbox_store_roundtrips_queue_and_ack_cursor() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store =
            FileIntegrationStateStore::new(temp_dir.path().join("integration.json")).unwrap();
        let outbox = store.outbox_store();

        let queue_id = outbox
            .enqueue_insight(sample_envelope(), sample_packet("packet-1"))
            .await
            .unwrap();
        let items = outbox.list_pending(10).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].queue_id, queue_id);

        outbox
            .store_ack_cursor(IntegrationAckCursor {
                stream_id: "insights".to_string(),
                cursor: "42".to_string(),
                acknowledged_at: Utc::now(),
            })
            .await
            .unwrap();

        let reloaded =
            FileIntegrationStateStore::new(temp_dir.path().join("integration.json")).unwrap();
        let outbox = reloaded.outbox_store();
        assert_eq!(outbox.list_pending(10).await.unwrap().len(), 1);
        assert_eq!(
            outbox.last_ack_cursor().await.unwrap().unwrap().cursor,
            "42"
        );

        outbox.delete(&[queue_id]).await.unwrap();
        assert!(outbox.list_pending(10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn integration_inbox_store_preserves_lifecycle_and_expires_stale_prompts() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store =
            FileIntegrationStateStore::new(temp_dir.path().join("integration.json")).unwrap();
        let inbox = store.inbox_store();

        let original = sample_prompt("prompt-1", "body-1");
        inbox.upsert_prompts(vec![original]).await.unwrap();
        inbox
            .update_status("prompt-1", IntegrationInboxItemStatus::Acknowledged, None)
            .await
            .unwrap();
        inbox
            .upsert_prompts(vec![sample_prompt("prompt-1", "body-2")])
            .await
            .unwrap();

        assert!(inbox.list_pending().await.unwrap().is_empty());

        let expiring = StoredProactivePrompt {
            prompt: ProactivePrompt {
                expires_at: Some(Utc::now() - Duration::seconds(1)),
                ..sample_prompt("prompt-2", "body-expiring").prompt
            },
            ..sample_prompt("prompt-2", "body-expiring")
        };
        inbox.upsert_prompts(vec![expiring]).await.unwrap();
        assert_eq!(inbox.expire_stale().await.unwrap(), 1);
        assert!(inbox.list_pending().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn integration_audit_store_roundtrips_recent_records() {
        let temp_dir = tempfile::tempdir().unwrap();
        let store =
            FileIntegrationStateStore::new(temp_dir.path().join("integration.json")).unwrap();
        let audit = store.audit_store();

        audit
            .record_insight_decision(IntegrationInsightAuditRecord {
                record_id: "audit-1".to_string(),
                envelope_id: "env-1".to_string(),
                packet_id: "packet-1".to_string(),
                disposition: oneshim_core::models::integration::IntegrationEgressDisposition::Allow,
                reason: None,
                privacy_classification: IntegrationPrivacyClassification::DerivedSummary,
                capability_scope: IntegrationCapabilityScope::InsightWrite,
                occurred_at: Utc::now(),
            })
            .await
            .unwrap();
        audit
            .record_insight_decision(IntegrationInsightAuditRecord {
                record_id: "audit-2".to_string(),
                envelope_id: "env-2".to_string(),
                packet_id: "packet-2".to_string(),
                disposition: oneshim_core::models::integration::IntegrationEgressDisposition::Deny,
                reason: Some("policy denied".to_string()),
                privacy_classification: IntegrationPrivacyClassification::DeviceLocal,
                capability_scope: IntegrationCapabilityScope::InsightWrite,
                occurred_at: Utc::now(),
            })
            .await
            .unwrap();

        let reloaded =
            FileIntegrationStateStore::new(temp_dir.path().join("integration.json")).unwrap();
        let recent = reloaded
            .audit_store()
            .recent_insight_decisions(10)
            .await
            .unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].record_id, "audit-2");
        assert_eq!(recent[1].record_id, "audit-1");
    }
}
