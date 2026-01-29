// Basic WebSocket example without decorators
//
// This example demonstrates manual WebSocket gateway implementation
// using the core types and traits directly (no macros).

use std::sync::Arc;
use toni::*;
use toni_axum::{AxumAdapter, AxumWebSocketAdapter};

// Manual gateway implementation
struct ChatGateway;

#[async_trait]
impl GatewayTrait for ChatGateway {
    fn get_token(&self) -> String {
        "ChatGateway".to_string()
    }

    fn get_path(&self) -> String {
        "/chat".to_string()
    }

    async fn on_connect(&self, client: &WsClient, _context: &Context) -> Result<(), WsError> {
        println!("Client {} connected!", client.id);
        Ok(())
    }

    async fn on_disconnect(&self, client: &WsClient, reason: DisconnectReason) {
        println!("Client {} disconnected: {:?}", client.id, reason);
    }

    async fn handle_event(
        &self,
        client: WsClient,
        message: WsMessage,
        event: &str,
    ) -> Result<Option<WsMessage>, WsError> {
        println!("Received event '{}' from client {}", event, client.id);

        match event {
            "message" => {
                if let Some(text) = message.as_text() {
                    println!("Message content: {}", text);

                    // Echo back the message
                    Ok(Some(WsMessage::text(format!("Echo: {}", text))))
                } else {
                    Err(WsError::InvalidMessage("Expected text message".into()))
                }
            }
            "ping" => {
                // Respond to ping
                Ok(Some(WsMessage::text("pong")))
            }
            _ => Err(WsError::EventNotFound(format!("Unknown event: {}", event))),
        }
    }
}

#[tokio::main]
async fn main() {
    println!("🚀 Starting basic WebSocket server...\n");
    println!("WebSocket endpoint: ws://127.0.0.1:8080/chat");
    println!("\nTest with:");
    println!(r#"  websocat ws://127.0.0.1:8080/chat"#);
    println!(r#"  Send: {{"event": "message", "data": "Hello"}}"#);
    println!();

    // Create WebSocket adapter
    let mut ws_adapter = AxumWebSocketAdapter::new();

    // Create gateway and wrap it
    let gateway = Arc::new(Box::new(ChatGateway) as Box<dyn GatewayTrait>);
    let wrapper = GatewayWrapper::new(
        gateway.clone(),
        vec![], // No guards
        vec![], // No interceptors
        vec![], // No pipes
        vec![], // No error handlers
        Arc::new(RouteMetadata::new()),
    );

    // Register gateway
    ws_adapter.add_gateway("/chat", Arc::new(wrapper));

    // Start WebSocket server on separate port
    println!("✅ WebSocket server listening on 127.0.0.1:8080\n");

    if let Err(e) = ws_adapter.listen(8080, "127.0.0.1").await {
        eprintln!("Error starting WebSocket server: {}", e);
    }
}
