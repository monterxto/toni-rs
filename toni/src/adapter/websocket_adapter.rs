use std::future::Future;
use std::sync::Arc;

use anyhow::Result;

use crate::websocket::GatewayWrapper;

/// WebSocket adapter trait for backend abstraction
///
/// Similar to `HttpAdapter`, this trait provides a common interface for different
/// WebSocket server implementations (Axum, Tungstenite, etc.).
///
/// Adapters can support:
/// - Standalone WebSocket servers (separate port)
/// - HTTP upgrade-based WebSocket (same port as HTTP)
pub trait WebSocketAdapter: Clone + Send + Sync + 'static {
    /// Register a gateway at a specific path
    ///
    /// # Arguments
    /// * `path` - WebSocket endpoint path (e.g., "/chat", "/notifications")
    /// * `gateway` - Wrapped gateway with guards, interceptors, etc.
    fn add_gateway(&mut self, path: &str, gateway: Arc<GatewayWrapper>);

    /// Start standalone WebSocket server
    ///
    /// This is used when WebSocket runs on a separate port from HTTP
    fn listen(self, port: u16, hostname: &str) -> impl Future<Output = Result<()>> + Send;
}
