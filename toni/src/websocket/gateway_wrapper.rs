use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::http_helpers::RouteMetadata;
use crate::injector::{Context, Protocol};
use crate::traits_helpers::{ErrorHandler, Guard, Interceptor, Pipe};

use super::{DisconnectReason, GatewayTrait, WsClient, WsError, WsMessage};

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
        // TODO: Implement interceptor chain similar to InstanceWrapper
        // For now, directly call handler

        // Run pipes for data transformation/validation
        for pipe in &self.pipes {
            pipe.process(context);
        }

        // Get client and message from context
        let (client, message, _) = context
            .switch_to_ws()
            .ok_or_else(|| WsError::Internal("Expected WebSocket context".into()))?;

        // Call gateway handler
        let result = self
            .gateway
            .handle_event(client.clone(), message.clone(), &event)
            .await;

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
            WsMessage::Ping(_) | WsMessage::Pong(_) => {
                Err(WsError::InvalidMessage("Ping/Pong are control frames".into()))
            }
        }
    }

    /// Get all connected clients
    pub async fn get_clients(&self) -> Vec<WsClient> {
        self.clients
            .read()
            .await
            .values()
            .cloned()
            .collect()
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
