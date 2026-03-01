use std::collections::HashMap;

use anyhow::Result;

use toni::async_trait;
use toni::WebSocketAdapter;

/// Standalone WebSocket-only adapter for separate-port deployment.
/// For same-port HTTP + WebSocket (the common case), use `AxumAdapter::on_upgrade()` instead.
#[derive(Clone, Default)]
pub struct AxumWebSocketAdapter;

impl AxumWebSocketAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl WebSocketAdapter for AxumWebSocketAdapter {
    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

// ============================================================================
// AxumWsSocket — WsSocket implementation for Axum
// ============================================================================

use axum::{
    extract::ws::{Message, WebSocket},
    http::HeaderMap,
};
use futures_util::{stream::SplitStream, SinkExt, StreamExt};
use toni::websocket::{Sender, WsError, WsMessage, WsSocket};

use crate::TokioSender;

/// WebSocket equivalent of `AxumRouteAdapter` — handles Axum ↔ `WsMessage` conversion
pub struct AxumWsSocket(WebSocket);

impl AxumWsSocket {
    pub fn new(socket: WebSocket) -> Self {
        Self(socket)
    }
}

#[async_trait]
impl WsSocket for AxumWsSocket {
    async fn recv(&mut self) -> Option<Result<WsMessage, WsError>> {
        self.0.recv().await.map(|r| {
            r.map_err(|e| WsError::Internal(e.to_string()))
                .and_then(axum_to_ws_message)
        })
    }

    async fn send(&mut self, msg: WsMessage) -> Result<(), WsError> {
        let axum_msg = ws_message_to_axum(msg)?;
        self.0
            .send(axum_msg)
            .await
            .map_err(|e| WsError::Internal(e.to_string()))
    }

    /// Returns a read-only `WsSocket` + a `TokioSender` that forwards writes to the socket's write half via mpsc channel
    fn split(self) -> (Box<dyn WsSocket>, Box<dyn Sender>) {
        let (write, read) = self.0.split();
        let (tx, mut rx) = tokio::sync::mpsc::channel::<WsMessage>(32);

        tokio::spawn(async move {
            let mut write = write;
            while let Some(msg) = rx.recv().await {
                if let Ok(axum_msg) = ws_message_to_axum(msg) {
                    if write.send(axum_msg).await.is_err() {
                        break;
                    }
                }
            }
        });

        let reader = AxumReadSocket(read);
        let sender = TokioSender::new(tx);

        (Box::new(reader), Box::new(sender))
    }
}

// ============================================================================
// AxumReadSocket — Read-only WsSocket (used after split)
// ============================================================================

/// Read-only socket wrapping the read half of a split Axum WebSocket
struct AxumReadSocket(SplitStream<WebSocket>);

#[async_trait]
impl WsSocket for AxumReadSocket {
    async fn recv(&mut self) -> Option<Result<WsMessage, WsError>> {
        self.0.next().await.map(|r| {
            r.map_err(|e| WsError::Internal(e.to_string()))
                .and_then(axum_to_ws_message)
        })
    }

    async fn send(&mut self, _msg: WsMessage) -> Result<(), WsError> {
        Err(WsError::Internal(
            "Cannot send on read-only socket (use Sender from split)".into(),
        ))
    }
}

// ============================================================================
// Message conversion (private — encapsulated inside AxumWsSocket)
// ============================================================================

fn axum_to_ws_message(msg: Message) -> Result<WsMessage, WsError> {
    match msg {
        Message::Text(text) => Ok(WsMessage::Text(text.to_string())),
        Message::Binary(data) => Ok(WsMessage::Binary(data.to_vec())),
        Message::Ping(data) => Ok(WsMessage::Ping(data.to_vec())),
        Message::Pong(data) => Ok(WsMessage::Pong(data.to_vec())),
        Message::Close(_) => Err(WsError::ConnectionClosed("Close frame received".into())),
    }
}

fn ws_message_to_axum(msg: WsMessage) -> Result<Message, WsError> {
    match msg {
        WsMessage::Text(text) => Ok(Message::Text(text.into())),
        WsMessage::Binary(data) => Ok(Message::Binary(data.into())),
        WsMessage::Ping(data) => Ok(Message::Ping(data.into())),
        WsMessage::Pong(data) => Ok(Message::Pong(data.into())),
        WsMessage::Close => Ok(Message::Close(None)),
    }
}

// ============================================================================
// Header extraction (public — used by AxumAdapter::on_upgrade)
// ============================================================================

/// Extract headers from Axum HeaderMap into a framework-agnostic HashMap
pub fn extract_headers(headers: &HeaderMap) -> HashMap<String, String> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_string(), v.to_string()))
        })
        .collect()
}
