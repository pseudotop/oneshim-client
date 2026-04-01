use std::sync::Arc;
use tracing::{info, warn};

use oneshim_core::config::AppConfig;
use oneshim_core::consent::ConsentManager;
use oneshim_core::ports::calibration_store::{CalibrationReader, CalibrationWriter};

use crate::scheduler::AdaptiveTriggerState;

use super::embedding_setup::EmbeddingComponents;

/// Result of the analysis pipeline setup.
pub(super) struct AnalysisResult {
    /// Populated when tiered-memory is enabled, consented, and calibration stores are present.
    pub adaptive_trigger_state: Option<AdaptiveTriggerState>,
}

/// Build the tiered-memory analysis pipeline (AdaptiveTriggerState).
///
/// Requires: embedding components (from `build_embedding_components`), consent for
/// activity_pattern_learning, and calibration reader/writer stores.
pub(super) fn build_analysis_pipeline(
    config: &AppConfig,
    consent_manager: &Option<Arc<ConsentManager>>,
    calibration_writer: Option<Arc<dyn CalibrationWriter>>,
    calibration_reader: Option<Arc<dyn CalibrationReader>>,
    override_store: Option<Arc<dyn oneshim_core::ports::override_store::OverrideStore>>,
    recluster_requested: Arc<std::sync::atomic::AtomicBool>,
    embedding: &mut EmbeddingComponents,
) -> AnalysisResult {
    // Config validation: embedding requires tiered_memory
    if config.analysis.embedding.enabled && !config.analysis.tiered_memory.enabled {
        warn!("embedding.enabled requires tiered_memory.enabled — embedding will not function");
    }

    // Config validation: gui_intelligence requires tiered_memory
    if config.analysis.gui_intelligence.enabled && !config.analysis.tiered_memory.enabled {
        warn!("gui_intelligence.enabled requires tiered_memory.enabled — GUI pipeline will not function");
    }

    // Gate on activity_pattern_learning consent (GDPR Tier 4).
    let consent_ok = consent_manager
        .as_ref()
        .and_then(|cm| cm.current_consent())
        .map(|c| c.permissions.activity_pattern_learning)
        .unwrap_or(false);

    if config.analysis.tiered_memory.enabled && !consent_ok {
        info!("activity_pattern_learning consent not granted, skipping tiered memory");
    }

    if config.analysis.tiered_memory.enabled && consent_ok {
        if let (Some(calibration_writer), Some(calibration_reader)) =
            (calibration_writer, calibration_reader)
        {
            let preset = config.analysis.tiered_memory.preset;
            let params = preset.default_params();
            let buf_cap = config.analysis.tiered_memory.buffer_capacity;
            let tm_config = &config.analysis.tiered_memory;
            let llm_work_type_refiner = embedding
                .llm_refiner_provider
                .take()
                .map(|provider| Arc::new(oneshim_analysis::LlmWorkTypeRefiner::new(provider)));
            if llm_work_type_refiner.is_none() {
                info!(
                    "LLM WorkType refiner disabled — requires: \
                     analysis.embedding.enabled=true, \
                     analysis.embedding.llm_summary_enabled=true, \
                     and a configured ai_provider.llm_api"
                );
            }
            let state = AdaptiveTriggerState {
                trigger: oneshim_analysis::AdaptiveTrigger::new(),
                segment_buffer: oneshim_analysis::SegmentBuffer::new(buf_cap),
                calibration_buffer: oneshim_analysis::CalibrationBuffer::new(buf_cap, 60),
                title_bar_parser: oneshim_analysis::TitleBarParser::new(),
                work_type_classifier: oneshim_analysis::WorkTypeClassifier::new(),
                content_tracker: oneshim_analysis::ContentTracker::new(),
                segment_summarizer: oneshim_analysis::SegmentSummarizer::new(),
                params,
                calibration_writer,
                regime_classifier: oneshim_analysis::RegimeClassifier::new(1.5),
                regime_manager: oneshim_analysis::RegimeManager::new(tm_config),
                regime_detector: oneshim_analysis::RegimeDetector::new(),
                param_resolver: oneshim_analysis::ParamResolver::new(preset),
                calibration_reader,
                current_regime_id: None,
                last_detection_time: None,
                ema_tracker: oneshim_analysis::auto_tuner::EmaStatsTracker::new(
                    tm_config.auto_tuning.ema_alpha,
                ),
                drift_detector: oneshim_analysis::auto_tuner::DriftDetector::new(
                    tm_config.auto_tuning.ema_alpha,
                    tm_config.auto_tuning.drift_threshold,
                ),
                auto_tune_tick_count: 0,
                regime_analysis: Some(oneshim_analysis::RegimeAnalysisFacade::new(
                    tm_config.clustering_algorithm.clone(),
                )),
                override_store,
                recluster_requested,
                last_drift_detected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                llm_summarizer: embedding.llm_summarizer.take(),
                embedding_pipeline: embedding.embedding_pipeline.take(),
                gui_pipeline_state: None,
                gui_work_type_refiner: oneshim_analysis::GuiWorkTypeRefiner,
                llm_work_type_refiner,
                app_registry: Arc::new(oneshim_core::app_registry::AppRegistry::new()),
                heatmap_aggregator: crate::scheduler::heatmap::HeatmapAggregator::new(),
            };
            info!("Adaptive tiered-memory pipeline enabled");
            return AnalysisResult {
                adaptive_trigger_state: Some(state),
            };
        } else {
            info!("Tiered memory enabled but no calibration writer/reader — skipped");
        }
    }

    AnalysisResult {
        adaptive_trigger_state: None,
    }
}
