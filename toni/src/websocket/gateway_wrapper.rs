use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::http_helpers::RouteMetadata;
use crate::injector::{Context, Protocol};
use crate::traits_helpers::{ErrorHandler, Guard, Interceptor, InterceptorNext, Pipe};

use super::{DisconnectReason, GatewayTrait, WsClient, WsError, WsMessage};

/// InterceptorNext implementation for WebSocket interceptor chain
struct WsChainNext {
    interceptors: Vec<Arc<dyn Interceptor>>,
    gateway: Arc<Box<dyn GatewayTrait>>,
    event: String,
    pipes: Vec<Arc<dyn Pipe>>,
}

#[async_trait]
impl InterceptorNext for WsChainNext {
    async fn run(self: Box<Self>, context: &mut Context) {
        GatewayWrapper::execute_with_interceptors_impl(
            context,
            &self.interceptors,
            &self.gateway,
            &self.event,
            &self.pipes,
        )
        .await;
    }
}

/// Wrapper around gateway for execution pipeline (similar to InstanceWrapper for HTTP)
///
/// GatewayWrapper manages the complete WebSocket request lifecycle:
/// - Connection management (track connected clients)
/// - Guards execution (access control)
/// - Interceptors (pre/post processing)
/// - Pipes (data transformation)
/// - Error handling
pub struct GatewayWrapper {
    gateway: Arc<Box<dyn GatewayTrait>>,
    guards: Vec<Arc<dyn Guard>>,
    interceptors: Vec<Arc<dyn Interceptor>>,
    pipes: Vec<Arc<dyn Pipe>>,
    error_handlers: Vec<Arc<dyn ErrorHandler>>,
    route_metadata: Arc<RouteMetadata>,
    /// Active client connections (client_id => WsClient)
    clients: Arc<RwLock<HashMap<String, WsClient>>>,
}

impl GatewayWrapper {
    /// Create a new gateway wrapper
    pub fn new(
        gateway: Arc<Box<dyn GatewayTrait>>,
        guards: Vec<Arc<dyn Guard>>,
        interceptors: Vec<Arc<dyn Interceptor>>,
        pipes: Vec<Arc<dyn Pipe>>,
        error_handlers: Vec<Arc<dyn ErrorHandler>>,
        route_metadata: Arc<RouteMetadata>,
    ) -> Self {
        Self {
            gateway,
            guards,
            interceptors,
            pipes,
            error_handlers,
            route_metadata,
            clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Handle new WebSocket connection
    pub async fn handle_connect(&self, client: WsClient) -> Result<(), WsError> {
        let mut context = Context::from_websocket(
            client.clone(),
            WsMessage::text(""),
            "connect",
            Some(self.route_metadata.clone()),
        );

        // Run guards for connection
        for guard in &self.guards {
            if !guard.can_activate(&context) {
                return Err(WsError::AuthFailed("Guard rejected connection".into()));
            }

            if context.should_abort() {
                return Err(WsError::AuthFailed("Connection aborted by guard".into()));
            }
        }

        // Call lifecycle hook
        self.gateway.on_connect(&client, &context).await?;

        // Store client
        self.clients.write().await.insert(client.id.clone(), client);

        Ok(())
    }

    /// Handle WebSocket message
    pub async fn handle_message(
        &self,
        client_id: String,
        message: WsMessage,
    ) -> Result<Option<WsMessage>, WsError> {
        let client = self
            .clients
            .read()
            .await
            .get(&client_id)
            .cloned()
            .ok_or_else(|| WsError::ConnectionClosed("Client not found".into()))?;

        // Extract event name from message
        let event = self.extract_event(&message)?;

        // Create execution context
        let mut context = Context::from_websocket(
            client.clone(),
            message.clone(),
            event.clone(),
            Some(self.route_metadata.clone()),
        );

        // Run guards
        for guard in &self.guards {
            if !guard.can_activate(&context) {
                return Err(WsError::AuthFailed("Guard rejected message".into()));
            }

            if context.should_abort() {
                return Err(WsError::AuthFailed("Message aborted by guard".into()));
            }
        }

        // Execute with interceptors
        self.execute_with_interceptors(&mut context, event).await
    }

    /// Execute handler with interceptor chain
    async fn execute_with_interceptors(
        &self,
        context: &mut Context,
        event: String,
    ) -> Result<Option<WsMessage>, WsError> {
        Self::execute_with_interceptors_impl(
            context,
            &self.interceptors,
            &self.gateway,
            &event,
            &self.pipes,
        )
        .await;

        // Check if interceptor aborted
        if context.should_abort() {
            if let Some(response) = context.get_ws_response() {
                return response.clone();
            }
            return Err(WsError::Internal(
                "Request aborted by interceptor without response".into(),
            ));
        }

        if let Some(response) = context.get_ws_response() {
            response.clone()
        } else {
            Err(WsError::Internal("Handler did not set response".into()))
        }
    }

    /// Internal implementation of interceptor chain (recursive/onion pattern)
    async fn execute_with_interceptors_impl(
        context: &mut Context,
        interceptors: &[Arc<dyn Interceptor>],
        gateway: &Arc<Box<dyn GatewayTrait>>,
        event: &str,
        pipes: &[Arc<dyn Pipe>],
    ) {
        // If no interceptors, execute handler directly
        if interceptors.is_empty() {
            let _ = Self::execute_handler(context, gateway, event, pipes).await;
            return;
        }

        // Get first interceptor and remaining
        let (first, rest) = interceptors.split_first().unwrap();

        // Create the "next" handler that wraps the rest of the chain
        let next = WsChainNext {
            interceptors: rest.to_vec(),
            gateway: gateway.clone(),
            event: event.to_string(),
            pipes: pipes.to_vec(),
        };

        // Execute this interceptor with the next chain
        first.intercept(context, Box::new(next)).await;
    }

    /// Execute the actual gateway handler (pipes + gateway.handle_event)
    async fn execute_handler(
        context: &mut Context,
        gateway: &Arc<Box<dyn GatewayTrait>>,
        event: &str,
        pipes: &[Arc<dyn Pipe>],
    ) -> Result<Option<WsMessage>, WsError> {
        // Run pipes for data transformation/validation
        for pipe in pipes {
            pipe.process(context);
            if context.should_abort() {
                let result = Err(WsError::Internal("Request aborted by pipe".into()));
                context.set_ws_response(result.clone());
                return result;
            }
        }

        // Get client and message from context
        let (client, message, _) = context
            .switch_to_ws()
            .ok_or_else(|| WsError::Internal("Expected WebSocket context".into()))?;

        // Call gateway handler
        let result = gateway
            .handle_event(client.clone(), message.clone(), event)
            .await;

        context.set_ws_response(result.clone());
        result
    }

    /// Handle disconnection
    pub async fn handle_disconnect(&self, client_id: String, reason: DisconnectReason) {
        if let Some(client) = self.clients.write().await.remove(&client_id) {
            self.gateway.on_disconnect(&client, reason).await;
        }
    }

    /// Extract event name from message
    ///
    /// Supports JSON format: { "event": "message", "data": {...} }
    fn extract_event(&self, message: &WsMessage) -> Result<String, WsError> {
        match message {
            WsMessage::Text(text) => {
                // Try to parse as JSON
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text) {
                    if let Some(event) = parsed.get("event").and_then(|v| v.as_str()) {
                        return Ok(event.to_string());
                    }
                }

                // If no event field, treat the whole message as the event
                Err(WsError::InvalidMessage(
                    "Missing 'event' field in JSON message".into(),
                ))
            }
            WsMessage::Binary(_) => Err(WsError::InvalidMessage(
                "Binary messages not yet supported for event extraction".into(),
            )),
            WsMessage::Ping(_) | WsMessage::Pong(_) => Err(WsError::InvalidMessage(
                "Ping/Pong are control frames".into(),
            )),
        }
    }

    /// Get all connected clients
    pub async fn get_clients(&self) -> Vec<WsClient> {
        self.clients.read().await.values().cloned().collect()
    }

    /// Get specific client by ID
    pub async fn get_client(&self, client_id: &str) -> Option<WsClient> {
        self.clients.read().await.get(client_id).cloned()
    }

    /// Get gateway path
    pub fn get_path(&self) -> String {
        self.gateway.get_path()
    }

    /// Get gateway namespace
    pub fn get_namespace(&self) -> Option<String> {
        self.gateway.get_namespace()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_event_from_json() {
        let wrapper = create_test_wrapper();

        let msg = WsMessage::text(r#"{"event": "message", "data": "hello"}"#);
        let event = wrapper.extract_event(&msg).unwrap();
        assert_eq!(event, "message");
    }

    #[test]
    fn test_extract_event_missing_field() {
        let wrapper = create_test_wrapper();

        let msg = WsMessage::text(r#"{"data": "hello"}"#);
        let result = wrapper.extract_event(&msg);
        assert!(result.is_err());
    }

    fn create_test_wrapper() -> GatewayWrapper {
        struct TestGateway;

        #[async_trait::async_trait]
        impl GatewayTrait for TestGateway {
            fn get_token(&self) -> String {
                "TestGateway".to_string()
            }

            fn get_path(&self) -> String {
                "/test".to_string()
            }

            async fn handle_event(
                &self,
                _client: WsClient,
                _message: WsMessage,
                _event: &str,
            ) -> Result<Option<WsMessage>, WsError> {
                Ok(None)
            }
        }

        GatewayWrapper::new(
            Arc::new(Box::new(TestGateway)),
            vec![],
            vec![],
            vec![],
            vec![],
            Arc::new(RouteMetadata::new()),
        )
    }
}
