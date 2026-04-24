//! `AuditLayer` tower middleware — records Started before handler call and
//! Completed/Denied/Timeout/Failed after handler returns.
//!
//! Positioned INNER of AuthLayer in the tonic `Server::builder()` chain so
//! only auth-passed requests reach it. AuthLayer still records `Failed` on
//! auth rejection (request never enters AuditLayer). AuditLayer owns the
//! terminal-status record for auth-passed requests.
//!
//! MVP terminal status: every auth-passed request that returns without a
//! panic is recorded as `Completed`. A follow-up (spec §8 open question)
//! can introspect `grpc-status` trailers to distinguish Denied / Timeout /
//! Failed on a per-response basis.

use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;

use oneshim_core::models::audit::AuditStatus;
use tower::{Layer, Service};

use super::audit_bridge::AuditBridge;
use super::conn_info::{AuthContext, PeerInfo};
use super::metrics::ExternalMetrics;

// counting_stream lives one level up (crate::grpc::counting_stream) so that
// the loopback subscribe_metrics / subscribe_events handlers can also wrap
// their outbound streams without depending on the feature-gated `external`
// submodule.

#[derive(Clone)]
pub(crate) struct AuditLayer {
    pub bridge: Arc<AuditBridge>,
    pub metrics: Arc<ExternalMetrics>,
}

impl<S: Clone> Layer<S> for AuditLayer {
    type Service = AuditService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        AuditService {
            inner,
            bridge: self.bridge.clone(),
            metrics: self.metrics.clone(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct AuditService<S> {
    inner: S,
    bridge: Arc<AuditBridge>,
    metrics: Arc<ExternalMetrics>,
}

impl<S, B, RespBody> Service<http::Request<B>> for AuditService<S>
where
    S: Service<
            http::Request<B>,
            Response = http::Response<RespBody>,
            Error = std::convert::Infallible,
        > + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
    RespBody: Send + 'static,
{
    type Response = http::Response<RespBody>;
    type Error = std::convert::Infallible;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: http::Request<B>) -> Self::Future {
        let mut inner = self.inner.clone();
        let bridge = self.bridge.clone();
        let metrics = self.metrics.clone();

        // Extract context (AuthLayer already inserted it at this point).
        let auth_ctx: Option<AuthContext> = req.extensions().get::<AuthContext>().cloned();
        let peer: Option<PeerInfo> = req.extensions().get::<PeerInfo>().cloned();
        let operation = req.uri().path().to_string();

        // Insert counter for streaming handlers to populate via CountingStream.
        let msg_counter = Arc::new(AtomicU64::new(0));
        req.extensions_mut().insert(msg_counter.clone());

        Box::pin(async move {
            // Fallthrough: if either extension is missing, the request was not
            // auth-processed (direct handler invocation in tests, internal
            // plumbing) — skip audit entirely.
            let Some(ctx) = auth_ctx else {
                return inner.call(req).await;
            };
            let Some(peer) = peer else {
                return inner.call(req).await;
            };

            let remote = peer.remote_addr.to_string();

            // Record Started (responsibility moved from AuthLayer per Task 7).
            let _ = bridge
                .record(
                    &ctx,
                    remote.clone(),
                    &operation,
                    "ok",
                    AuditStatus::Started,
                    std::time::Duration::ZERO,
                    None,
                    None,
                    None,
                    None,
                )
                .await;

            let start = Instant::now();
            let response = inner.call(req).await?;
            let duration = start.elapsed();

            // MVP: every auth-passed, non-panicking request → Completed.
            // Follow-up (spec §8): parse grpc-status trailer to distinguish
            // Denied / Timeout / Failed.
            let status = AuditStatus::Completed;
            let msg_count = msg_counter.load(std::sync::atomic::Ordering::Relaxed);
            let msg_count_opt = if msg_count > 0 { Some(msg_count) } else { None };

            let _ = bridge
                .record_completion(
                    &ctx,
                    remote,
                    &operation,
                    status,
                    duration,
                    msg_count_opt,
                    None,
                    None,
                    None,
                )
                .await;

            metrics.request_bump("external", ctx.auth_type.as_str(), "ok");
            Ok(response)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grpc::external::audit_bridge::AuditBridge;
    use crate::grpc::external::conn_info::{AuthContext, AuthType, PeerInfo};
    use async_trait::async_trait;
    use http::{Request, Response};
    use oneshim_core::models::ai_session::SessionAuditEntry;
    use oneshim_core::models::audit::{AuditEntry, AuditLevel, AuditStats};
    use oneshim_core::ports::audit_log::AuditLogPort;
    use std::convert::Infallible;
    use std::net::SocketAddr;
    use std::sync::Mutex;
    use tower::ServiceExt;
    use ulid::Ulid;

    // ── Mock inner service — returns a preset Response<Vec<u8>> ────────────
    #[derive(Clone)]
    struct MockInner {
        body: &'static [u8],
    }

    impl Service<Request<Vec<u8>>> for MockInner {
        type Response = Response<Vec<u8>>;
        type Error = Infallible;
        type Future = std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
        >;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, _req: Request<Vec<u8>>) -> Self::Future {
            let body = self.body.to_vec();
            Box::pin(async move { Ok(Response::builder().status(200).body(body).unwrap()) })
        }
    }

    // ── Capturing audit port — records every log_complete_with_time call ───
    struct CapturingAudit {
        entries: Mutex<Vec<AuditEntry>>,
    }
    impl CapturingAudit {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                entries: Mutex::new(vec![]),
            })
        }
    }
    #[async_trait]
    impl AuditLogPort for CapturingAudit {
        async fn pending_count(&self) -> usize {
            0
        }
        async fn recent_entries(&self, _l: usize) -> Vec<AuditEntry> {
            vec![]
        }
        async fn entries_by_status(&self, _s: &AuditStatus, _l: usize) -> Vec<AuditEntry> {
            vec![]
        }
        async fn entries_by_action_prefix(&self, _p: &str, _l: usize) -> Vec<AuditEntry> {
            vec![]
        }
        async fn entries_by_command_id(&self, _cmd_id: &str, _limit: usize) -> Vec<AuditEntry> {
            vec![]
        }
        async fn stats(&self) -> AuditStats {
            AuditStats::default()
        }
        async fn has_pending_batch(&self) -> bool {
            false
        }
        async fn log_event(&self, _a: &str, _s: &str, _d: &str) {}
        async fn log_start_if(&self, _l: AuditLevel, _c: &str, _s: &str, _a: &str) {}
        async fn log_complete_with_time(
            &self,
            _level: AuditLevel,
            command_id: &str,
            session_id: &str,
            details: &str,
            execution_time_ms: u64,
        ) {
            // Infer status from the JSON `result` field so tests can branch on it.
            let status = serde_json::from_str::<serde_json::Value>(details)
                .ok()
                .and_then(|v| {
                    v.get("result").and_then(|r| r.as_str()).map(|r| match r {
                        "ok" => AuditStatus::Completed,
                        "denied" => AuditStatus::Denied,
                        "timeout" => AuditStatus::Timeout,
                        _ => AuditStatus::Failed,
                    })
                })
                .unwrap_or(AuditStatus::Completed);

            self.entries.lock().unwrap().push(AuditEntry {
                entry_id: Ulid::new().to_string(),
                timestamp: chrono::Utc::now(),
                action_type: "external_grpc".into(),
                command_id: command_id.into(),
                session_id: session_id.into(),
                status,
                details: Some(details.into()),
                execution_time_ms: Some(execution_time_ms),
            });
        }
        async fn drain_batch(&self) -> Vec<AuditEntry> {
            vec![]
        }
        async fn drain_all(&self) -> Vec<AuditEntry> {
            vec![]
        }
        async fn record_session_event(&self, _e: SessionAuditEntry) {}
    }

    fn mk_ctx() -> AuthContext {
        AuthContext {
            command_id: Ulid::new().to_string(),
            client_id: "client-1".into(),
            auth_type: AuthType::Jwt,
            jti: None,
        }
    }

    fn mk_peer() -> PeerInfo {
        PeerInfo {
            remote_addr: "127.0.0.1:5000".parse::<SocketAddr>().unwrap(),
            peer_cert_der: None,
            cert_subject_cn: None,
            tls_version: "TLSv1.3".into(),
        }
    }

    fn mk_layer(audit: Arc<CapturingAudit>) -> AuditLayer {
        AuditLayer {
            bridge: Arc::new(AuditBridge::new(audit as Arc<dyn AuditLogPort>)),
            metrics: Arc::new(ExternalMetrics::new()),
        }
    }

    #[tokio::test]
    async fn ok_response_records_started_then_completed() {
        let audit = CapturingAudit::new();
        let layer = mk_layer(audit.clone());
        let mut svc = layer.layer(MockInner { body: b"response" });

        let mut req = Request::builder()
            .uri("/dashboard.v1.DashboardService/GetSessionStats")
            .body(vec![])
            .unwrap();
        req.extensions_mut().insert(mk_ctx());
        req.extensions_mut().insert(mk_peer());

        let resp = svc.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(resp.status(), 200);

        let entries = audit.entries.lock().unwrap();
        assert_eq!(entries.len(), 2, "expected Started + Completed audit rows");
        let first: serde_json::Value =
            serde_json::from_str(entries[0].details.as_ref().unwrap()).unwrap();
        let second: serde_json::Value =
            serde_json::from_str(entries[1].details.as_ref().unwrap()).unwrap();
        // Started row uses result = "ok" and has no message_count.
        assert_eq!(first["result"], "ok");
        assert!(first.get("response_message_count").is_none());
        // Completed row mirrors result = "ok" with no count (non-streaming).
        assert_eq!(second["result"], "ok");
    }

    #[tokio::test]
    async fn missing_auth_context_skips_audit() {
        let audit = CapturingAudit::new();
        let layer = mk_layer(audit.clone());
        let mut svc = layer.layer(MockInner { body: b"body" });

        // No AuthContext inserted — fallthrough path.
        let req = Request::builder().uri("/svc/op").body(vec![]).unwrap();
        let resp = svc.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(audit.entries.lock().unwrap().len(), 0, "no auth → no audit");
    }

    #[tokio::test]
    async fn missing_peer_info_skips_audit() {
        let audit = CapturingAudit::new();
        let layer = mk_layer(audit.clone());
        let mut svc = layer.layer(MockInner { body: b"body" });
        let mut req = Request::builder().uri("/svc/op").body(vec![]).unwrap();
        req.extensions_mut().insert(mk_ctx());
        // No PeerInfo extension.
        let resp = svc.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(audit.entries.lock().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn message_counter_extension_is_inserted_before_handler() {
        #[derive(Clone)]
        struct CheckExt;
        impl Service<Request<Vec<u8>>> for CheckExt {
            type Response = Response<Vec<u8>>;
            type Error = Infallible;
            type Future = std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
            >;
            fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }
            fn call(&mut self, req: Request<Vec<u8>>) -> Self::Future {
                let has = req.extensions().get::<Arc<AtomicU64>>().is_some();
                Box::pin(async move {
                    let body = if has {
                        b"ok".to_vec()
                    } else {
                        b"missing".to_vec()
                    };
                    Ok(Response::builder().status(200).body(body).unwrap())
                })
            }
        }
        let audit = CapturingAudit::new();
        let layer = mk_layer(audit.clone());
        let mut svc = layer.layer(CheckExt);

        let mut req = Request::builder().uri("/svc/op").body(vec![]).unwrap();
        req.extensions_mut().insert(mk_ctx());
        req.extensions_mut().insert(mk_peer());
        let resp = svc.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(resp.body().as_slice(), b"ok");
    }

    #[tokio::test]
    async fn completed_entry_has_duration_ge_handler_elapsed() {
        #[derive(Clone)]
        struct SlowInner;
        impl Service<Request<Vec<u8>>> for SlowInner {
            type Response = Response<Vec<u8>>;
            type Error = Infallible;
            type Future = std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
            >;
            fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }
            fn call(&mut self, _req: Request<Vec<u8>>) -> Self::Future {
                Box::pin(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    Ok(Response::builder().status(200).body(vec![]).unwrap())
                })
            }
        }

        let audit = CapturingAudit::new();
        let layer = mk_layer(audit.clone());
        let mut svc = layer.layer(SlowInner);

        let mut req = Request::builder().uri("/svc/op").body(vec![]).unwrap();
        req.extensions_mut().insert(mk_ctx());
        req.extensions_mut().insert(mk_peer());
        svc.ready().await.unwrap().call(req).await.unwrap();

        let entries = audit.entries.lock().unwrap();
        assert_eq!(entries.len(), 2);
        // Completed entry is the second — execution_time_ms ≥ 5 ms.
        let completed_ms = entries[1].execution_time_ms.unwrap_or(0);
        assert!(
            completed_ms >= 5,
            "expected ≥5ms elapsed, got {completed_ms}"
        );
    }
}
