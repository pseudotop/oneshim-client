use serde::{Deserialize, Serialize};

use crate::config::enums::Weekday;
use crate::models::tiered_memory::PresetProfile;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_throttle_secs")]
    pub throttle_secs: u64,
    #[serde(default = "default_interval_secs")]
    pub interval_secs: u64,
    #[serde(default = "default_full_interval_secs")]
    pub full_interval_secs: u64,
    #[serde(default = "default_min_confidence")]
    pub min_confidence: f64,
    #[serde(default = "default_max_suggestions")]
    pub max_suggestions: usize,
    #[serde(default = "default_server_coexistence_lookback_secs")]
    pub server_coexistence_lookback_secs: u64,
    #[serde(default)]
    pub tiered_memory: TieredMemoryConfig,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    #[serde(default)]
    pub gui_intelligence: GuiIntelligenceConfig,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            throttle_secs: default_throttle_secs(),
            interval_secs: default_interval_secs(),
            full_interval_secs: default_full_interval_secs(),
            min_confidence: default_min_confidence(),
            max_suggestions: default_max_suggestions(),
            server_coexistence_lookback_secs: default_server_coexistence_lookback_secs(),
            tiered_memory: TieredMemoryConfig::default(),
            embedding: EmbeddingConfig::default(),
            gui_intelligence: GuiIntelligenceConfig::default(),
        }
    }
}

/// Clustering algorithm selection for regime detection.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClusteringAlgorithm {
    /// HDBSCAN: density-based clustering with automatic k and noise detection.
    #[default]
    Hdbscan,
    /// K-means: centroid-based clustering (legacy fallback).
    Kmeans,
}

/// Configuration for the auto-tuning subsystem (EMA stats + drift detection).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoTuningConfig {
    /// Whether auto-tuning is enabled.
    #[serde(default = "default_auto_tuning_enabled")]
    pub enabled: bool,

    /// Exponential moving average smoothing factor (0 < alpha < 1).
    #[serde(default = "default_ema_alpha")]
    pub ema_alpha: f32,

    /// Drift detection threshold in sigma units.
    #[serde(default = "default_drift_threshold")]
    pub drift_threshold: f32,

    /// Adjusted Rand Index threshold below which re-clustering is triggered.
    #[serde(default = "default_reclustering_ari_threshold")]
    pub reclustering_ari_threshold: f32,
}

impl Default for AutoTuningConfig {
    fn default() -> Self {
        Self {
            enabled: default_auto_tuning_enabled(),
            ema_alpha: default_ema_alpha(),
            drift_threshold: default_drift_threshold(),
            reclustering_ari_threshold: default_reclustering_ari_threshold(),
        }
    }
}

fn default_auto_tuning_enabled() -> bool {
    true
}
fn default_ema_alpha() -> f32 {
    0.05
}
fn default_drift_threshold() -> f32 {
    2.0
}
fn default_reclustering_ari_threshold() -> f32 {
    0.7
}

/// Configuration for the Adaptive Tiered Memory subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TieredMemoryConfig {
    /// Master switch for the entire tiered memory pipeline.
    #[serde(default)]
    pub enabled: bool,

    /// Role-based preset profile used as the base parameter set.
    #[serde(default)]
    pub preset: PresetProfile,

    /// Calibration log retention in days.
    #[serde(default = "default_calibration_retention_days")]
    pub calibration_retention_days: u32,

    /// Maximum rows kept in the calibration_log table.
    #[serde(default = "default_calibration_max_rows")]
    pub calibration_max_rows: u64,

    /// In-memory buffer capacity before flushing to SQLite.
    #[serde(default = "default_buffer_capacity")]
    pub buffer_capacity: usize,

    /// Buffer flush interval in seconds.
    #[serde(default = "default_buffer_flush_interval_secs")]
    pub buffer_flush_interval_secs: u64,

    /// Maximum segment duration in seconds before force-close.
    #[serde(default = "default_max_segment_secs")]
    pub max_segment_secs: u64,

    /// Minimum segment duration in seconds before close is allowed.
    #[serde(default = "default_min_segment_secs")]
    pub min_segment_secs: u64,

    /// Clustering algorithm for regime detection.
    #[serde(default)]
    pub clustering_algorithm: ClusteringAlgorithm,

    /// Auto-tuning configuration (EMA stats + drift detection).
    #[serde(default)]
    pub auto_tuning: AutoTuningConfig,
}

impl Default for TieredMemoryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            preset: PresetProfile::default(),
            calibration_retention_days: default_calibration_retention_days(),
            calibration_max_rows: default_calibration_max_rows(),
            buffer_capacity: default_buffer_capacity(),
            buffer_flush_interval_secs: default_buffer_flush_interval_secs(),
            max_segment_secs: default_max_segment_secs(),
            min_segment_secs: default_min_segment_secs(),
            clustering_algorithm: ClusteringAlgorithm::default(),
            auto_tuning: AutoTuningConfig::default(),
        }
    }
}

fn default_throttle_secs() -> u64 {
    120
}
fn default_interval_secs() -> u64 {
    300
}
fn default_full_interval_secs() -> u64 {
    1800
}
fn default_min_confidence() -> f64 {
    0.6
}
fn default_max_suggestions() -> usize {
    3
}
fn default_server_coexistence_lookback_secs() -> u64 {
    300
}

fn default_calibration_retention_days() -> u32 {
    14
}
fn default_calibration_max_rows() -> u64 {
    500_000
}
fn default_buffer_capacity() -> usize {
    100
}
fn default_buffer_flush_interval_secs() -> u64 {
    30
}
fn default_max_segment_secs() -> u64 {
    600
}
fn default_min_segment_secs() -> u64 {
    120
}

// ---------------------------------------------------------------------------
// Embedding configuration
// ---------------------------------------------------------------------------

/// Embedding provider type for vector generation.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EmbeddingProviderType {
    #[default]
    Local,
    Remote,
}

/// Configuration for the embedding and vector RAG subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Master switch for the embedding pipeline.
    #[serde(default)]
    pub enabled: bool,

    /// Embedding provider type (local fastembed-rs or remote API).
    #[serde(default = "default_embedding_provider")]
    pub provider: EmbeddingProviderType,

    /// Local embedding model identifier (fastembed-rs compatible).
    #[serde(default = "default_local_model")]
    pub local_model: String,

    /// Remote embedding API endpoint (used when provider = Remote).
    #[serde(default)]
    pub remote_endpoint: Option<String>,

    /// Maximum number of search results returned by vector queries.
    #[serde(default = "default_max_search_results")]
    pub max_search_results: usize,

    /// Time decay half-life in hours for vector similarity weighting.
    #[serde(default = "default_time_decay_hours")]
    pub time_decay_hours: f32,

    /// Number of days to retain embedding vectors before cleanup.
    #[serde(default = "default_embedding_retention_days")]
    pub retention_days: u32,

    /// Enable LLM-based segment summarization before embedding.
    #[serde(default)]
    pub llm_summary_enabled: bool,

    /// Minimum segment duration in seconds before generating an LLM summary.
    #[serde(default = "default_min_segment_for_summary")]
    pub min_segment_for_summary_secs: u64,

    /// Day of week to generate the weekly digest.
    #[serde(default = "default_digest_day")]
    pub digest_day: Weekday,

    /// Enable INT8 scalar quantization for 4x storage reduction.
    /// When true, new vectors are stored in both f32 and INT8 formats,
    /// and search uses INT8 cosine similarity.
    #[serde(default)]
    pub quantization_enabled: bool,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_embedding_provider(),
            local_model: default_local_model(),
            remote_endpoint: None,
            max_search_results: default_max_search_results(),
            time_decay_hours: default_time_decay_hours(),
            retention_days: default_embedding_retention_days(),
            llm_summary_enabled: false,
            min_segment_for_summary_secs: default_min_segment_for_summary(),
            digest_day: default_digest_day(),
            quantization_enabled: false,
        }
    }
}

fn default_embedding_provider() -> EmbeddingProviderType {
    EmbeddingProviderType::Local
}
fn default_local_model() -> String {
    "all-MiniLM-L6-v2".to_string()
}
fn default_max_search_results() -> usize {
    5
}
fn default_time_decay_hours() -> f32 {
    168.0 // 1 week
}
fn default_embedding_retention_days() -> u32 {
    90
}
fn default_min_segment_for_summary() -> u64 {
    300 // 5 minutes
}
fn default_digest_day() -> Weekday {
    Weekday::Sun
}

// ---------------------------------------------------------------------------
// GUI Intelligence configuration
// ---------------------------------------------------------------------------

/// Configuration for the GUI Activity Intelligence subsystem (Phase 2).
///
/// **Privacy**: The GUI pipeline MUST be gated on `activity_pattern_learning`
/// consent (GDPR Tier 4) in addition to this config flag. The scheduler
/// must check both `gui_intelligence.enabled` AND consent before constructing
/// `GuiPipelineState`. See `agent_runtime.rs` consent check pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiIntelligenceConfig {
    /// Master switch for GUI intelligence pipeline.
    /// Requires `activity_pattern_learning` consent to actually activate.
    #[serde(default = "default_gui_enabled")]
    pub enabled: bool,

    /// Aggregation time window in seconds. Events within a window are
    /// grouped into a single `GuiActivitySummary`.
    #[serde(default = "default_aggregation_window_secs")]
    pub aggregation_window_secs: u64,

    /// Maximum number of events buffered per segment before force-flush.
    #[serde(default = "default_max_events_per_segment")]
    pub max_events_per_segment: usize,

    /// Pixel distance threshold for proximity fallback in click-to-element
    /// matching. When no direct hit is found, the nearest region within
    /// this distance is used.
    #[serde(default = "default_proximity_threshold_px")]
    pub proximity_threshold_px: u32,
}

impl Default for GuiIntelligenceConfig {
    fn default() -> Self {
        Self {
            enabled: default_gui_enabled(),
            aggregation_window_secs: default_aggregation_window_secs(),
            max_events_per_segment: default_max_events_per_segment(),
            proximity_threshold_px: default_proximity_threshold_px(),
        }
    }
}

fn default_gui_enabled() -> bool {
    false
}
fn default_aggregation_window_secs() -> u64 {
    300
}
fn default_max_events_per_segment() -> usize {
    500
}
fn default_proximity_threshold_px() -> u32 {
    40
}
