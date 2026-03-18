mod adaptive_trigger;
mod analyzer;
mod assembler;
pub mod calibration_buffer;
mod content_tracker;
pub mod embedding_pipeline;
pub mod focus_shared;
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
pub use segment_summarizer::SegmentSummarizer;
pub use suggestion_filter::filter_by_regime;
pub use title_bar_parser::{ParsedContent, TitleBarParser};
pub use work_type_classifier::WorkTypeClassifier;

pub use embedding_pipeline::EmbeddingPipeline;
pub use llm_segment_summarizer::{LlmSegmentSummarizer, SEGMENT_SUMMARY_PROMPT};
pub use vector_retriever::VectorRetriever;
pub use weekly_digest_generator::WeeklyDigestGenerator;
