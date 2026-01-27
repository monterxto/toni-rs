use thiserror::Error;

/// WebSocket-specific errors
#[derive(Debug, Error, Clone)]
pub enum WsError {
    #[error("Connection closed: {0}")]
    ConnectionClosed(String),

    #[error("Invalid message format: {0}")]
    InvalidMessage(String),

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Event not found: {0}")]
    EventNotFound(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Reason for client disconnection
#[derive(Debug, Clone)]
pub enum DisconnectReason {
    /// Client initiated disconnection
    ClientDisconnect,
    /// Server is shutting down
    ServerShutdown,
    /// Connection timeout
    Timeout,
    /// Error occurred
    Error(String),
}

impl DisconnectReason {
    pub fn error(msg: impl Into<String>) -> Self {
        Self::Error(msg.into())
    }
}
