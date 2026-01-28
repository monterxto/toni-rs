use toni_macros::websocket_gateway;

#[websocket_gateway("/chat", pub struct ChatGateway {})]
impl ChatGateway {
    #[subscribe_message("message")]
    async fn handle_message(
        &self,
        client: toni::WsClient,
        message: toni::WsMessage,
    ) -> Result<Option<toni::WsMessage>, toni::WsError> {
        let text = message.as_text().ok_or_else(|| {
            toni::WsError::InvalidMessage("Expected text message".into())
        })?;

        println!("Received message from {}: {}", client.id, text);

        Ok(Some(toni::WsMessage::text(format!("Echo: {}", text))))
    }

    #[subscribe_message("ping")]
    async fn handle_ping(
        &self,
        _client: toni::WsClient,
        _message: toni::WsMessage,
    ) -> Result<Option<toni::WsMessage>, toni::WsError> {
        Ok(Some(toni::WsMessage::text("pong")))
    }

    #[subscribe_message("join")]
    async fn handle_join(
        &self,
        client: toni::WsClient,
        message: toni::WsMessage,
    ) -> Result<Option<toni::WsMessage>, toni::WsError> {
        let username = message.as_text().ok_or_else(|| {
            toni::WsError::InvalidMessage("Expected username".into())
        })?;

        println!("User {} joined as {}", client.id, username);

        Ok(Some(toni::WsMessage::text(format!(
            "Welcome, {}! You are connected.",
            username
        ))))
    }
}

#[tokio::main]
async fn main() {
    use std::sync::Arc;
    use toni::{GatewayWrapper, WsMessage, WebSocketAdapter};
    use toni_axum::AxumWebSocketAdapter;
    use toni::http_helpers::RouteMetadata;

    println!("Starting WebSocket chat server with decorator-based gateway...");

    let mut ws_adapter = AxumWebSocketAdapter::new();

    // Create gateway instance
    let gateway = Arc::new(Box::new(ChatGateway {}) as Box<dyn toni::GatewayTrait>);

    // Wrap with pipeline
    let wrapper = GatewayWrapper::new(
        gateway,
        vec![],
        vec![],
        vec![],
        vec![],
        Arc::new(RouteMetadata::new()),
    );

    ws_adapter.add_gateway("/chat", Arc::new(wrapper));

    println!("WebSocket server listening on ws://127.0.0.1:8080/chat");
    println!("\nTest with wscat:");
    println!("  wscat -c ws://127.0.0.1:8080/chat");
    println!("  > {{\"event\": \"join\", \"data\": \"Alice\"}}");
    println!("  > {{\"event\": \"message\", \"data\": \"Hello everyone!\"}}");
    println!("  > {{\"event\": \"ping\"}}");

    ws_adapter.listen(8080, "127.0.0.1").await.unwrap();
}
