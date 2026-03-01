//! WebSocket end-to-end integration tests
//!
//! Exercises the full path a real client message travels:
//!
//!   WS client → HTTP upgrade → AxumWsSocket → GatewayWrapper → handler → WS client
//!
//! Two tests:
//!
//! 1. **Echo** — simple gateway, no `BroadcastModule`. Verifies the `handle_connection()`
//!    (simple) path end-to-end with a real TCP connection.
//!
//! 2. **Broadcast** — `BroadcastModule` imported, two clients. Verifies the
//!    `handle_connection_with_broadcast()` path: a message sent by one client is
//!    received by both via `BroadcastService::to_all()`.
//!
//!    Race-free via handshake: each client sends `{"event":"ping"}` and waits for `"pong"`
//!    before the broadcast is sent. Receiving `"pong"` proves the client has passed
//!    `complete_connect()` and is registered in `ConnectionManager`.

mod common;

use common::TestServer;
use futures_util::{SinkExt, StreamExt};
use serial_test::serial;
use toni::module;
use toni::websocket::{BroadcastModule, BroadcastService, WsClient, WsError, WsMessage};
use toni_macros::websocket_gateway;

// ─────────────────────────────────────────────────────────────────────────────
// Echo gateway — simple request-response, no BroadcastModule
// ─────────────────────────────────────────────────────────────────────────────

#[websocket_gateway("/echo", pub struct EchoGateway {})]
impl EchoGateway {
    pub fn new() -> Self {
        Self {}
    }

    #[subscribe_message("message")]
    async fn on_message(
        &self,
        _client: WsClient,
        msg: WsMessage,
    ) -> Result<Option<WsMessage>, WsError> {
        let text = msg
            .as_text()
            .ok_or_else(|| WsError::InvalidMessage("Expected text".into()))?;
        Ok(Some(WsMessage::text(format!("Echo: {}", text))))
    }
}

#[module(providers: [EchoGateway])]
struct EchoModule;

// ─────────────────────────────────────────────────────────────────────────────
// Room gateway — broadcast-aware, BroadcastModule required
// ─────────────────────────────────────────────────────────────────────────────

#[websocket_gateway("/room", pub struct RoomGateway {
    #[inject] broadcast: BroadcastService,
})]
impl RoomGateway {
    pub fn new(broadcast: BroadcastService) -> Self {
        Self { broadcast }
    }

    /// Handshake: proves the client is fully registered in ConnectionManager.
    /// Response routes back to sender only (via CM.send_to_clients in broadcast mode).
    #[subscribe_message("ping")]
    async fn on_ping(
        &self,
        _client: WsClient,
        _msg: WsMessage,
    ) -> Result<Option<WsMessage>, WsError> {
        Ok(Some(WsMessage::text("pong")))
    }

    /// Broadcast the raw message text to every connected client.
    #[subscribe_message("shout")]
    async fn on_shout(
        &self,
        _client: WsClient,
        msg: WsMessage,
    ) -> Result<Option<WsMessage>, WsError> {
        let text = msg
            .as_text()
            .ok_or_else(|| WsError::InvalidMessage("Expected text".into()))?;
        self.broadcast
            .to_all()
            .send(WsMessage::text(text.to_string()))
            .await
            .ok();
        Ok(None)
    }
}

#[module(providers: [RoomGateway], imports: [BroadcastModule::new()])]
struct RoomModule;

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

/// Full path: TCP upgrade → AxumWsSocket → GatewayWrapper → echo handler → response.
/// Uses the simple `handle_connection()` path (no BroadcastModule).
#[serial]
#[tokio_localset_test::localset_test]
async fn websocket_echo_end_to_end() {
    let server = TestServer::start(EchoModule::module_definition()).await;
    let ws_url = format!("ws://127.0.0.1:{}/echo", server.port);

    let (mut ws, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();

    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        r#"{"event": "message", "data": "hello"}"#.to_string().into(),
    ))
    .await
    .unwrap();

    let msg = ws.next().await.unwrap().unwrap();
    assert_eq!(
        msg.to_text().unwrap(),
        r#"Echo: {"event": "message", "data": "hello"}"#,
    );
}

/// Full path: two real TCP clients, `handle_connection_with_broadcast()` path.
/// Race-free: each client handshakes with ping/pong before the broadcast is sent,
/// proving it has passed `complete_connect()` and is registered in `ConnectionManager`.
#[serial]
#[tokio_localset_test::localset_test]
async fn websocket_broadcast_end_to_end() {
    let server = TestServer::start(RoomModule::module_definition()).await;
    let ws_url = format!("ws://127.0.0.1:{}/room", server.port);

    let (mut client_a, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();
    let (mut client_b, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();

    // Handshake: wait for both clients to be registered in ConnectionManager.
    // Receiving "pong" means the server's message loop is running, which only
    // starts after begin_connect → CM.register() → complete_connect() have all run.
    for ws in [&mut client_a, &mut client_b] {
        ws.send(tokio_tungstenite::tungstenite::Message::Text(
            r#"{"event": "ping"}"#.to_string().into(),
        ))
        .await
        .unwrap();
        let pong = ws.next().await.unwrap().unwrap();
        assert_eq!(pong.to_text().unwrap(), "pong");
    }

    // Both clients are now guaranteed to be in ConnectionManager.
    client_a
        .send(tokio_tungstenite::tungstenite::Message::Text(
            r#"{"event": "shout", "data": "hello room"}"#.to_string().into(),
        ))
        .await
        .unwrap();

    let recv_a = client_a.next().await.unwrap().unwrap();
    let recv_b = client_b.next().await.unwrap().unwrap();

    assert_eq!(
        recv_a.to_text().unwrap(),
        r#"{"event": "shout", "data": "hello room"}"#,
    );
    assert_eq!(
        recv_b.to_text().unwrap(),
        r#"{"event": "shout", "data": "hello room"}"#,
    );
}
