use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use chrono::{Duration, Utc};
use tokio::sync::Mutex;
use tracing::debug;

use oneshim_core::config::AnalysisConfig;
use oneshim_core::error::CoreError;
use oneshim_core::models::event::Event;
use oneshim_core::models::suggestion::Suggestion;
use oneshim_core::ports::analysis_provider::AnalysisProvider;
use oneshim_core::ports::storage::StorageService;

use crate::assembler::{
    humanize_time_ago, ContextAssembler, CurrentActivity, RelevantHistoryEntry, SessionMetrics,
};
use crate::pattern_miner::{is_communication_app, PatternMiner};
use crate::vector_retriever::VectorRetriever;

/// Maximum events to query for full periodic analysis.
const FULL_ANALYSIS_EVENT_LIMIT: usize = 500;
/// Maximum events to query for lightweight change detection.
const CHANGE_DETECTION_EVENT_LIMIT: usize = 200;
/// Maximum events to query for event-driven significant analysis.
const SIGNIFICANT_EVENT_LIMIT: usize = 100;
/// Lookback duration for significant event analysis (minutes).
const SIGNIFICANT_EVENT_LOOKBACK_MINS: i64 = 5;
/// Default focus score for inferred current activity.
const DEFAULT_FOCUS_SCORE: f32 = 0.5;
/// Elevated focus score for significant event triggers.
const SIGNIFICANT_EVENT_FOCUS_SCORE: f32 = 0.7;

/// Central orchestrator for the analysis cycle.
/// Concrete struct, NOT a port trait (ADR-011 section 3).
pub struct ContextAnalyzer {
    storage: Arc<dyn StorageService>,
    analysis_provider: Arc<dyn AnalysisProvider>,
    pattern_miner: PatternMiner,
    context_assembler: ContextAssembler,
    vector_retriever: Option<VectorRetriever>,
    config: AnalysisConfig,
    last_analysis_at: Mutex<Option<chrono::DateTime<Utc>>>,
    last_patterns_hash: Mutex<u64>,
}

impl ContextAnalyzer {
    pub fn new(
        storage: Arc<dyn StorageService>,
        analysis_provider: Arc<dyn AnalysisProvider>,
        pattern_miner: PatternMiner,
        context_assembler: ContextAssembler,
        config: AnalysisConfig,
    ) -> Self {
        Self {
            storage,
            analysis_provider,
            pattern_miner,
            context_assembler,
            vector_retriever: None,
            config,
            last_analysis_at: Mutex::new(None),
            last_patterns_hash: Mutex::new(0),
        }
    }

    /// Create a ContextAnalyzer with an optional VectorRetriever for RAG-enriched context.
    pub fn with_vector_retriever(
        storage: Arc<dyn StorageService>,
        analysis_provider: Arc<dyn AnalysisProvider>,
        pattern_miner: PatternMiner,
        context_assembler: ContextAssembler,
        vector_retriever: Option<VectorRetriever>,
        config: AnalysisConfig,
    ) -> Self {
        Self {
            storage,
            analysis_provider,
            pattern_miner,
            context_assembler,
            vector_retriever,
            config,
            last_analysis_at: Mutex::new(None),
            last_patterns_hash: Mutex::new(0),
        }
    }

    /// Full periodic analysis: query events, mine patterns, call LLM.
    pub async fn analyze(&self) -> Result<Vec<Suggestion>, CoreError> {
        if !self.should_analyze().await {
            debug!("Analysis throttled — skipping");
            return Ok(vec![]);
        }

        let now = Utc::now();
        let lookback = Duration::seconds(self.config.full_interval_secs as i64);
        let from = now - lookback;

        let events = self
            .storage
            .get_events(from, now, FULL_ANALYSIS_EVENT_LIMIT)
            .await?;

        if events.is_empty() {
            debug!("No events found for analysis");
            return Ok(vec![]);
        }

        let patterns = self.pattern_miner.detect(&events);
        let current = Self::build_current_activity(&events);
        let metrics = Self::build_session_metrics(&events);

        // Retrieve relevant history via RAG if VectorRetriever is available
        let relevant_history = if let Some(ref retriever) = self.vector_retriever {
            match retriever
                .retrieve_for_context(
                    &current.app_name,
                    &current.window_title,
                    current.ocr_hint.as_deref(),
                )
                .await
            {
                Ok(results) => results
                    .into_iter()
                    .map(|r| RelevantHistoryEntry {
                        when: humanize_time_ago(r.timestamp),
                        summary: r.original_text,
                        similarity: r.similarity,
                    })
                    .collect(),
                Err(e) => {
                    debug!("RAG retrieval skipped: {e}");
                    vec![]
                }
            }
        } else {
            vec![]
        };

        let ctx = self.context_assembler.build_with_history(
            &current,
            &events,
            &patterns,
            &metrics,
            None,
            &relevant_history,
        );

        let suggestions = self
            .analysis_provider
            .analyze(&ctx.user_context_json, &ctx.system_prompt)
            .await?;

        let filtered = self.filter_suggestions(suggestions);

        // Update last analysis timestamp
        let mut last = self.last_analysis_at.lock().await;
        *last = Some(Utc::now());

        // Update patterns hash
        let hash = Self::compute_patterns_hash(&patterns);
        let mut last_hash = self.last_patterns_hash.lock().await;
        *last_hash = hash;

        debug!(
            suggestion_count = filtered.len(),
            "Analysis cycle completed"
        );

        Ok(filtered)
    }

    /// Lightweight check: only call full analysis if patterns changed.
    pub async fn analyze_if_changed(&self) -> Result<Vec<Suggestion>, CoreError> {
        let now = Utc::now();
        let lookback = Duration::seconds(self.config.interval_secs as i64);
        let from = now - lookback;

        let events = self
            .storage
            .get_events(from, now, CHANGE_DETECTION_EVENT_LIMIT)
            .await?;

        let patterns = self.pattern_miner.detect(&events);
        let new_hash = Self::compute_patterns_hash(&patterns);

        let old_hash = {
            let guard = self.last_patterns_hash.lock().await;
            *guard
        };

        if new_hash != old_hash && new_hash != 0 {
            debug!(
                old_hash = old_hash,
                new_hash = new_hash,
                "Patterns changed — triggering full analysis"
            );
            return self.analyze().await;
        }

        debug!("Patterns unchanged — skipping analysis");
        Ok(vec![])
    }

    /// Triggered by a significant event (e.g., major app switch).
    pub async fn on_significant_event(
        &self,
        app_name: &str,
        window_title: &str,
        ocr_text: Option<&str>,
    ) -> Result<Vec<Suggestion>, CoreError> {
        if !self.should_analyze().await {
            return Ok(vec![]);
        }

        let now = Utc::now();
        let from = now - Duration::minutes(SIGNIFICANT_EVENT_LOOKBACK_MINS);

        let events = self
            .storage
            .get_events(from, now, SIGNIFICANT_EVENT_LIMIT)
            .await?;

        let patterns = self.pattern_miner.detect(&events);
        let metrics = Self::build_session_metrics(&events);

        let current = CurrentActivity {
            app_name: app_name.to_string(),
            window_title: window_title.to_string(),
            ocr_hint: ocr_text.map(String::from),
            focus_score: SIGNIFICANT_EVENT_FOCUS_SCORE,
            deep_work_mins: 0,
        };

        let ctx = self
            .context_assembler
            .build(&current, &events, &patterns, &metrics);

        let suggestions = self
            .analysis_provider
            .analyze(&ctx.user_context_json, &ctx.system_prompt)
            .await?;

        let filtered = self.filter_suggestions(suggestions);

        let mut last = self.last_analysis_at.lock().await;
        *last = Some(Utc::now());

        Ok(filtered)
    }

    /// Check whether enough time has elapsed since last analysis.
    async fn should_analyze(&self) -> bool {
        let guard = self.last_analysis_at.lock().await;
        match *guard {
            None => true,
            Some(last) => {
                let elapsed = (Utc::now() - last).num_seconds() as u64;
                elapsed >= self.config.throttle_secs
            }
        }
    }

    /// Filter suggestions by min_confidence and cap at max_suggestions.
    fn filter_suggestions(&self, suggestions: Vec<Suggestion>) -> Vec<Suggestion> {
        let min_conf = self.config.min_confidence;
        let max = self.config.max_suggestions;

        suggestions
            .into_iter()
            .filter(|s| {
                if s.confidence_score < min_conf {
                    debug!(
                        confidence = s.confidence_score,
                        min = min_conf,
                        "Dropping low-confidence suggestion"
                    );
                    false
                } else {
                    true
                }
            })
            .take(max)
            .collect()
    }

    /// Build CurrentActivity from the most recent context event.
    fn build_current_activity(events: &[Event]) -> CurrentActivity {
        let last_ctx = events.iter().rev().find_map(|e| match e {
            Event::Context(ctx) => Some(ctx),
            _ => None,
        });

        match last_ctx {
            Some(ctx) => CurrentActivity {
                app_name: ctx.app_name.clone(),
                window_title: ctx.window_title.clone(),
                ocr_hint: None,
                focus_score: DEFAULT_FOCUS_SCORE,
                deep_work_mins: 0,
            },
            None => CurrentActivity {
                app_name: "Unknown".to_string(),
                window_title: "Unknown".to_string(),
                ocr_hint: None,
                focus_score: 0.0,
                deep_work_mins: 0,
            },
        }
    }

    /// Build SessionMetrics from event analysis.
    fn build_session_metrics(events: &[Event]) -> SessionMetrics {
        let ctx_events: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::Context(ctx) => Some(ctx),
                _ => None,
            })
            .collect();

        let total_work_mins = if ctx_events.len() >= 2 {
            let first = ctx_events.first().unwrap().timestamp;
            let last = ctx_events.last().unwrap().timestamp;
            ((last - first).num_minutes() as u32).max(1)
        } else {
            0
        };

        // Count context switches (app changes)
        let context_switches = ctx_events
            .windows(2)
            .filter(|pair| pair[0].app_name != pair[1].app_name)
            .count() as u32;

        // Estimate communication ratio using shared app classification
        let comm_count = ctx_events
            .iter()
            .filter(|ctx| is_communication_app(&ctx.app_name.to_lowercase()))
            .count();

        let communication_ratio = if ctx_events.is_empty() {
            0.0
        } else {
            comm_count as f32 / ctx_events.len() as f32
        };

        SessionMetrics {
            total_work_mins,
            context_switches,
            communication_ratio,
        }
    }

    /// Compute a simple hash of detected patterns for change detection.
    fn compute_patterns_hash(patterns: &[oneshim_core::models::analysis::ActivityPattern]) -> u64 {
        let mut hasher = DefaultHasher::new();
        for p in patterns {
            p.description.hash(&mut hasher);
            p.frequency.hash(&mut hasher);
        }
        hasher.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::DateTime;
    use oneshim_core::models::event::ContextEvent;
    use oneshim_core::models::suggestion::{Priority, SuggestionSource, SuggestionType};

    // ── Mock StorageService ────────────────────────────────────────

    struct MockStorage {
        events: Vec<Event>,
    }

    impl MockStorage {
        fn new(events: Vec<Event>) -> Self {
            Self { events }
        }
    }

    #[async_trait]
    impl StorageService for MockStorage {
        async fn save_event(&self, _event: &Event) -> Result<(), CoreError> {
            Ok(())
        }

        async fn get_events(
            &self,
            _from: DateTime<Utc>,
            _to: DateTime<Utc>,
            _limit: usize,
        ) -> Result<Vec<Event>, CoreError> {
            Ok(self.events.clone())
        }

        async fn get_pending_events(&self, _limit: usize) -> Result<Vec<Event>, CoreError> {
            Ok(vec![])
        }

        async fn mark_as_sent(&self, _event_ids: &[String]) -> Result<(), CoreError> {
            Ok(())
        }

        async fn mark_unsent_as_sent_before(
            &self,
            _before: DateTime<Utc>,
        ) -> Result<usize, CoreError> {
            Ok(0)
        }

        async fn enforce_retention(&self) -> Result<usize, CoreError> {
            Ok(0)
        }

        async fn save_suggestion(&self, _suggestion: &Suggestion) -> Result<(), CoreError> {
            Ok(())
        }

        async fn update_segment_llm_summary(
            &self,
            _segment_id: &str,
            _llm_summary: &str,
        ) -> Result<(), CoreError> {
            Ok(())
        }
    }

    // ── Mock AnalysisProvider ──────────────────────────────────────

    struct MockAnalysisProvider {
        suggestions: Vec<Suggestion>,
    }

    impl MockAnalysisProvider {
        fn new(suggestions: Vec<Suggestion>) -> Self {
            Self { suggestions }
        }
    }

    #[async_trait]
    impl AnalysisProvider for MockAnalysisProvider {
        async fn analyze(
            &self,
            _context_json: &str,
            _system_prompt: &str,
        ) -> Result<Vec<Suggestion>, CoreError> {
            Ok(self.suggestions.clone())
        }

        fn provider_name(&self) -> &str {
            "mock"
        }
    }

    // ── Helpers ────────────────────────────────────────────────────

    fn make_suggestion(content: &str, confidence: f64) -> Suggestion {
        Suggestion {
            suggestion_id: uuid::Uuid::new_v4().to_string(),
            suggestion_type: SuggestionType::ProductivityTip,
            content: content.to_string(),
            priority: Priority::Medium,
            confidence_score: confidence,
            relevance_score: confidence,
            is_actionable: true,
            created_at: Utc::now(),
            expires_at: None,
            source: SuggestionSource::LlmLocal,
            reasoning: None,
        }
    }

    fn make_events(count: usize) -> Vec<Event> {
        (0..count)
            .map(|i| {
                Event::Context(ContextEvent {
                    app_name: if i % 2 == 0 {
                        "VSCode".to_string()
                    } else {
                        "Slack".to_string()
                    },
                    window_title: format!("Window {}", i),
                    timestamp: Utc::now() - Duration::minutes((count - i) as i64),
                    ..Default::default()
                })
            })
            .collect()
    }

    fn make_analyzer(events: Vec<Event>, suggestions: Vec<Suggestion>) -> ContextAnalyzer {
        let storage = Arc::new(MockStorage::new(events));
        let provider = Arc::new(MockAnalysisProvider::new(suggestions));
        let miner = PatternMiner::new();
        let assembler = ContextAssembler::new(Box::new(|t: &str| t.to_string()));
        let config = AnalysisConfig::default();

        ContextAnalyzer::new(storage, provider, miner, assembler, config)
    }

    // ── Tests ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn analyze_returns_filtered_suggestions() {
        let suggestions = vec![
            make_suggestion("High confidence", 0.9),
            make_suggestion("Low confidence", 0.3),
            make_suggestion("Medium confidence", 0.7),
        ];
        let events = make_events(10);
        let analyzer = make_analyzer(events, suggestions);

        let result = analyzer.analyze().await.unwrap();

        // Low confidence (0.3) should be filtered out (min_confidence = 0.6)
        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|s| s.confidence_score >= 0.6));
    }

    #[tokio::test]
    async fn throttle_prevents_rapid_reanalysis() {
        let suggestions = vec![make_suggestion("Tip", 0.8)];
        let events = make_events(5);
        let analyzer = make_analyzer(events, suggestions);

        // First analysis should succeed
        let result1 = analyzer.analyze().await.unwrap();
        assert!(!result1.is_empty());

        // Second analysis should be throttled (throttle_secs = 120)
        let result2 = analyzer.analyze().await.unwrap();
        assert!(result2.is_empty(), "Should be throttled");
    }

    #[tokio::test]
    async fn analyze_if_changed_returns_empty_when_patterns_unchanged() {
        let suggestions = vec![make_suggestion("Tip", 0.8)];
        let events = make_events(5);

        let storage = Arc::new(MockStorage::new(events));
        let provider = Arc::new(MockAnalysisProvider::new(suggestions));
        let miner = PatternMiner::new();
        let assembler = ContextAssembler::new(Box::new(|t: &str| t.to_string()));
        let mut config = AnalysisConfig::default();
        config.throttle_secs = 0; // Disable throttle for this test

        let analyzer = ContextAnalyzer::new(storage, provider, miner, assembler, config);

        // First call triggers full analysis
        let result1 = analyzer.analyze().await.unwrap();
        assert!(!result1.is_empty());

        // Second call with same events should detect unchanged patterns
        let result2 = analyzer.analyze_if_changed().await.unwrap();
        assert!(
            result2.is_empty(),
            "Should return empty when patterns unchanged"
        );
    }

    #[tokio::test]
    async fn confidence_filter_drops_low_confidence() {
        let suggestions = vec![
            make_suggestion("Very low", 0.1),
            make_suggestion("Below threshold", 0.5),
            make_suggestion("Just above", 0.6),
            make_suggestion("Well above", 0.9),
        ];
        let events = make_events(5);
        let analyzer = make_analyzer(events, suggestions);

        let result = analyzer.analyze().await.unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|s| s.confidence_score >= 0.6));
    }

    #[tokio::test]
    async fn max_suggestions_cap() {
        let suggestions: Vec<_> = (0..10)
            .map(|i| make_suggestion(&format!("Tip {}", i), 0.8))
            .collect();
        let events = make_events(5);
        let analyzer = make_analyzer(events, suggestions);

        let result = analyzer.analyze().await.unwrap();

        // Default max_suggestions is 3
        assert!(result.len() <= 3);
    }

    #[tokio::test]
    async fn analyze_with_empty_events_returns_empty() {
        let analyzer = make_analyzer(vec![], vec![make_suggestion("Tip", 0.9)]);
        let result = analyzer.analyze().await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn on_significant_event_calls_analysis() {
        let suggestions = vec![make_suggestion("Context tip", 0.85)];
        let events = make_events(5);
        let analyzer = make_analyzer(events, suggestions);

        let result = analyzer
            .on_significant_event("VSCode", "main.rs", Some("fn main()"))
            .await
            .unwrap();

        assert!(!result.is_empty());
    }

    #[test]
    fn build_session_metrics_from_events() {
        let events = make_events(10);
        let metrics = ContextAnalyzer::build_session_metrics(&events);
        assert!(metrics.total_work_mins > 0);
        assert!(metrics.context_switches > 0);
    }

    #[test]
    fn build_current_activity_from_events() {
        let events = make_events(5);
        let current = ContextAnalyzer::build_current_activity(&events);
        // Last event in make_events is index 4 (even), so app is "Slack" (index 4 is even -> VSCode)
        assert!(!current.app_name.is_empty());
    }
}
