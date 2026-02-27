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

pub struct WsClient {
    base_url: String,
    token_manager: Arc<TokenManager>,
}

#[derive(Debug, Clone)]
pub enum WsMessage {
    Text(String),
    Binary(Vec<u8>),
    Close,
}

impl WsClient {
    pub fn new(base_url: &str, token_manager: Arc<TokenManager>) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token_manager,
        }
    }

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

        info!(
            "WebSocket connection: {}",
            url.split('?').next().unwrap_or(&url)
        );

        let (ws_stream, _) = tokio_tungstenite::connect_async(&url)
            .await
            .map_err(|e| CoreError::Internal(format!("WebSocket connection failure: {e}")))?;

        let (write, read) = futures::StreamExt::split(ws_stream);
        let (tx, rx) = mpsc::channel(64);

        tokio::spawn(Self::read_loop(read, tx));

        Ok((
            WsSender {
                write: Arc::new(tokio::sync::Mutex::new(write)),
            },
            rx,
        ))
    }

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
                Ok(_) => {} // Ping/Pong
                Err(e) => {
                    warn!("WebSocket received error: {e}");
                    break;
                }
            }
        }
        debug!("WebSocket received ended");
    }
}

pub struct WsSender {
    write: Arc<tokio::sync::Mutex<SplitSink<WsStream, Message>>>,
}

impl WsSender {
    pub async fn send_text(&self, text: &str) -> Result<(), CoreError> {
        let mut write = self.write.lock().await;
        write
            .send(Message::Text(text.to_string()))
            .await
            .map_err(|e| CoreError::Internal(format!("WebSocket sent failure: {e}")))
    }

    pub async fn send_json<T: serde::Serialize>(&self, data: &T) -> Result<(), CoreError> {
        let json = serde_json::to_string(data)
            .map_err(|e| CoreError::Internal(format!("JSON serialization failed: {e}")))?;
        self.send_text(&json).await
    }

    pub async fn close(&self) -> Result<(), CoreError> {
        let mut write = self.write.lock().await;
        write
            .send(Message::Close(None))
            .await
            .map_err(|e| CoreError::Internal(format!("WebSocket ended failure: {e}")))
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
