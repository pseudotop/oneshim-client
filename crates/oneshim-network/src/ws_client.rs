//! WebSocket 클라이언트.
//!
//! `tokio-tungstenite` 기반 양방향 대화 모드.

use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use oneshim_core::error::CoreError;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tracing::{debug, info, warn};

use crate::auth::TokenManager;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// WebSocket 클라이언트 — 양방향 대화 모드
pub struct WsClient {
    base_url: String,
    token_manager: Arc<TokenManager>,
}

/// WebSocket으로 수신한 메시지
#[derive(Debug, Clone)]
pub enum WsMessage {
    /// 텍스트 메시지 (JSON)
    Text(String),
    /// 바이너리 메시지
    Binary(Vec<u8>),
    /// 연결 종료
    Close,
}

impl WsClient {
    /// 새 WebSocket 클라이언트 생성
    pub fn new(base_url: &str, token_manager: Arc<TokenManager>) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token_manager,
        }
    }

    /// WebSocket 연결 수립
    ///
    /// 수신 메시지는 `rx`로, 송신은 반환된 `WsSender`로 처리.
    pub async fn connect(
        &self,
        path: &str,
    ) -> Result<(WsSender, mpsc::Receiver<WsMessage>), CoreError> {
        let token = self.token_manager.get_token().await?;
        let ws_url = self
            .base_url
            .replace("http://", "ws://")
            .replace("https://", "wss://");
        let url = format!("{ws_url}{path}?token={token}");

        info!("WebSocket 연결: {}", url.split('?').next().unwrap_or(&url));

        let (ws_stream, _) = tokio_tungstenite::connect_async(&url)
            .await
            .map_err(|e| CoreError::Internal(format!("WebSocket 연결 실패: {e}")))?;

        let (write, read) = futures::StreamExt::split(ws_stream);
        let (tx, rx) = mpsc::channel(64);

        // 수신 태스크
        tokio::spawn(Self::read_loop(read, tx));

        Ok((
            WsSender {
                write: Arc::new(tokio::sync::Mutex::new(write)),
            },
            rx,
        ))
    }

    /// 수신 루프
    async fn read_loop(mut read: SplitStream<WsStream>, tx: mpsc::Sender<WsMessage>) {
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if tx.send(WsMessage::Text(text.to_string())).await.is_err() {
                        break;
                    }
                }
                Ok(Message::Binary(data)) => {
                    if tx.send(WsMessage::Binary(data.to_vec())).await.is_err() {
                        break;
                    }
                }
                Ok(Message::Close(_)) => {
                    let _ = tx.send(WsMessage::Close).await;
                    break;
                }
                Ok(_) => {} // Ping/Pong은 자동 처리
                Err(e) => {
                    warn!("WebSocket 수신 에러: {e}");
                    break;
                }
            }
        }
        debug!("WebSocket 수신 루프 종료");
    }
}

/// WebSocket 송신기
pub struct WsSender {
    write: Arc<tokio::sync::Mutex<SplitSink<WsStream, Message>>>,
}

impl WsSender {
    /// 텍스트 메시지 전송
    pub async fn send_text(&self, text: &str) -> Result<(), CoreError> {
        let mut write = self.write.lock().await;
        write
            .send(Message::Text(text.to_string()))
            .await
            .map_err(|e| CoreError::Internal(format!("WebSocket 전송 실패: {e}")))
    }

    /// JSON 메시지 전송
    pub async fn send_json<T: serde::Serialize>(&self, data: &T) -> Result<(), CoreError> {
        let json = serde_json::to_string(data)
            .map_err(|e| CoreError::Internal(format!("JSON 직렬화 실패: {e}")))?;
        self.send_text(&json).await
    }

    /// 연결 종료
    pub async fn close(&self) -> Result<(), CoreError> {
        let mut write = self.write.lock().await;
        write
            .send(Message::Close(None))
            .await
            .map_err(|e| CoreError::Internal(format!("WebSocket 종료 실패: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ws_client_creation() {
        let tm = Arc::new(TokenManager::new("http://localhost:8000"));
        let ws = WsClient::new("http://localhost:8000", tm);
        assert_eq!(ws.base_url, "http://localhost:8000");
    }
}
