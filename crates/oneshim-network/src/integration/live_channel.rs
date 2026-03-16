use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use reqwest::header::HeaderMap;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tracing::{debug, warn};

use oneshim_core::error::CoreError;

type LiveWebSocketStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Clone)]
pub struct WebSocketIntegrationControlChannel {
    sender: Arc<Mutex<futures::stream::SplitSink<LiveWebSocketStream, Message>>>,
}

impl WebSocketIntegrationControlChannel {
    pub async fn connect(url: &str, headers: HeaderMap) -> Result<Self, CoreError> {
        let mut request = url
            .into_client_request()
            .map_err(|err| CoreError::Validation {
                field: "integration.session.channel_url".to_string(),
                message: format!("invalid websocket URL: {err}"),
            })?;

        for (name, value) in headers.iter() {
            request.headers_mut().insert(name, value.clone());
        }

        let (stream, _) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(|err| {
                CoreError::Network(format!("integration websocket connect failed: {err}"))
            })?;
        let (writer, mut reader) = stream.split();

        tokio::spawn(async move {
            while let Some(message) = reader.next().await {
                match message {
                    Ok(Message::Close(_)) => break,
                    Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {}
                    Ok(Message::Text(_)) | Ok(Message::Binary(_)) => {}
                    Ok(Message::Frame(_)) => {}
                    Err(err) => {
                        warn!("integration websocket read failed: {err}");
                        break;
                    }
                }
            }
            debug!("integration websocket control channel reader ended");
        });

        Ok(Self {
            sender: Arc::new(Mutex::new(writer)),
        })
    }

    pub async fn send_json<T: serde::Serialize>(&self, payload: &T) -> Result<(), CoreError> {
        let text = serde_json::to_string(payload).map_err(|err| {
            CoreError::Internal(format!("integration websocket serialization failed: {err}"))
        })?;
        let mut sender = self.sender.lock().await;
        sender
            .send(Message::Text(text.into()))
            .await
            .map_err(|err| CoreError::Network(format!("integration websocket send failed: {err}")))
    }

    pub async fn close(&self) -> Result<(), CoreError> {
        let mut sender = self.sender.lock().await;
        sender
            .send(Message::Close(None))
            .await
            .map_err(|err| CoreError::Network(format!("integration websocket close failed: {err}")))
    }
}
