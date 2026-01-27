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

    /// Get message handler instances (for macro-generated gateways)
    fn get_message_handlers(&self) -> HashMap<String, Arc<Box<dyn MessageHandlerTrait>>> {
        HashMap::new()
    }

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

/// Individual message handler trait for specific events
///
/// Each #[subscribe_message("event")] method generates an implementation of this trait
#[async_trait]
pub trait MessageHandlerTrait: Send + Sync {
    /// Handle specific event
    ///
    /// Returns Some(WsMessage) to send a response, or None for no response
    async fn handle(&self, context: &mut Context) -> Result<Option<WsMessage>, WsError>;

    /// Event name this handler responds to
    fn event_name(&self) -> &str;

    /// Method-level guards
    fn get_guard_tokens(&self) -> Vec<String> {
        vec![]
    }

    /// Method-level interceptors
    fn get_interceptor_tokens(&self) -> Vec<String> {
        vec![]
    }

    /// Method-level pipes
    fn get_pipe_tokens(&self) -> Vec<String> {
        vec![]
    }
}
