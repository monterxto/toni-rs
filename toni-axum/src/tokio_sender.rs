//! Tokio-based sender implementation for WebSocket broadcasting

use tokio::sync::mpsc;
use toni::async_trait;
use toni::websocket::{SendError, Sender, TrySendError, WsMessage};

/// Tokio mpsc channel sender wrapper
///
/// Implements the `Sender` trait for Tokio's async MPSC channels.
/// This allows the framework's broadcasting system to work with Tokio runtime.
#[derive(Clone)]
pub struct TokioSender {
    inner: mpsc::Sender<WsMessage>,
}

impl TokioSender {
    /// Create a new TokioSender
    pub fn new(sender: mpsc::Sender<WsMessage>) -> Self {
        Self { inner: sender }
    }

    /// Get the inner Tokio sender
    pub fn inner(&self) -> &mpsc::Sender<WsMessage> {
        &self.inner
    }
}

#[async_trait]
impl Sender for TokioSender {
    async fn send(&self, message: WsMessage) -> Result<(), SendError> {
        self.inner.send(message).await.map_err(|_| SendError)
    }

    fn try_send(&self, message: WsMessage) -> Result<(), TrySendError> {
        self.inner.try_send(message).map_err(|e| match e {
            mpsc::error::TrySendError::Full(msg) => TrySendError::Full(msg),
            mpsc::error::TrySendError::Closed(_) => TrySendError::Closed,
        })
    }
}
