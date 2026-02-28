use anyhow::Result;

use crate::async_trait;

/// Minimal interface for standalone (separate-port) WebSocket servers.
///
/// For same-port deployment (HTTP + WebSocket on the same port), use `HttpAdapter::on_upgrade()` instead.
#[async_trait]
pub trait WebSocketAdapter: Send + Sync + 'static {
    /// Only used when you want WebSocket on a separate port from HTTP.
    /// Most applications use same-port deployment via HTTP upgrade instead.
    async fn listen(&mut self, port: u16, hostname: &str) -> Result<()> {
        let _ = (port, hostname);
        Err(anyhow::anyhow!(
            "Standalone WebSocket server not supported by this adapter"
        ))
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}
