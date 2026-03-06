use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::websocket::{Sender as WsSender, WsError, WsMessage};

/// Callbacks the framework supplies to an adapter for one gateway path.
///
/// The adapter calls these at the right moment in the connection lifecycle — it never
/// touches `GatewayWrapper`, `WsGatewayHandle`, or `ConnectionManager` directly.
pub struct WsConnectionCallbacks {
    on_connect: Arc<
        dyn Fn(
                HashMap<String, String>,
                Arc<dyn WsSender>,
            ) -> Pin<Box<dyn Future<Output = Result<String, WsError>> + Send>>
            + Send
            + Sync,
    >,
    on_message:
        Arc<dyn Fn(String, WsMessage) -> Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync>,
    on_disconnect: Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>,
}

impl WsConnectionCallbacks {
    pub(crate) fn new(
        on_connect: impl Fn(
            HashMap<String, String>,
            Arc<dyn WsSender>,
        ) -> Pin<Box<dyn Future<Output = Result<String, WsError>> + Send>>
        + Send
        + Sync
        + 'static,
        on_message: impl Fn(String, WsMessage) -> Pin<Box<dyn Future<Output = bool> + Send>>
        + Send
        + Sync
        + 'static,
        on_disconnect: impl Fn(String) -> Pin<Box<dyn Future<Output = ()> + Send>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            on_connect: Arc::new(on_connect),
            on_message: Arc::new(on_message),
            on_disconnect: Arc::new(on_disconnect),
        }
    }

    /// Called by the adapter when a new client connects.
    ///
    /// Pass the upgrade headers and an adapter-owned sender for this connection.
    /// Returns the assigned client id, or an error if a guard rejects the connection.
    pub async fn connect(
        &self,
        headers: HashMap<String, String>,
        sender: Arc<dyn WsSender>,
    ) -> Result<String, WsError> {
        (self.on_connect)(headers, sender).await
    }

    /// Called by the adapter for each decoded message from a connected client.
    ///
    /// Returns `true` to keep reading, `false` to close the connection.
    pub async fn message(&self, client_id: String, msg: WsMessage) -> bool {
        (self.on_message)(client_id, msg).await
    }

    /// Called by the adapter when the read loop ends (client disconnected).
    pub async fn disconnect(&self, client_id: String) {
        (self.on_disconnect)(client_id).await
    }
}

/// Interface for standalone (separate-port) WebSocket server adapters.
///
/// Implement `bind`, `create`, and `close`. The framework constructs
/// [`WsConnectionCallbacks`] with all lifecycle logic embedded — the adapter never
/// touches `GatewayWrapper` or `ConnectionManager` directly.
///
/// Same-port (HTTP upgrade) gateways are handled by [`HttpAdapter::bind_ws`].
#[async_trait]
pub trait WebSocketAdapter: Send + Sync + 'static {
    /// Register a gateway path for `port`, storing `callbacks` for each incoming connection.
    ///
    /// Called once per gateway before `create` is called for the same port.
    /// **Default:** returns error — implement for separate-port support.
    fn bind(&mut self, port: u16, path: &str, callbacks: Arc<WsConnectionCallbacks>) -> Result<()> {
        let _ = (port, path, callbacks);
        Err(anyhow::anyhow!(
            "This WebSocket adapter does not support separate-port servers"
        ))
    }

    /// Seal the configuration for `port` and return the running server future.
    ///
    /// Called once per unique port after all `bind` calls for that port. The returned
    /// future is the accept loop — the framework joins it alongside the HTTP server future
    /// so no server-level `tokio::spawn` is needed in the adapter.
    ///
    /// TODO: extend to `create(port, hostname, options: WsServerOptions)` once
    /// gateway-level options (TLS, backlog, keep-alive, etc.) are captured by the
    /// `#[websocket_gateway]` macro and propagated here.
    ///
    /// **Default:** returns error — implement for separate-port support.
    fn create(
        &mut self,
        port: u16,
        hostname: &str,
    ) -> Result<Pin<Box<dyn Future<Output = ()> + Send + 'static>>> {
        let _ = (port, hostname);
        Err(anyhow::anyhow!(
            "This WebSocket adapter does not support separate-port servers"
        ))
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Object-safe internal facade over [`WebSocketAdapter`] for storage in `ToniApplication`.
#[async_trait]
pub(crate) trait ErasedWebSocketAdapter: Send + Sync + 'static {
    fn bind(&mut self, port: u16, path: &str, callbacks: Arc<WsConnectionCallbacks>) -> Result<()>;
    fn create(
        &mut self,
        port: u16,
        hostname: &str,
    ) -> Result<Pin<Box<dyn Future<Output = ()> + Send + 'static>>>;
    async fn close(&mut self) -> Result<()>;
}

#[async_trait]
impl<W: WebSocketAdapter> ErasedWebSocketAdapter for W {
    fn bind(&mut self, port: u16, path: &str, callbacks: Arc<WsConnectionCallbacks>) -> Result<()> {
        <W as WebSocketAdapter>::bind(self, port, path, callbacks)
    }

    fn create(
        &mut self,
        port: u16,
        hostname: &str,
    ) -> Result<Pin<Box<dyn Future<Output = ()> + Send + 'static>>> {
        <W as WebSocketAdapter>::create(self, port, hostname)
    }

    async fn close(&mut self) -> Result<()> {
        <W as WebSocketAdapter>::close(self).await
    }
}
