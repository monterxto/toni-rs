//! WebSocket socket abstraction (parallel to RouteAdapter for HTTP)
//!
//! Each framework wraps its native socket type and implements `WsSocket`,
//! encapsulating message format conversion inside `recv`/`send`.
//!
//! This is the WebSocket equivalent of `RouteAdapter`:
//! - `RouteAdapter::adapt_request()` ↔ `WsSocket::recv()` (framework → toni)
//! - `RouteAdapter::adapt_response()` ↔ `WsSocket::send()` (toni → framework)
//! - `RouteAdapter::handle_request()` ↔ `WsSocket::handle_connection()` (core lifecycle)

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use super::broadcast::{ConnectionManager, Sender};
use super::helpers::create_client_from_headers;
use super::{DisconnectReason, GatewayWrapper, WsClient, WsError, WsMessage};

/// Framework-agnostic WebSocket socket
///
/// Implementors wrap a framework-specific socket (e.g. Axum's `WebSocket`,
/// Actix's `ws::WebsocketContext`) and handle message format conversion
/// transparently inside `recv` and `send`.
///
/// The `handle_connection` default method manages the full connection lifecycle,
/// parallel to `RouteAdapter::handle_request()` on the HTTP side.
/// Framework adapters only need to implement `recv()` and `send()`.
///
/// # Example (Axum)
///
/// ```rust,ignore
/// pub struct AxumWsSocket(pub WebSocket);
///
/// #[async_trait]
/// impl WsSocket for AxumWsSocket {
///     async fn recv(&mut self) -> Option<Result<WsMessage, WsError>> {
///         self.0.recv().await.map(|r| {
///             r.map_err(|e| WsError::Internal(e.to_string()))
///              .and_then(axum_to_ws_message)
///         })
///     }
///
///     async fn send(&mut self, msg: WsMessage) -> Result<(), WsError> {
///         let axum_msg = ws_message_to_axum(msg)?;
///         self.0.send(axum_msg).await
///             .map_err(|e| WsError::Internal(e.to_string()))
///     }
/// }
///
/// // Usage in on_upgrade:
/// let mut ws_socket = AxumWsSocket::new(socket);
/// ws_socket.handle_connection(&gateway, headers).await;
/// ```
#[async_trait]
pub trait WsSocket: Send {
    /// Receive the next message, converting from framework-specific format to `WsMessage`
    ///
    /// Returns `None` when the connection is closed.
    async fn recv(&mut self) -> Option<Result<WsMessage, WsError>>;

    /// Send a message, converting from `WsMessage` to framework-specific format
    async fn send(&mut self, msg: WsMessage) -> Result<(), WsError>;

    /// Parallel to `RouteAdapter::handle_request()`. Manages:
    /// 1. Create client from handshake headers
    /// 2. Run connection guards via `gateway.handle_connect()`
    /// 3. Message loop: recv → gateway pipeline (guards, interceptors, pipes, handler) → send
    /// 4. Disconnect cleanup via `gateway.handle_disconnect()`
    ///
    /// Framework adapters typically don't override this — just implement `recv()` and `send()`.
    async fn handle_connection(
        &mut self,
        gateway: &Arc<GatewayWrapper>,
        headers: HashMap<String, String>,
    ) {
        let client = create_client_from_headers(headers);
        let client_id = client.id.clone();

        // Runs guards and lifecycle hooks before entering the message loop
        if let Err(e) = gateway.handle_connect(client).await {
            eprintln!("Connection rejected: {}", e);
            return;
        }

        println!("Client {} connected to {}", client_id, gateway.get_path());

        while let Some(msg_result) = self.recv().await {
            match msg_result {
                Ok(ws_msg) => {
                    match gateway.handle_message(client_id.clone(), ws_msg).await {
                        Ok(Some(response)) => {
                            if self.send(response).await.is_err() {
                                break;
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            eprintln!("Error handling message: {}", e);
                            let error_msg = WsMessage::text(
                                serde_json::json!({
                                    "error": e.to_string()
                                })
                                .to_string(),
                            );
                            let _ = self.send(error_msg).await;

                            // Only fatal errors disconnect; recoverable ones (InvalidMessage, EventNotFound) stay alive
                            match &e {
                                WsError::ConnectionClosed(_) | WsError::AuthFailed(_) => break,
                                _ => {}
                            }
                        }
                    }
                }
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

    /// Like `handle_connection()` but routes all writes through `ConnectionManager`,
    /// enabling broadcasting to other clients. Splits the socket via `split()`, registers
    /// the write handle with `ConnectionManager`, then runs the read loop.
    ///
    /// Framework adapters only need to implement `split()` to support broadcast mode.
    async fn handle_connection_with_broadcast(
        self,
        gateway: &Arc<GatewayWrapper>,
        headers: HashMap<String, String>,
        connection_manager: &Arc<ConnectionManager>,
    ) where
        Self: Sized,
    {
        let (mut reader, sender) = self.split();
        let client = create_client_from_headers(headers);
        let client_id = client.id.clone();

        // Phase 1: guards pass, client stored in GatewayWrapper
        if let Err(e) = gateway.begin_connect(client).await {
            eprintln!("Connection rejected: {}", e);
            return;
        }

        // Phase 2: register with ConnectionManager — client is now reachable for broadcasting
        let sender: Arc<dyn Sender> = Arc::from(sender);
        connection_manager.register(WsClient::new(&client_id), sender, gateway.get_namespace());

        // Phase 3: on_connect fires — client is fully live in GW map AND ConnectionManager
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

        // Responses go through ConnectionManager so other handlers can broadcast to this client
        while let Some(msg_result) = reader.recv().await {
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

    /// Split into a read-only socket + a shared write handle (`Sender`)
    ///
    /// Used for broadcast mode where multiple tasks need concurrent write access
    /// to the same connection. The framework adapter splits the underlying socket,
    /// creates a channel + write forwarder, and returns a `Sender` that bridges
    /// the channel to the socket's write half.
    ///
    /// The returned `WsSocket` is read-only (recv works, send returns error).
    /// All writes go through the returned `Sender` via `ConnectionManager`.
    fn split(self) -> (Box<dyn WsSocket>, Box<dyn Sender>)
    where
        Self: Sized,
    {
        unimplemented!("split() not supported for this socket type")
    }
}
