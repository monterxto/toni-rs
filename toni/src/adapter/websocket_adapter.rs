use std::sync::Arc;

use anyhow::Result;

use crate::async_trait;
use crate::websocket::{ConnectionManager, GatewayWrapper};

/// Interface for standalone (separate-port) WebSocket servers.
///
/// For same-port deployment (HTTP + WebSocket on the same port), use `HttpAdapter::on_upgrade()` instead.
///
/// # Separate-port deployment flow
///
/// 1. `ToniApplication` calls `on_gateway()` / `on_gateway_with_broadcast()` for each discovered
///    gateway — the adapter stores the path → gateway map.
/// 2. `ToniApplication` calls `listen()` — the adapter starts accepting connections, routes each
///    incoming connection to the right gateway by path, wraps the socket in a `WsSocket` impl,
///    and calls `ws_socket.handle_connection()`.
#[async_trait]
pub trait WebSocketAdapter: Send + Sync + 'static {
    /// Register a gateway with this adapter.
    ///
    /// Called once per gateway at startup (before `listen()`). The adapter stores the
    /// path → gateway mapping and uses it to route incoming connections.
    ///
    /// Equivalent of `HttpAdapter::on_upgrade()` for the separate-port path.
    ///
    /// **Default:** Returns error — implement this to support gateway routing.
    fn on_gateway(&mut self, path: &str, gateway: Arc<GatewayWrapper>) -> Result<()> {
        let _ = (path, gateway);
        Err(anyhow::anyhow!(
            "This WebSocket adapter does not implement on_gateway"
        ))
    }

    /// Broadcast-aware gateway registration.
    ///
    /// Called instead of `on_gateway()` when `BroadcastModule` is imported.
    ///
    /// **Default:** Ignores `connection_manager` and falls back to `on_gateway()`.
    fn on_gateway_with_broadcast(
        &mut self,
        path: &str,
        gateway: Arc<GatewayWrapper>,
        connection_manager: Arc<ConnectionManager>,
    ) -> Result<()> {
        let _ = connection_manager;
        self.on_gateway(path, gateway)
    }

    /// Start the standalone WebSocket server.
    ///
    /// Called after all gateways have been registered via `on_gateway()`.
    ///
    /// **Default:** Returns error — implement this to start a separate-port server.
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
