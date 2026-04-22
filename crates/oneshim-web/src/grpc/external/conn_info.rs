//! Per-connection peer info + per-request auth context.
//! PeerInfo is injected by the custom accept loop (Task 12); tonic's
//! ConnectInfoLayer propagates it into request extensions via the
//! Connected trait impl on PeerAwareStream.

use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub remote_addr: SocketAddr,
    pub peer_cert_der: Option<Vec<u8>>,
    pub cert_subject_cn: Option<String>,
    pub tls_version: String,
}

/// Authenticated caller identity, inserted by AuthLayer into request extensions.
/// Task 11 (audit_bridge) reads this for the audit entry.
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub auth_type: AuthType,
    pub client_id: String,
    pub jti: Option<String>,
    pub command_id: String, // ulid
}

#[derive(Debug, Clone, Copy)]
pub enum AuthType {
    Jwt,
    Mtls,
    JwtAndMtls,
}

/// Wraps a TLS stream + PeerInfo; implements Connected so tonic's
/// ConnectInfoLayer exposes PeerInfo in every request's extensions.
pub struct PeerAwareStream<S> {
    inner: S,
    peer_info: PeerInfo,
}

impl<S> PeerAwareStream<S> {
    pub fn new(inner: S, peer_info: PeerInfo) -> Self {
        Self { inner, peer_info }
    }
    pub fn peer_info(&self) -> &PeerInfo {
        &self.peer_info
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for PeerAwareStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for PeerAwareStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

impl<S: Send + 'static> tonic::transport::server::Connected for PeerAwareStream<S> {
    type ConnectInfo = PeerInfo;
    fn connect_info(&self) -> Self::ConnectInfo {
        self.peer_info.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt};
    use tonic::transport::server::Connected as _;

    fn mk_peer() -> PeerInfo {
        PeerInfo {
            remote_addr: "127.0.0.1:5001".parse().unwrap(),
            peer_cert_der: None,
            cert_subject_cn: None,
            tls_version: "TLSv1.3".into(),
        }
    }

    #[test]
    fn connect_info_returns_peer() {
        let (a, _b) = duplex(64);
        let stream = PeerAwareStream::new(a, mk_peer());
        let info = stream.connect_info();
        assert_eq!(info.remote_addr.port(), 5001);
    }

    #[tokio::test]
    async fn async_read_delegates() {
        let (a, mut b) = duplex(64);
        let mut stream = PeerAwareStream::new(a, mk_peer());
        b.write_all(b"hello").await.unwrap();
        drop(b);
        let mut buf = [0u8; 5];
        stream.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");
    }

    #[tokio::test]
    async fn async_write_delegates() {
        let (a, mut b) = duplex(64);
        let mut stream = PeerAwareStream::new(a, mk_peer());
        stream.write_all(b"world").await.unwrap();
        stream.shutdown().await.unwrap();
        let mut buf = Vec::new();
        b.read_to_end(&mut buf).await.unwrap();
        assert_eq!(&buf, b"world");
    }

    #[test]
    fn auth_context_clone_works() {
        let ctx = AuthContext {
            auth_type: AuthType::Jwt,
            client_id: "u1".into(),
            jti: Some("j1".into()),
            command_id: "01HXYZ".into(),
        };
        let cloned = ctx.clone();
        assert_eq!(ctx.client_id, cloned.client_id);
    }
}
