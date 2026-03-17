use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use crate::http_helpers::RouteMetadata;
use crate::injector::Context;

use super::{DisconnectReason, WsClient, WsError, WsMessage};

/// Core gateway trait for WebSocket handlers
///
/// Gateways handle WebSocket connections and route messages to appropriate handlers.
/// They integrate with Toni's DI system and execution context for guards, interceptors,
/// and error handling.
#[async_trait]
pub trait GatewayTrait: Send + Sync {
    /// Get unique token for DI registration
    fn get_token(&self) -> String;

    /// Get WebSocket path (e.g., "/chat", "/notifications")
    fn get_path(&self) -> String;

    /// Get namespace (optional, for multi-tenancy)
    fn get_namespace(&self) -> Option<String> {
        None
    }

    /// Get the port this gateway listens on.
    ///
    /// `None` (default) means same port as the HTTP server.
    /// `Some(port)` triggers a separate WebSocket server on that port — requires a
    /// `WebSocketAdapter` to be registered via `ToniApplication::use_websocket_adapter()`.
    fn get_port(&self) -> Option<u16> {
        None
    }

    /// Called once after the gateway path is registered with the adapter, before any connections.
    async fn after_init(&self) {}

    /// Connection lifecycle: called when a client connects
    async fn on_connect(&self, client: &WsClient, context: &Context) -> Result<(), WsError> {
        // Default implementation: allow all connections
        let _ = (client, context);
        Ok(())
    }

    /// Connection lifecycle: called when a client disconnects
    async fn on_disconnect(&self, client: &WsClient, reason: DisconnectReason) {
        // Default implementation: no-op
        let _ = (client, reason);
    }

    /// Route message to appropriate handler based on event name
    ///
    /// Returns Some(WsMessage) to send a response, or None for no response
    async fn handle_event(
        &self,
        client: WsClient,
        message: WsMessage,
        event: &str,
    ) -> Result<Option<WsMessage>, WsError>;

    /// Get guard tokens for DI resolution
    fn get_guard_tokens(&self) -> Vec<String> {
        vec![]
    }

    /// Get interceptor tokens for DI resolution
    fn get_interceptor_tokens(&self) -> Vec<String> {
        vec![]
    }

    /// Get pipe tokens for DI resolution
    fn get_pipe_tokens(&self) -> Vec<String> {
        vec![]
    }

    /// Get error handler tokens for DI resolution
    fn get_error_handler_tokens(&self) -> Vec<String> {
        vec![]
    }

    /// Get route metadata (permissions, rate limits, etc.)
    fn get_route_metadata(&self) -> Arc<RouteMetadata> {
        Arc::new(RouteMetadata::new())
    }
}
