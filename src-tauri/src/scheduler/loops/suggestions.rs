use oneshim_suggestion::receiver::SuggestionReceiver;
use std::sync::Arc;
use tracing::info;

/// Spawn the suggestion reception loop.
///
/// Connects to the server SSE stream via `SuggestionReceiver::run()` and
/// keeps receiving suggestions until the stream ends or shutdown is signaled.
#[cfg(feature = "server")]
pub(crate) fn spawn_suggestion_loop(
    receiver: Arc<SuggestionReceiver>,
    session_id: String,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("suggestion reception loop started");
        tokio::select! {
            result = receiver.run(&session_id) => {
                match result {
                    Ok(()) => info!("suggestion stream ended normally"),
                    Err(e) => tracing::warn!("suggestion stream error: {e}"),
                }
            }
            _ = shutdown_rx.changed() => {
                info!("suggestion loop shutdown");
            }
        }
    })
}
