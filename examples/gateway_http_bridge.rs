//! Pushing messages from an HTTP handler to WebSocket clients
//!
//! A #[websocket_gateway] is a DI provider. It can be injected into any HTTP
//! controller, and because both share the same BroadcastService instance, the
//! controller can push to all live WebSocket connections through it.
//!
//! This is the pattern for server-initiated notifications: a REST endpoint
//! triggers a broadcast to everyone currently connected over WebSocket.
//!
//! Run with:  cargo run --example gateway_http_bridge
//!
//! Connect a WebSocket client:
//!   websocat ws://127.0.0.1:3000/notifications
//!   Send:  {"event": "ping"}          — expect "pong" back
//!
//! Trigger a broadcast from HTTP (in a second terminal):
//!   curl -X POST http://127.0.0.1:3000/notify \
//!        -H "Content-Type: application/json" \
//!        -d '{"message": "hello from HTTP"}'
//!
//! Every connected WebSocket client receives the message.

use serde::Deserialize;
use toni::extractors::Json;
use toni::websocket::{BroadcastModule, BroadcastService, WsClient, WsError, WsMessage};
use toni::*;
use toni_axum::AxumAdapter;
use toni_macros::{module, websocket_gateway};

// ---- gateway -----------------------------------------------------------------

#[websocket_gateway("/notifications", pub struct NotificationGateway {
    #[inject] broadcast: BroadcastService,
})]
impl NotificationGateway {
    pub fn new(broadcast: BroadcastService) -> Self {
        Self { broadcast }
    }

    /// Called by NotifyController to push a message to all connected clients.
    pub async fn push(&self, message: &str) {
        self.broadcast
            .to_all()
            .send(WsMessage::text(message.to_string()))
            .await
            .ok();
    }

    #[subscribe_message("ping")]
    async fn on_ping(
        &self,
        _client: WsClient,
        _msg: WsMessage,
    ) -> Result<Option<WsMessage>, WsError> {
        Ok(Some(WsMessage::text("pong")))
    }
}

// ---- HTTP controller ---------------------------------------------------------

#[derive(Deserialize)]
struct NotifyPayload {
    message: String,
}

#[controller("/notify", pub struct NotifyController {
    #[inject] gateway: NotificationGateway,
})]
impl NotifyController {
    /// Accepts a JSON body and broadcasts the message to all WS clients.
    #[post("/")]
    async fn notify(&self, Json(payload): Json<NotifyPayload>) -> Body {
        self.gateway.push(&payload.message).await;
        Body::text(format!("broadcast: {}", payload.message))
    }
}

// ---- module ------------------------------------------------------------------

#[module(
    providers: [NotificationGateway],
    controllers: [NotifyController],
    imports: [BroadcastModule::new()],
)]
impl AppModule {}

// ---- main --------------------------------------------------------------------

#[tokio::main]
async fn main() {
    println!("🔔 toni gateway → HTTP bridge\n");
    println!("  WS   ws://127.0.0.1:3000/notifications");
    println!("  POST http://127.0.0.1:3000/notify  {{\"message\": \"hello\"}}");
    println!();
    println!("Every connected WS client receives messages POSTed to /notify.");
    println!();

    let mut app = ToniFactory::new()
        .create_with(AppModule::module_definition())
        .await;

    app.use_http_adapter(AxumAdapter::new("127.0.0.1", 3000))
        .unwrap();

    app.start().await;
}
