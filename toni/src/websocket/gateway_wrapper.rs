use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::http_helpers::RouteMetadata;
use crate::injector::{Context, Protocol};
use crate::traits_helpers::{ErrorHandler, Guard, Interceptor, InterceptorNext, Pipe};

use super::{DisconnectReason, GatewayTrait, WsClient, WsError, WsMessage};

struct WsChainNext {
    interceptors: Vec<Arc<dyn Interceptor>>,
    gateway: Arc<Box<dyn GatewayTrait>>,
    event: String,
    pipes: Vec<Arc<dyn Pipe>>,
    error_handlers: Vec<Arc<dyn ErrorHandler>>,
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
            &self.error_handlers,
        )
        .await;
    }
}

/// Parallel to `InstanceWrapper` on the HTTP side — wraps a gateway with the full
/// guard/interceptor/pipe pipeline and tracks its own connected clients.
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

    /// Phase 1 of connection setup: run guards and store client.
    ///
    /// Does NOT fire `on_connect` — call `complete_connect` after any external
    /// registration (e.g. `ConnectionManager`) so the hook fires when the client
    /// is fully live everywhere.
    pub async fn begin_connect(&self, client: WsClient) -> Result<(), WsError> {
        let context = Context::from_websocket(
            client.clone(),
            WsMessage::text(""),
            "connect",
            Some(self.route_metadata.clone()),
        );

        for guard in &self.guards {
            if !guard.can_activate(&context) {
                return Err(WsError::AuthFailed("Guard rejected connection".into()));
            }
            if context.should_abort() {
                return Err(WsError::AuthFailed("Connection aborted by guard".into()));
            }
        }

        self.clients.write().await.insert(client.id.clone(), client);
        Ok(())
    }

    /// Phase 2 of connection setup: fire the `on_connect` lifecycle hook.
    ///
    /// Must be called after `begin_connect` and after any external registration
    /// (e.g. `ConnectionManager`). When this fires, the client is in both
    /// `GatewayWrapper.clients` and `ConnectionManager`.
    pub async fn complete_connect(&self, client_id: &str) -> Result<(), WsError> {
        let client = self
            .clients
            .read()
            .await
            .get(client_id)
            .cloned()
            .ok_or_else(|| WsError::ConnectionClosed("Client not found".into()))?;

        let context = Context::from_websocket(
            client.clone(),
            WsMessage::text(""),
            "connect",
            Some(self.route_metadata.clone()),
        );

        self.gateway.on_connect(&client, &context).await
    }

    /// Handle new WebSocket connection (simple path — no ConnectionManager).
    ///
    /// Composes `begin_connect` + `complete_connect` in sequence. Used by
    /// `handle_connection()` where there is no broadcast infrastructure.
    pub async fn handle_connect(&self, client: WsClient) -> Result<(), WsError> {
        let client_id = client.id.clone();
        self.begin_connect(client).await?;
        self.complete_connect(&client_id).await
    }

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

        let event = self.extract_event(&message)?;

        let mut context = Context::from_websocket(
            client.clone(),
            message.clone(),
            event.clone(),
            Some(self.route_metadata.clone()),
        );

        for guard in &self.guards {
            if !guard.can_activate(&context) {
                return Err(WsError::AuthFailed("Guard rejected message".into()));
            }

            if context.should_abort() {
                return Err(WsError::AuthFailed("Message aborted by guard".into()));
            }
        }

        self.execute_with_interceptors(&mut context, event).await
    }

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
            &self.error_handlers,
        )
        .await;

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

    /// Stores the result in context rather than returning it directly (mirrors HTTP's pattern);
    /// retrieve via `context.get_ws_response()` after calling.
    async fn execute_with_interceptors_impl(
        context: &mut Context,
        interceptors: &[Arc<dyn Interceptor>],
        gateway: &Arc<Box<dyn GatewayTrait>>,
        event: &str,
        pipes: &[Arc<dyn Pipe>],
        error_handlers: &[Arc<dyn ErrorHandler>],
    ) {
        if interceptors.is_empty() {
            Self::execute_handler_with_error_handling(
                context,
                gateway,
                event,
                pipes,
                error_handlers,
            )
            .await;
            return;
        }

        let (first, rest) = interceptors.split_first().unwrap();

        let next = WsChainNext {
            interceptors: rest.to_vec(),
            gateway: gateway.clone(),
            event: event.to_string(),
            pipes: pipes.to_vec(),
            error_handlers: error_handlers.to_vec(),
        };

        first.intercept(context, Box::new(next)).await;
    }

    async fn execute_handler_with_error_handling(
        context: &mut Context,
        gateway: &Arc<Box<dyn GatewayTrait>>,
        event: &str,
        pipes: &[Arc<dyn Pipe>],
        error_handlers: &[Arc<dyn ErrorHandler>],
    ) {
        let _ = Self::execute_handler(context, gateway, event, pipes).await;

        // TODO: Implement error handler execution for WebSocket
        // Currently ErrorHandler trait is HTTP-specific (takes HttpRequest, returns HttpResponse)
        // Need to either:
        // 1. Make ErrorHandler protocol-agnostic (breaking change)
        // 2. Create WsErrorHandler trait
        // 3. Add handle_ws_error method to ErrorHandler trait
        //
        // For now, error_handlers are resolved and stored but not executed.
        // When a WsError occurs, it's returned directly to the adapter.

        if !error_handlers.is_empty() {
            if let Some(result) = context.get_ws_response() {
                if result.is_err() {}
            }
        }
    }

    async fn execute_handler(
        context: &mut Context,
        gateway: &Arc<Box<dyn GatewayTrait>>,
        event: &str,
        pipes: &[Arc<dyn Pipe>],
    ) -> Result<Option<WsMessage>, WsError> {
        for pipe in pipes {
            pipe.process(context);
            if context.should_abort() {
                let result = Err(WsError::Internal("Request aborted by pipe".into()));
                context.set_ws_response(result.clone());
                return result;
            }
        }

        let (client, message, _) = context
            .switch_to_ws()
            .ok_or_else(|| WsError::Internal("Expected WebSocket context".into()))?;

        let result = gateway
            .handle_event(client.clone(), message.clone(), event)
            .await;

        context.set_ws_response(result.clone());
        result
    }

    pub async fn handle_disconnect(&self, client_id: String, reason: DisconnectReason) {
        if let Some(client) = self.clients.write().await.remove(&client_id) {
            self.gateway.on_disconnect(&client, reason).await;
        }
    }

    /// Parses the event name from a message; expects JSON `{ "event": "...", ... }` format
    fn extract_event(&self, message: &WsMessage) -> Result<String, WsError> {
        match message {
            WsMessage::Text(text) => {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text) {
                    if let Some(event) = parsed.get("event").and_then(|v| v.as_str()) {
                        return Ok(event.to_string());
                    }
                }

                Err(WsError::InvalidMessage(
                    "Missing 'event' field in JSON message".into(),
                ))
            }
            WsMessage::Binary(_) => Err(WsError::InvalidMessage(
                "Binary messages not yet supported for event extraction".into(),
            )),
            WsMessage::Ping(_) | WsMessage::Pong(_) | WsMessage::Close => Err(
                WsError::InvalidMessage("Control frames don't have events".into()),
            ),
        }
    }

    pub async fn get_clients(&self) -> Vec<WsClient> {
        self.clients.read().await.values().cloned().collect()
    }

    pub async fn get_client(&self, client_id: &str) -> Option<WsClient> {
        self.clients.read().await.get(client_id).cloned()
    }

    pub fn get_path(&self) -> String {
        self.gateway.get_path()
    }

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
