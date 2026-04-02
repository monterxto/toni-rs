//! Simple WebSocket echo/chat example
//!
//! The minimal WebSocket setup — no BroadcastService, no rooms.
//! Each message is echoed back to the sender (request-response).
//!
//! BroadcastService is *optional*: when you don't inject it and don't import
//! BroadcastModule, the framework uses the simple `handle_connection()` path
//! automatically. No broadcast infrastructure is wired in.
//!
//! Compare with `websocket_di.rs` which adds DI, guards, and broadcasting.
//!
//! Run with:  cargo run --example websocket_chat
//! Connect:   websocat ws://127.0.0.1:8080/chat
//! Send:      {"event": "message", "data": "Hello"}
//!            {"event": "ping"}

use toni::*;
use toni_macros::{module, websocket_gateway};

#[websocket_gateway("/chat", pub struct EchoGateway {})]
impl EchoGateway {
    pub fn new() -> Self {
        Self {}
    }

    #[subscribe_message("message")]
    async fn handle_message(
        &self,
        client: WsClient,
        message: WsMessage,
    ) -> Result<Option<WsMessage>, WsError> {
        let text = message
            .as_text()
            .ok_or_else(|| WsError::InvalidMessage("Expected text message".into()))?;

        println!("[{}] {}", client.id, text);

        Ok(Some(WsMessage::text(format!("Echo: {}", text))))
    }

    #[subscribe_message("ping")]
    async fn handle_ping(
        &self,
        _client: WsClient,
        _message: WsMessage,
    ) -> Result<Option<WsMessage>, WsError> {
        Ok(Some(WsMessage::text("pong")))
    }
}

// No BroadcastModule — BroadcastService is not needed here.
// The framework detects this and uses the simple request-response path.
#[module(providers: [EchoGateway])]
struct AppModule;

#[tokio::main]
async fn main() {
    println!("🚀 Simple WebSocket echo server\n");
    println!("Connect:  websocat ws://127.0.0.1:8080/chat");
    println!(r#"Send:     {{"event": "message", "data": "Hello"}}"#);
    println!(r#"          {{"event": "ping"}}"#);
    println!();

    let mut app = ToniFactory::new().create_with(AppModule).await;

    app.use_http_adapter(toni_axum::AxumAdapter::new(), 8080, "127.0.0.1")
        .unwrap();

    app.start().await;
}
