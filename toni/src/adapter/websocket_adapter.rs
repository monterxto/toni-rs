use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;

use crate::websocket::helpers::create_client_from_headers;
use crate::websocket::{
    ConnectionManager, DisconnectReason, GatewayWrapper, Sender as WsSender, WsClient,
    WsError, WsGatewayHandle, WsMessage,
};

/// Interface for WebSocket server adapters (both same-port upgrade and standalone).
///
/// Implement `recv`, `send`, and either `bind_gateway` (same-port, via `HttpAdapter`) or
/// `create`/`attach`/`listen` (separate-port standalone server).
/// `split` is required only when broadcast mode or `WsGatewayHandle` is used.
/// Template methods (`handle_connection*`) are provided as defaults and should not be overridden.
#[async_trait]
pub trait WebSocketAdapter: Send + Sync + 'static {
    /// The adapter's native per-connection type (e.g. `axum::extract::ws::WebSocket`).
    type Connection: Send + 'static;

    /// The write-only handle returned by `split`, used for proactive sends.
    type Sender: WsSender + Send + Sync + 'static;

    /// Returns `None` when the connection is closed.
    async fn recv(conn: &mut Self::Connection) -> Option<Result<WsMessage, WsError>>;

    async fn send(conn: &mut Self::Connection, msg: WsMessage) -> Result<(), WsError>;

    /// Split the connection into a read half and a shared write handle.
    ///
    /// Required for `WsGatewayHandle` and broadcast mode. **Default:** panics.
    fn split(conn: Self::Connection) -> (Self::Connection, Self::Sender) {
        let _ = conn;
        unimplemented!(
            "split() not supported — implement WebSocketAdapter::split to enable handle/broadcast mode"
        )
    }

    // -------------------------------------------------------------------------
    // Separate-port server lifecycle
    // -------------------------------------------------------------------------

    /// Configure a server for the given port without starting it.
    ///
    /// Called once per unique port before any `attach` calls.
    /// **Default:** Returns error — implement for separate-port support.
    fn create(&mut self, port: u16) -> Result<()> {
        let _ = port;
        Err(anyhow::anyhow!(
            "This WebSocket adapter does not support separate-port servers"
        ))
    }

    /// Wire a gateway and its handle to a path on an already-created port.
    ///
    /// Called once per gateway after `create`. **Default:** Returns error.
    fn attach(
        &mut self,
        port: u16,
        path: &str,
        gateway: Arc<GatewayWrapper>,
        handle: Arc<WsGatewayHandle>,
    ) -> Result<()> {
        let _ = (port, path, gateway, handle);
        Err(anyhow::anyhow!(
            "This WebSocket adapter does not support separate-port servers"
        ))
    }

    /// Broadcast-aware variant of `attach`. Called instead of `attach` when `BroadcastModule`
    /// is imported. **Default:** Ignores `connection_manager` and delegates to `attach`.
    fn attach_with_broadcast(
        &mut self,
        port: u16,
        path: &str,
        gateway: Arc<GatewayWrapper>,
        handle: Arc<WsGatewayHandle>,
        connection_manager: Arc<ConnectionManager>,
    ) -> Result<()> {
        let _ = connection_manager;
        self.attach(port, path, gateway, handle)
    }

    /// Start accepting connections on all previously `create`d ports.
    ///
    /// Implementations should spawn tasks and return immediately so the caller
    /// can start the HTTP server concurrently.
    /// **Default:** Returns error.
    async fn listen(&mut self, hostname: &str) -> Result<()> {
        let _ = hostname;
        Err(anyhow::anyhow!(
            "Standalone WebSocket server not supported by this adapter"
        ))
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Connection loop templates — do not override
    // -------------------------------------------------------------------------

    /// Simple request/response loop — no split, no external registry.
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
                Err(_) => break,
            }
        }

        gateway
            .handle_disconnect(client_id, DisconnectReason::ClientDisconnect)
            .await;
    }

    /// Handle + split loop — registers senders with the gateway handle for proactive sends.
    async fn handle_connection_with_handle(
        conn: Self::Connection,
        gateway: &Arc<GatewayWrapper>,
        headers: HashMap<String, String>,
        handle: &Arc<WsGatewayHandle>,
    ) {
        let (mut read_half, sender) = Self::split(conn);
        let client = create_client_from_headers(headers);
        let client_id = client.id.clone();

        // Phase 1: guards pass, client stored
        if let Err(e) = gateway.begin_connect(client).await {
            eprintln!("Connection rejected: {}", e);
            return;
        }

        // Phase 2: register sender with handle — client is now reachable for proactive sends
        let sender: Arc<dyn WsSender> = Arc::new(sender);
        handle.register(client_id.clone(), sender);

        // Phase 3: on_connect fires
        if let Err(e) = gateway.complete_connect(&client_id).await {
            eprintln!("on_connect hook failed: {}", e);
            handle.unregister(&client_id);
            return;
        }

        while let Some(msg_result) = Self::recv(&mut read_half).await {
            match msg_result {
                Ok(ws_msg) => match gateway.handle_message(client_id.clone(), ws_msg).await {
                    Ok(Some(response)) => {
                        handle.emit(&client_id, response).await;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        let error_msg = WsMessage::text(
                            serde_json::json!({ "error": e.to_string() }).to_string(),
                        );
                        handle.emit(&client_id, error_msg).await;
                        match &e {
                            WsError::ConnectionClosed(_) | WsError::AuthFailed(_) => break,
                            _ => {}
                        }
                    }
                },
                Err(_) => break,
            }
        }

        handle.unregister(&client_id);
        gateway
            .handle_disconnect(client_id, DisconnectReason::ClientDisconnect)
            .await;
    }

    /// Handle + split loop with both a gateway handle and `ConnectionManager`.
    ///
    /// Registers senders with both so the gateway can use `WsGatewayHandle` for
    /// per-gateway sends and `BroadcastService` for cross-gateway sends.
    async fn handle_connection_with_handle_and_broadcast(
        conn: Self::Connection,
        gateway: &Arc<GatewayWrapper>,
        headers: HashMap<String, String>,
        handle: &Arc<WsGatewayHandle>,
        connection_manager: &Arc<ConnectionManager>,
    ) {
        let (mut read_half, sender) = Self::split(conn);
        let client = create_client_from_headers(headers);
        let client_id = client.id.clone();

        // Phase 1: guards pass, client stored
        if let Err(e) = gateway.begin_connect(client).await {
            eprintln!("Connection rejected: {}", e);
            return;
        }

        // Phase 2: register with both registries
        let sender: Arc<dyn WsSender> = Arc::new(sender);
        handle.register(client_id.clone(), sender.clone());
        connection_manager.register(
            WsClient::new(&client_id),
            sender,
            gateway.get_namespace(),
        );

        // Phase 3: on_connect fires
        if let Err(e) = gateway.complete_connect(&client_id).await {
            eprintln!("on_connect hook failed: {}", e);
            handle.unregister(&client_id);
            connection_manager.unregister(&client_id);
            return;
        }

        while let Some(msg_result) = Self::recv(&mut read_half).await {
            match msg_result {
                Ok(ws_msg) => match gateway.handle_message(client_id.clone(), ws_msg).await {
                    Ok(Some(response)) => {
                        handle.emit(&client_id, response).await;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        let error_msg = WsMessage::text(
                            serde_json::json!({ "error": e.to_string() }).to_string(),
                        );
                        handle.emit(&client_id, error_msg).await;
                        match &e {
                            WsError::ConnectionClosed(_) | WsError::AuthFailed(_) => break,
                            _ => {}
                        }
                    }
                },
                Err(_) => break,
            }
        }

        handle.unregister(&client_id);
        connection_manager.unregister(&client_id);
        gateway
            .handle_disconnect(client_id, DisconnectReason::ClientDisconnect)
            .await;
    }

    /// Same-port broadcast loop — kept for `HttpAdapter` upgrade routes.
    async fn handle_connection_with_broadcast(
        conn: Self::Connection,
        gateway: &Arc<GatewayWrapper>,
        headers: HashMap<String, String>,
        connection_manager: &Arc<ConnectionManager>,
    ) {
        let (mut read_half, sender) = Self::split(conn);
        let client = create_client_from_headers(headers);
        let client_id = client.id.clone();

        if let Err(e) = gateway.begin_connect(client).await {
            eprintln!("Connection rejected: {}", e);
            return;
        }

        let sender: Arc<dyn WsSender> = Arc::new(sender);
        connection_manager.register(WsClient::new(&client_id), sender, gateway.get_namespace());

        if let Err(e) = gateway.complete_connect(&client_id).await {
            eprintln!("on_connect hook failed: {}", e);
            connection_manager.unregister(&client_id);
            return;
        }

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
                Err(_) => break,
            }
        }

        connection_manager.unregister(&client_id);
        gateway
            .handle_disconnect(client_id, DisconnectReason::ClientDisconnect)
            .await;
    }
}

/// Object-safe internal facade over `WebSocketAdapter` for storage in `ToniApplication`.
#[async_trait]
pub(crate) trait ErasedWebSocketAdapter: Send + Sync + 'static {
    fn create(&mut self, port: u16) -> Result<()>;
    fn attach(
        &mut self,
        port: u16,
        path: &str,
        gateway: Arc<GatewayWrapper>,
        handle: Arc<WsGatewayHandle>,
    ) -> Result<()>;
    fn attach_with_broadcast(
        &mut self,
        port: u16,
        path: &str,
        gateway: Arc<GatewayWrapper>,
        handle: Arc<WsGatewayHandle>,
        connection_manager: Arc<ConnectionManager>,
    ) -> Result<()>;
    async fn listen(&mut self, hostname: &str) -> Result<()>;
    async fn close(&mut self) -> Result<()>;
}

#[async_trait]
impl<W: WebSocketAdapter> ErasedWebSocketAdapter for W {
    fn create(&mut self, port: u16) -> Result<()> {
        <W as WebSocketAdapter>::create(self, port)
    }

    fn attach(
        &mut self,
        port: u16,
        path: &str,
        gateway: Arc<GatewayWrapper>,
        handle: Arc<WsGatewayHandle>,
    ) -> Result<()> {
        <W as WebSocketAdapter>::attach(self, port, path, gateway, handle)
    }

    fn attach_with_broadcast(
        &mut self,
        port: u16,
        path: &str,
        gateway: Arc<GatewayWrapper>,
        handle: Arc<WsGatewayHandle>,
        connection_manager: Arc<ConnectionManager>,
    ) -> Result<()> {
        <W as WebSocketAdapter>::attach_with_broadcast(
            self,
            port,
            path,
            gateway,
            handle,
            connection_manager,
        )
    }

    async fn listen(&mut self, hostname: &str) -> Result<()> {
        <W as WebSocketAdapter>::listen(self, hostname).await
    }

    async fn close(&mut self) -> Result<()> {
        <W as WebSocketAdapter>::close(self).await
    }
}
