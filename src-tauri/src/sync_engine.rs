//! SyncEngine -- orchestrates the pull/merge/push sync cycle.
//!
//! This is a wiring-level component (no SQL, no transport logic).
//! It coordinates ChangeExtractor, ChangeMerger, and SyncTransport
//! through the port traits defined in oneshim-core.

use std::sync::Arc;
use tracing::{debug, info};

use oneshim_core::consent::ConsentManager;
use oneshim_core::error::CoreError;
use oneshim_core::models::sync::{ChangeSet, ChangeSetKind, SyncResult};
use oneshim_core::ports::change_extractor::ChangeExtractor;
use oneshim_core::ports::change_merger::ChangeMerger;
use oneshim_core::ports::sync_transport::SyncTransport;
use oneshim_core::sync::Hlc;

#[allow(dead_code)]
pub struct SyncEngine {
    extractor: Arc<dyn ChangeExtractor>,
    merger: Arc<dyn ChangeMerger>,
    transport: Arc<dyn SyncTransport>,
    consent_manager: Arc<parking_lot::Mutex<ConsentManager>>,
    device_id: String,
    device_name: String,
}

#[allow(dead_code)]
impl SyncEngine {
    pub fn new(
        extractor: Arc<dyn ChangeExtractor>,
        merger: Arc<dyn ChangeMerger>,
        transport: Arc<dyn SyncTransport>,
        consent_manager: Arc<parking_lot::Mutex<ConsentManager>>,
        device_id: String,
        device_name: String,
    ) -> Self {
        Self {
            extractor,
            merger,
            transport,
            consent_manager,
            device_id,
            device_name,
        }
    }

    /// Run one complete sync cycle: check consent, handle deletion,
    /// pull + merge, extract + push.
    pub async fn run_cycle(&self) -> Result<Option<SyncResult>, CoreError> {
        // Gate 1: consent check
        {
            let cm = self.consent_manager.lock();
            if !cm.is_permitted(|p| p.cross_device_sync) {
                debug!("sync skipped: cross_device_sync consent not granted");
                return Ok(None);
            }
        }

        // Gate 2: check for pending GDPR deletion
        let has_pending_deletion = {
            let cm = self.consent_manager.lock();
            cm.has_pending_deletion()
        };
        if has_pending_deletion {
            return self.push_deletion_event().await;
        }

        // --- Pull phase ---
        let local_watermark = self.extractor.local_watermark().await?;
        let mut merge_result: Option<SyncResult> = None;

        // Pull changesets in a loop until no more are available
        loop {
            let watermark = merge_result
                .as_ref()
                .map(|r| &r.new_watermark)
                .unwrap_or(&local_watermark);

            match self.transport.pull(watermark).await? {
                None => break,
                Some(changeset) => {
                    info!(
                        origin = %changeset.origin_device_id,
                        rows = changeset.row_count(),
                        "pulled changeset from transport"
                    );
                    let result = self.merger.apply_changes(changeset).await?;
                    debug!(
                        applied = result.applied,
                        skipped_lww = result.skipped_lww,
                        skipped_dup = result.skipped_dup,
                        tombstoned = result.tombstoned,
                        "merge completed"
                    );
                    merge_result = Some(result);
                }
            }
        }

        // --- Push phase ---
        let since = Hlc::default(); // Push all local changes (peers track their own watermarks)
        let local_changes = self.extractor.get_changes_since(&since).await?;

        if !local_changes.is_empty() {
            info!(rows = local_changes.row_count(), "pushing local changes");
            self.transport.push(&local_changes).await?;
        }

        Ok(merge_result)
    }

    /// Push a GDPR Article 17 deletion event and clear the pending flag.
    async fn push_deletion_event(&self) -> Result<Option<SyncResult>, CoreError> {
        info!("pushing GDPR Article 17 deletion event");

        let deletion_cs = ChangeSet {
            kind: ChangeSetKind::DeletionEvent,
            origin_device_id: self.device_id.clone(),
            origin_device_name: self.device_name.clone(),
            watermark: Hlc::now(&self.device_id),
            ..Default::default()
        };

        self.transport.push(&deletion_cs).await?;

        // Clear the pending deletion flag only after successful push
        {
            let mut cm = self.consent_manager.lock();
            cm.clear_pending_deletion();
        }

        info!("GDPR deletion event pushed successfully");
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use oneshim_core::consent::ConsentPermissions;
    use oneshim_core::models::sync::PeerInfo;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // --- Mock implementations ---

    struct MockExtractor {
        changeset: ChangeSet,
    }

    #[async_trait]
    impl ChangeExtractor for MockExtractor {
        async fn get_changes_since(&self, _since: &Hlc) -> Result<ChangeSet, CoreError> {
            Ok(self.changeset.clone())
        }
        async fn local_watermark(&self) -> Result<Hlc, CoreError> {
            Ok(Hlc::default())
        }
    }

    struct MockMerger {
        apply_count: AtomicUsize,
    }

    #[async_trait]
    impl ChangeMerger for MockMerger {
        async fn apply_changes(&self, _changes: ChangeSet) -> Result<SyncResult, CoreError> {
            self.apply_count.fetch_add(1, Ordering::SeqCst);
            Ok(SyncResult {
                applied: 1,
                ..Default::default()
            })
        }
    }

    struct MockTransport {
        pull_result: std::sync::Mutex<Vec<Option<ChangeSet>>>,
        push_count: AtomicUsize,
    }

    #[async_trait]
    impl SyncTransport for MockTransport {
        async fn push(&self, _changes: &ChangeSet) -> Result<(), CoreError> {
            self.push_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn pull(&self, _since: &Hlc) -> Result<Option<ChangeSet>, CoreError> {
            let mut results = self.pull_result.lock().unwrap();
            if results.is_empty() {
                Ok(None)
            } else {
                Ok(results.remove(0))
            }
        }
        async fn discover_peers(&self) -> Result<Vec<PeerInfo>, CoreError> {
            Ok(vec![])
        }
    }

    fn make_consent_manager(sync_granted: bool) -> Arc<parking_lot::Mutex<ConsentManager>> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("consent.json");
        let mut cm = ConsentManager::new(path);
        if sync_granted {
            cm.grant_consent(
                ConsentPermissions {
                    cross_device_sync: true,
                    ..Default::default()
                },
                30,
            )
            .unwrap();
        }
        // Leak the tempdir to keep the path alive
        std::mem::forget(dir);
        Arc::new(parking_lot::Mutex::new(cm))
    }

    #[tokio::test]
    async fn cycle_skipped_when_consent_not_granted() {
        let engine = SyncEngine::new(
            Arc::new(MockExtractor {
                changeset: ChangeSet::default(),
            }),
            Arc::new(MockMerger {
                apply_count: AtomicUsize::new(0),
            }),
            Arc::new(MockTransport {
                pull_result: std::sync::Mutex::new(vec![]),
                push_count: AtomicUsize::new(0),
            }),
            make_consent_manager(false),
            "dev-a".to_string(),
            "Test".to_string(),
        );

        let result = engine.run_cycle().await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn normal_pull_merge_push_cycle() {
        let remote_cs = ChangeSet {
            origin_device_id: "dev-b".to_string(),
            origin_device_name: "Remote".to_string(),
            segments: vec![serde_json::json!({"id": "seg-1"})],
            ..Default::default()
        };

        let merger = Arc::new(MockMerger {
            apply_count: AtomicUsize::new(0),
        });
        let transport = Arc::new(MockTransport {
            pull_result: std::sync::Mutex::new(vec![Some(remote_cs), None]),
            push_count: AtomicUsize::new(0),
        });

        let engine = SyncEngine::new(
            Arc::new(MockExtractor {
                changeset: ChangeSet {
                    segments: vec![serde_json::json!({"id": "local-seg"})],
                    origin_device_id: "dev-a".to_string(),
                    ..Default::default()
                },
            }),
            merger.clone(),
            transport.clone(),
            make_consent_manager(true),
            "dev-a".to_string(),
            "Test".to_string(),
        );

        let result = engine.run_cycle().await.unwrap();
        assert!(result.is_some());
        assert_eq!(merger.apply_count.load(Ordering::SeqCst), 1);
        assert!(transport.push_count.load(Ordering::SeqCst) >= 1);
    }

    #[tokio::test]
    async fn empty_pull_results_in_push_only() {
        let transport = Arc::new(MockTransport {
            pull_result: std::sync::Mutex::new(vec![]),
            push_count: AtomicUsize::new(0),
        });
        let merger = Arc::new(MockMerger {
            apply_count: AtomicUsize::new(0),
        });

        let engine = SyncEngine::new(
            Arc::new(MockExtractor {
                changeset: ChangeSet {
                    segments: vec![serde_json::json!({"id": "local-seg"})],
                    origin_device_id: "dev-a".to_string(),
                    ..Default::default()
                },
            }),
            merger.clone(),
            transport.clone(),
            make_consent_manager(true),
            "dev-a".to_string(),
            "Test".to_string(),
        );

        let result = engine.run_cycle().await.unwrap();
        assert!(result.is_none()); // no merge happened
        assert_eq!(merger.apply_count.load(Ordering::SeqCst), 0);
        assert_eq!(transport.push_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn deletion_event_pushed_when_pending() {
        let consent_mgr = make_consent_manager(true);
        // Revoke consent to trigger pending deletion
        {
            let mut cm = consent_mgr.lock();
            cm.revoke_consent().unwrap();
            // Re-grant consent WITH cross_device_sync so the consent gate passes,
            // but the pending_deletion flag remains true.
            cm.grant_consent(
                ConsentPermissions {
                    cross_device_sync: true,
                    ..Default::default()
                },
                30,
            )
            .unwrap();
        }

        let transport = Arc::new(MockTransport {
            pull_result: std::sync::Mutex::new(vec![]),
            push_count: AtomicUsize::new(0),
        });

        let engine = SyncEngine::new(
            Arc::new(MockExtractor {
                changeset: ChangeSet::default(),
            }),
            Arc::new(MockMerger {
                apply_count: AtomicUsize::new(0),
            }),
            transport.clone(),
            consent_mgr.clone(),
            "dev-a".to_string(),
            "Test".to_string(),
        );

        let result = engine.run_cycle().await.unwrap();
        assert!(result.is_none());
        assert_eq!(transport.push_count.load(Ordering::SeqCst), 1);

        // pending_deletion should be cleared
        assert!(!consent_mgr.lock().has_pending_deletion());
    }
}
