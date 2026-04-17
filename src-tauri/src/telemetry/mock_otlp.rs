//! Test-only mock OTLP collector.
//!
//! Starts a minimal Axum server on a random loopback port that accepts POSTs
//! to `/v1/traces` and forwards the bytes via an `mpsc::UnboundedChannel`.
//! Used by T-X2-3 (pipeline builds against a real endpoint) and T-X2-10
//! (end-to-end span reaches the collector).

#![cfg(all(test, feature = "telemetry"))]

use axum::{body::Bytes, http::StatusCode, routing::post, Router};
use std::sync::Arc;
use tokio::sync::mpsc;

pub(super) struct MockCollector {
    pub endpoint: String,
    pub rx: mpsc::UnboundedReceiver<Vec<u8>>,
}

pub(super) async fn start() -> MockCollector {
    let (tx, rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let tx = Arc::new(tx);

    let app = Router::new().route(
        "/v1/traces",
        post({
            let tx = Arc::clone(&tx);
            move |body: Bytes| async move {
                let _ = tx.send(body.to_vec());
                StatusCode::OK
            }
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    MockCollector {
        endpoint: format!("http://{addr}"),
        rx,
    }
}
