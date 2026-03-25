//! WebSocket end-to-end integration tests
//!
//! Exercises the full path a real client message travels:
//!
//!   WS client → HTTP upgrade → AxumWsSocket → GatewayWrapper → handler → WS client
//!
//! Four tests:
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
//!
//! 3. **Separate-port** — gateway declares `port = 19001` in the macro. Verifies the
//!    full separate-port path: `get_port()` routes the gateway to `TungsteniteAdapter`
//!    rather than the HTTP adapter, and a client connecting to port 19001 gets its
//!    message handled correctly.
//!
//! 4. **Separate-port shutdown** — `app.close()` stops the tungstenite server on port 19001.
//!    Verifies that the WS port refuses new connections after shutdown.

mod common;

use common::TestServer;
use futures_util::{SinkExt, StreamExt};
use serial_test::serial;
use toni::toni_factory::ToniFactory;
use toni::websocket::{BroadcastModule, BroadcastService, WsClient, WsError, WsMessage};
use toni::{controller, module, post, Body as ToniBody};
use toni_axum::AxumAdapter;
use toni_macros::websocket_gateway;
use toni_tungstenite::TungsteniteAdapter;

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
// Separate-port gateway — `port = 19001` routes via TungsteniteAdapter, not HTTP
// ─────────────────────────────────────────────────────────────────────────────

#[websocket_gateway("/ws", port = 19001, pub struct PingGateway {})]
impl PingGateway {
    pub fn new() -> Self {
        Self {}
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

#[module(providers: [PingGateway])]
struct PingModule;

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

#[websocket_gateway("/events", pub struct EventGateway {
    #[inject] broadcast: BroadcastService,
})]
impl EventGateway {
    pub fn new(broadcast: BroadcastService) -> Self {
        Self { broadcast }
    }

    // Called by the REST controller to push a message to all connected WS clients.
    pub async fn push(&self, msg: &str) {
        self.broadcast
            .to_all()
            .send(WsMessage::text(msg.to_string()))
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

#[controller("/trigger", pub struct TriggerController {
    #[inject] gateway: EventGateway,
})]
impl TriggerController {
    #[post("/")]
    async fn trigger(&self) -> ToniBody {
        self.gateway.push("server_push").await;
        ToniBody::text("ok".to_string())
    }
}

#[module(
    providers: [EventGateway],
    controllers: [TriggerController],
    imports: [BroadcastModule::new()],
)]
struct GatewayInjectionModule;

/// Gateway injected into a REST controller.
///
/// Verifies that a `#[websocket_gateway]` struct — which is also a DI provider — can be
/// injected as a dependency into an HTTP controller, and that calling a method on the
/// injected instance broadcasts to connected WebSocket clients via the shared
/// `BroadcastService`.
///
/// Flow:
///   1. WS client connects and handshakes (ping/pong) — proves it is registered in ConnectionManager.
///   2. HTTP client POSTs to `/trigger` — controller calls `gateway.push("server_push")`.
///   3. WS client receives `"server_push"` — proves the injected gateway shares the same
///      `ConnectionManager` as the live gateway.
#[serial]
#[tokio_localset_test::localset_test]
async fn gateway_injected_into_rest_controller() {
    let server = TestServer::start(GatewayInjectionModule::module_definition()).await;
    let ws_url = format!("ws://127.0.0.1:{}/events", server.port);

    let (mut ws, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();

    // Handshake: receiving "pong" proves the client has passed complete_connect()
    // and is registered in ConnectionManager.
    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        r#"{"event": "ping"}"#.to_string().into(),
    ))
    .await
    .unwrap();
    let pong = ws.next().await.unwrap().unwrap();
    assert_eq!(pong.to_text().unwrap(), "pong");

    // Trigger broadcast from the REST handler.
    let resp = server
        .client()
        .post(server.url("/trigger"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // The WS client must receive the message pushed by the controller.
    let msg = ws.next().await.unwrap().unwrap();
    assert_eq!(msg.to_text().unwrap(), "server_push");
}

/// Separate-port path: `PingGateway` declares `port = 19001`, so the framework routes it
/// through `TungsteniteAdapter` instead of the HTTP adapter. A client connecting directly
/// to port 19001 exercises the full chain:
///
///   TCP connect → tungstenite handshake → TungsteniteWsSocket → GatewayWrapper → PingGateway
#[serial]
#[tokio_localset_test::localset_test]
async fn websocket_separate_port_end_to_end() {
    use std::time::Duration;

    // HTTP server on a throw-away port; WS gateway listens separately on 19001.
    static HTTP_PORT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(31000);
    let http_port = HTTP_PORT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

    let local = tokio::task::LocalSet::new();
    local.spawn_local(async move {
        let mut app = ToniFactory::create(PingModule::module_definition()).await;
        app.use_http_adapter(AxumAdapter::new("127.0.0.1", http_port)).unwrap();
        app.use_websocket_adapter(TungsteniteAdapter::new())
            .unwrap();
        app.start().await;
    });
    tokio::task::spawn_local(async move {
        local.await;
    });

    // Give both servers (HTTP + separate WS) time to bind.
    tokio::time::sleep(Duration::from_millis(500)).await;

    let (mut ws, _) = tokio_tungstenite::connect_async("ws://127.0.0.1:19001/ws")
        .await
        .expect("should connect to separate-port WS server");

    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        r#"{"event": "ping"}"#.to_string().into(),
    ))
    .await
    .unwrap();

    let reply = ws.next().await.unwrap().unwrap();
    assert_eq!(reply.to_text().unwrap(), "pong");
}

/// app.close() must stop the tungstenite server on port 19001.
/// Verifies via a real TCP connect attempt that the port is no longer listening.
#[serial]
#[tokio_localset_test::localset_test]
async fn separate_port_close_stops_ws_server() {
    use std::time::Duration;
    use tokio::sync::oneshot;

    static HTTP_PORT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(32000);
    let http_port = HTTP_PORT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

    let (close_tx, close_rx) = oneshot::channel::<()>();

    let local = tokio::task::LocalSet::new();
    local.spawn_local(async move {
        let mut app = ToniFactory::create(PingModule::module_definition()).await;
        app.use_http_adapter(AxumAdapter::new("127.0.0.1", http_port)).unwrap();
        app.use_websocket_adapter(TungsteniteAdapter::new())
            .unwrap();
        tokio::select! {
            _ = app.start() => {}
            _ = close_rx => {
                app.close().await.unwrap();
            }
        }
    });
    tokio::task::spawn_local(async move { local.await });

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify port 19001 is up and handling messages before shutdown.
    let (mut ws, _) = tokio_tungstenite::connect_async("ws://127.0.0.1:19001/ws")
        .await
        .expect("WS server should be reachable before close");

    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        r#"{"event": "ping"}"#.to_string().into(),
    ))
    .await
    .unwrap();
    let reply = ws.next().await.unwrap().unwrap();
    assert_eq!(reply.to_text().unwrap(), "pong");

    // Trigger graceful shutdown.
    close_tx.send(()).unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Port 19001 must refuse new connections.
    let result = tokio_tungstenite::connect_async("ws://127.0.0.1:19001/ws").await;
    assert!(
        result.is_err(),
        "WS server on port 19001 should be stopped after app.close()"
    );

    // HTTP server must also be stopped.
    let result = reqwest::get(format!("http://127.0.0.1:{}/", http_port)).await;
    assert!(
        result.is_err(),
        "HTTP server should be stopped after app.close()"
    );
}
