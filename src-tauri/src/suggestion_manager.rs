use lru::LruCache;
use oneshim_suggestion::feedback::FeedbackSender;
use oneshim_suggestion::history::SuggestionHistory;
use oneshim_suggestion::queue::SuggestionQueue;
use oneshim_suggestion::scorer::FeedbackScorer;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Maximum number of read-status entries to track. More than enough for
/// max 50 queue items + some history overlap. Using an LRU cache prevents
/// unbounded growth when suggestions are continuously received.
#[allow(dead_code)] // wired in app_runtime_launch; IPC commands access via AppState
const READ_IDS_CAPACITY: usize = 200;

/// Thin wrapper providing unified access to suggestion pipeline components.
/// CRITICAL: `queue` and `history` must be the SAME Arc instances passed
/// to SuggestionReceiver, so SSE-received suggestions appear in IPC queries.
#[allow(dead_code)] // wired in app_runtime_launch; IPC commands access via AppState
pub struct SuggestionManager {
    queue: Arc<Mutex<SuggestionQueue>>,
    history: Arc<Mutex<SuggestionHistory>>,
    feedback: FeedbackSender,
    read_ids: Mutex<LruCache<String, ()>>,
    scorer: Arc<Mutex<FeedbackScorer>>,
}

#[allow(dead_code)] // wired in app_runtime_launch
impl SuggestionManager {
    pub fn new(
        queue: Arc<Mutex<SuggestionQueue>>,
        history: Arc<Mutex<SuggestionHistory>>,
        feedback: FeedbackSender,
        scorer: Arc<Mutex<FeedbackScorer>>,
    ) -> Self {
        Self {
            queue,
            history,
            feedback,
            read_ids: Mutex::new(LruCache::new(
                NonZeroUsize::new(READ_IDS_CAPACITY).expect("non-zero capacity"),
            )),
            scorer,
        }
    }

    pub fn queue(&self) -> &Arc<Mutex<SuggestionQueue>> {
        &self.queue
    }

    pub fn history(&self) -> &Arc<Mutex<SuggestionHistory>> {
        &self.history
    }

    pub fn feedback(&self) -> &FeedbackSender {
        &self.feedback
    }

    pub fn scorer(&self) -> &Arc<Mutex<FeedbackScorer>> {
        &self.scorer
    }

    pub async fn mark_read(&self, suggestion_id: &str) {
        self.read_ids
            .lock()
            .await
            .put(suggestion_id.to_string(), ());
    }

    pub async fn is_read(&self, suggestion_id: &str) -> bool {
        self.read_ids.lock().await.contains(suggestion_id)
    }
}
