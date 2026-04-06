use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use oneshim_analysis::{FallbackAnalysisProvider, NoOpAnalysisProvider};
use oneshim_core::config::AiProviderConfig;
use oneshim_core::ports::analysis_provider::AnalysisProvider;
use oneshim_network::analysis_client::AnalysisClient;

/// Build an AnalysisProvider with automatic fallback chaining.
///
/// Returns `None` when no primary `llm_api` is configured.
/// The returned `Arc<AtomicBool>` tracks primary provider health and can be
/// stored in AppState for IPC health queries.
#[allow(dead_code)] // DI helper — wired in future AppState integration
pub fn build_analysis_provider(
    config: &AiProviderConfig,
) -> Option<(Arc<dyn AnalysisProvider>, Arc<AtomicBool>)> {
    let llm_api = config.llm_api.as_ref()?;
    let primary: Arc<dyn AnalysisProvider> = Arc::new(AnalysisClient::new(llm_api));

    let fallback: Arc<dyn AnalysisProvider> = match config.llm_api_fallback.as_ref() {
        Some(api) => Arc::new(AnalysisClient::new(api)),
        None => Arc::new(NoOpAnalysisProvider),
    };

    let health_flag = Arc::new(AtomicBool::new(true));
    let provider = Arc::new(FallbackAnalysisProvider::new_with_flag(
        primary,
        fallback,
        health_flag.clone(),
    ));

    Some((provider as Arc<dyn AnalysisProvider>, health_flag))
}
