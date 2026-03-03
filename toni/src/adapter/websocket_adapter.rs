use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::websocket::helpers::create_client_from_headers;
use crate::websocket::{
    ConnectionManager, DisconnectReason, GatewayWrapper, Sender as WsSender, WsClient, WsError,
    WsMessage,
};

/// Interface for WebSocket server adapters (both same-port upgrade and standalone).
///
/// Implement `recv`, `send`, and `bind_gateway` at minimum. `split` and `listen` are
/// only needed for broadcast mode and separate-port deployment respectively.
/// `handle_connection` and `handle_connection_with_broadcast` are provided as defaults
/// and should not be overridden.
///
/// ```rust,ignore
/// impl WebSocketAdapter for MyAdapter {
///     type Connection = my_framework::WebSocket;
///     type Sender = MySender;
///
///     async fn recv(conn: &mut Self::Connection) -> Option<Result<WsMessage, WsError>> { ... }
///     async fn send(conn: &mut Self::Connection, msg: WsMessage) -> Result<(), WsError> { ... }
///
///     fn bind_gateway(&mut self, path: &str, gateway: Arc<GatewayWrapper>) -> Result<()> { ... }
///     async fn listen(&mut self, port: u16, hostname: &str) -> Result<()> { ... }
/// }
/// ```
#[async_trait]
pub trait WebSocketAdapter: Send + Sync + 'static {
    /// The adapter's native per-connection type (e.g. `axum::extract::ws::WebSocket`).
    type Connection: Send + 'static;

    /// The write-only handle returned by `split`, used by `ConnectionManager` for broadcasting.
    ///
    /// Only required when broadcast mode is used — otherwise `split` can remain unimplemented.
    type Sender: WsSender + Send + Sync + 'static;

    /// Returns `None` when the connection is closed.
    async fn recv(conn: &mut Self::Connection) -> Option<Result<WsMessage, WsError>>;

    async fn send(conn: &mut Self::Connection, msg: WsMessage) -> Result<(), WsError>;

    /// Split the connection into a read half (still `Self::Connection`) and a shared write handle.
    ///
    /// Required only for broadcast mode. After splitting, `recv` is called on the read half
    /// and all writes go through `ConnectionManager` via the returned `Sender`.
    ///
    /// **Default:** panics — implement when `BroadcastModule` is in use.
    fn split(conn: Self::Connection) -> (Self::Connection, Self::Sender) {
        let _ = conn;
        unimplemented!(
            "split() not supported — implement WebSocketAdapter::split to enable broadcast mode"
        )
    }

    /// Framework adapters should not override this — implement `recv` and `send` instead.
    async fn handle_connection(
        conn: Self::Connection,
        gateway: &Arc<GatewayWrapper>,
        headers: HashMap<String, String>,
    ) {
        let mut conn = conn;
        let client = create_client_from_headers(headers);
        let client_id = client.id.clone();

        if let Err(e) = gateway.handle_connect(client).await {
            eprintln!("Connection rejected: {}", e);
            return;
        }

        println!("Client {} connected to {}", client_id, gateway.get_path());

        while let Some(msg_result) = Self::recv(&mut conn).await {
            match msg_result {
                Ok(ws_msg) => match gateway.handle_message(client_id.clone(), ws_msg).await {
                    Ok(Some(response)) => {
                        if Self::send(&mut conn, response).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        eprintln!("Error handling message: {}", e);
                        let error_msg = WsMessage::text(
                            serde_json::json!({ "error": e.to_string() }).to_string(),
                        );
                        let _ = Self::send(&mut conn, error_msg).await;
                        match &e {
                            WsError::ConnectionClosed(_) | WsError::AuthFailed(_) => break,
                            _ => {}
                        }
                    }
                },
                Err(e) => {
                    eprintln!("WebSocket error: {}", e);
                    break;
                }
            }
        }

        println!(
            "Client {} disconnected from {}",
            client_id,
            gateway.get_path()
        );
        gateway
            .handle_disconnect(client_id, DisconnectReason::ClientDisconnect)
            .await;
    }

    /// Framework adapters should not override this — implement `recv`, `send`, and `split`.
    async fn handle_connection_with_broadcast(
        conn: Self::Connection,
        gateway: &Arc<GatewayWrapper>,
        headers: HashMap<String, String>,
        connection_manager: &Arc<ConnectionManager>,
    ) {
        let (mut read_half, sender) = Self::split(conn);
        let client = create_client_from_headers(headers);
        let client_id = client.id.clone();

        // Phase 1: guards pass, client stored in GatewayWrapper
        if let Err(e) = gateway.begin_connect(client).await {
            eprintln!("Connection rejected: {}", e);
            return;
        }

        // Phase 2: register with ConnectionManager — client is now reachable for broadcasting
        let sender: Arc<dyn WsSender> = Arc::new(sender);
        connection_manager.register(WsClient::new(&client_id), sender, gateway.get_namespace());

        // Phase 3: on_connect fires — client is fully live in both GW map and ConnectionManager
        if let Err(e) = gateway.complete_connect(&client_id).await {
            eprintln!("on_connect hook failed: {}", e);
            connection_manager.unregister(&client_id);
            return;
        }

        println!(
            "✓ Client {} connected to {} (broadcast)",
            client_id,
            gateway.get_path()
        );

        while let Some(msg_result) = Self::recv(&mut read_half).await {
            match msg_result {
                Ok(ws_msg) => match gateway.handle_message(client_id.clone(), ws_msg).await {
                    Ok(Some(response)) => {
                        let _ = connection_manager
                            .send_to_clients(&[client_id.clone()], response)
                            .await;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        eprintln!("Error handling message: {}", e);
                        let error_msg = WsMessage::text(
                            serde_json::json!({ "error": e.to_string() }).to_string(),
                        );
                        let _ = connection_manager
                            .send_to_clients(&[client_id.clone()], error_msg)
                            .await;
                        match &e {
                            WsError::ConnectionClosed(_) | WsError::AuthFailed(_) => break,
                            _ => {}
                        }
                    }
                },
                Err(e) => {
                    eprintln!("WebSocket error: {}", e);
                    break;
                }
            }
        }

        println!(
            "✗ Client {} disconnected from {}",
            client_id,
            gateway.get_path()
        );
        connection_manager.unregister(&client_id);
        gateway
            .handle_disconnect(client_id, DisconnectReason::ClientDisconnect)
            .await;
    }

    /// Register a gateway for the standalone (separate-port) WebSocket server.
    ///
    /// **Default:** Returns error — implement to support gateway routing.
    fn bind_gateway(&mut self, path: &str, gateway: Arc<GatewayWrapper>) -> Result<()> {
        let _ = (path, gateway);
        Err(anyhow::anyhow!(
            "This WebSocket adapter does not implement bind_gateway"
        ))
    }

    /// Broadcast-aware variant of `bind_gateway`.
    ///
    /// Called instead of `bind_gateway` when `BroadcastModule` is imported.
    ///
    /// **Default:** Ignores `connection_manager` and falls back to `bind_gateway`.
    fn bind_gateway_with_broadcast(
        &mut self,
        path: &str,
        gateway: Arc<GatewayWrapper>,
        connection_manager: Arc<ConnectionManager>,
    ) -> Result<()> {
        let _ = connection_manager;
        self.bind_gateway(path, gateway)
    }

    /// **Default:** Returns error — implement to start a separate-port server.
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

/// Object-safe internal facade used by `ToniApplication` to store any `WebSocketAdapter`
/// behind a `Box<dyn ErasedWebSocketAdapter>` without requiring associated types at the
/// storage site. Users never interact with this trait directly.
#[async_trait]
pub(crate) trait ErasedWebSocketAdapter: Send + Sync + 'static {
    fn bind_gateway(&mut self, path: &str, gateway: Arc<GatewayWrapper>) -> Result<()>;
    fn bind_gateway_with_broadcast(
        &mut self,
        path: &str,
        gateway: Arc<GatewayWrapper>,
        connection_manager: Arc<ConnectionManager>,
    ) -> Result<()>;
    async fn listen(&mut self, port: u16, hostname: &str) -> Result<()>;
    async fn close(&mut self) -> Result<()>;
}

#[async_trait]
impl<W: WebSocketAdapter> ErasedWebSocketAdapter for W {
    fn bind_gateway(&mut self, path: &str, gateway: Arc<GatewayWrapper>) -> Result<()> {
        <W as WebSocketAdapter>::bind_gateway(self, path, gateway)
    }

    fn bind_gateway_with_broadcast(
        &mut self,
        path: &str,
        gateway: Arc<GatewayWrapper>,
        connection_manager: Arc<ConnectionManager>,
    ) -> Result<()> {
        <W as WebSocketAdapter>::bind_gateway_with_broadcast(
            self,
            path,
            gateway,
            connection_manager,
        )
    }

    async fn listen(&mut self, port: u16, hostname: &str) -> Result<()> {
        <W as WebSocketAdapter>::listen(self, port, hostname).await
    }

    async fn close(&mut self) -> Result<()> {
        <W as WebSocketAdapter>::close(self).await
    }
}
