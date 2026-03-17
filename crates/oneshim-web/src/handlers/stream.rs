use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use std::convert::Infallible;
use std::time::Duration;

use crate::services::stream_service::RealtimeStreamQueryService;
use crate::services::web_contexts::RealtimeStreamWebContext;

/// GET /api/stream
pub async fn event_stream(
    State(context): State<RealtimeStreamWebContext>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let sse_stream = RealtimeStreamQueryService::new(context).event_stream();

    Sse::new(sse_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}
