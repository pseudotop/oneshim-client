mod adaptive_trigger;
mod analyzer;
mod assembler;
pub mod auto_tuner;
pub mod calibration_buffer;
pub mod clustering_strategy;
pub mod constraint_builder;
pub mod content_tracker;
pub mod daily_digest_generator;
pub mod daily_insight_generator;
pub mod embedding_pipeline;
pub mod focus_shared;
pub mod gui_aggregator;
pub mod gui_work_type_refiner;
pub mod hdbscan_detector;
pub mod hybrid_search_service;
pub mod kmeans_adapter;
pub mod llm_segment_summarizer;
pub mod param_resolver;
mod pattern_miner;
mod prompts;
pub mod regime_classifier;
pub mod regime_detector;
pub mod regime_manager;
pub mod segment_buffer;
mod segment_summarizer;
pub mod suggestion_filter;
mod title_bar_parser;
pub mod vector_retriever;
pub mod weekly_digest_generator;
mod work_type_classifier;

pub use adaptive_trigger::{AdaptiveTrigger, TriggerDecision};
pub use analyzer::ContextAnalyzer;
pub use assembler::{
    humanize_time_ago, AnalysisContext, ContentSummaryEntry, ContextAssembler, CurrentActivity,
    PiiFilter, RelevantHistoryEntry, SegmentStats, SessionMetrics,
};
pub use calibration_buffer::CalibrationBuffer;
pub use content_tracker::ContentTracker;
pub use param_resolver::ParamResolver;
pub use pattern_miner::PatternMiner;
pub use prompts::ANALYSIS_SYSTEM_PROMPT;
pub use regime_classifier::RegimeClassifier;
pub use regime_detector::RegimeDetector;
pub use regime_manager::RegimeManager;
pub use segment_buffer::SegmentBuffer;
pub use segment_summarizer::{to_content_summary_entries, SegmentSummarizer};
pub use suggestion_filter::filter_by_regime;
pub use title_bar_parser::{ParsedContent, TitleBarParser};
pub use work_type_classifier::WorkTypeClassifier;

// Priority 2: Accuracy Improvements re-exports
pub use auto_tuner::{DriftDetector, EmaStatsTracker};
pub use clustering_strategy::{ClusterAssignment, ClusteringResult, ClusteringStrategy};

pub use daily_digest_generator::DailyDigestGenerator;
pub use daily_insight_generator::DailyInsightGenerator;
pub use embedding_pipeline::EmbeddingPipeline;
pub use gui_aggregator::GuiActivityAggregator;
pub use gui_work_type_refiner::GuiWorkTypeRefiner;
pub use hybrid_search_service::{HybridSearchService, SearchMode};
pub use llm_segment_summarizer::{LlmSegmentSummarizer, SEGMENT_SUMMARY_PROMPT};
pub use vector_retriever::VectorRetriever;
pub use weekly_digest_generator::WeeklyDigestGenerator;
