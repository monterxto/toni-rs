use serde::{Deserialize, Serialize};

/// WebSocket message data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WsMessage {
    /// Text message (JSON, plain text, etc.)
    Text(String),

    /// Binary message
    Binary(Vec<u8>),

    /// Ping frame
    Ping(Vec<u8>),

    /// Pong frame
    Pong(Vec<u8>),
}

impl WsMessage {
    pub fn text(data: impl Into<String>) -> Self {
        Self::Text(data.into())
    }

    pub fn binary(data: Vec<u8>) -> Self {
        Self::Binary(data)
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            WsMessage::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_binary(&self) -> Option<&[u8]> {
        match self {
            WsMessage::Binary(b) => Some(b),
            _ => None,
        }
    }
}
