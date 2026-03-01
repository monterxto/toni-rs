//! WebSocket end-to-end integration tests
//!
//! Exercises the full path a real client message travels:
//!
//!   WS client → HTTP upgrade → AxumWsSocket → GatewayWrapper → handler → WS client
//!
//! Three tests:
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
//!    full separate-port path: `get_port()` routes the gateway to `WebSocketAdapter`
//!    rather than the HTTP adapter, `TestWsAdapter.listen()` starts a real TCP server,
//!    and a client connecting to port 19001 gets its message handled correctly.

mod common;

use common::TestServer;
use futures_util::{SinkExt, StreamExt};
use serial_test::serial;
use toni::module;
use toni::websocket::{BroadcastModule, BroadcastService, GatewayWrapper, WsClient, WsError, WsMessage, WsSocket};
use toni::WebSocketAdapter;
use toni_macros::websocket_gateway;
use toni_axum::AxumAdapter;
use toni::toni_factory::ToniFactory;
use std::collections::HashMap;
use std::sync::Arc;

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

// ─────────────────────────────────────────────────────────────────────────────
// Separate-port gateway — `port = 19001` routes via WebSocketAdapter, not HTTP
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

/// Real tokio-tungstenite TCP server used in the separate-port test.
///
/// `on_gateway()` stores path→gateway registrations from the framework.
/// `listen()` binds a TCP port, upgrades each connection with tungstenite,
/// then runs the connection through `WsSocket::handle_connection()`.
struct TestWsAdapter {
    gateways: HashMap<String, Arc<GatewayWrapper>>,
}

impl TestWsAdapter {
    fn new() -> Self {
        Self {
            gateways: HashMap::new(),
        }
    }
}

#[toni::async_trait]
impl WebSocketAdapter for TestWsAdapter {
    fn on_gateway(&mut self, path: &str, gateway: Arc<GatewayWrapper>) -> anyhow::Result<()> {
        self.gateways.insert(path.to_string(), gateway);
        Ok(())
    }

    async fn listen(&mut self, port: u16, hostname: &str) -> anyhow::Result<()> {
        let addr = format!("{}:{}", hostname, port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        let gateways: Arc<HashMap<String, Arc<GatewayWrapper>>> =
            Arc::new(self.gateways.clone());

        loop {
            let (stream, _) = listener.accept().await?;
            let gateways = gateways.clone();
            tokio::spawn(async move {
                let ws_stream = match tokio_tungstenite::accept_async(stream).await {
                    Ok(ws) => ws,
                    Err(e) => {
                        eprintln!("WS handshake error: {}", e);
                        return;
                    }
                };
                // Route to the only registered gateway on this port
                if let Some(gateway) = gateways.values().next() {
                    let gateway = gateway.clone();
                    let mut socket = TungsteniteSocket { inner: ws_stream };
                    socket.handle_connection(&gateway, HashMap::new()).await;
                }
            });
        }
    }
}

/// `WsSocket` impl backed by a tungstenite stream over a raw TCP connection.
struct TungsteniteSocket {
    inner: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
}

#[toni::async_trait]
impl WsSocket for TungsteniteSocket {
    async fn recv(&mut self) -> Option<Result<WsMessage, WsError>> {
        use tokio_tungstenite::tungstenite::Message;
        loop {
            return match self.inner.next().await? {
                Ok(Message::Text(t)) => Some(Ok(WsMessage::Text(t.to_string()))),
                Ok(Message::Binary(b)) => Some(Ok(WsMessage::Binary(b.to_vec()))),
                Ok(Message::Close(_)) => None,
                Ok(Message::Ping(_)) | Ok(Message::Pong(_)) | Ok(Message::Frame(_)) => continue,
                Err(e) => Some(Err(WsError::Internal(e.to_string()))),
            };
        }
    }

    async fn send(&mut self, msg: WsMessage) -> Result<(), WsError> {
        use tokio_tungstenite::tungstenite::Message;
        let m = match msg {
            WsMessage::Text(t) => Message::Text(t.into()),
            WsMessage::Binary(b) => Message::Binary(b.into()),
            WsMessage::Ping(d) => Message::Ping(d.into()),
            WsMessage::Pong(d) => Message::Pong(d.into()),
            WsMessage::Close => Message::Close(None),
        };
        self.inner
            .send(m)
            .await
            .map_err(|e| WsError::Internal(e.to_string()))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

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

/// Separate-port path: `PingGateway` declares `port = 19001`, so the framework routes it
/// through `WebSocketAdapter` instead of the HTTP adapter. `TestWsAdapter.listen()` starts
/// a real TCP server; a client connecting directly to port 19001 exercises the full chain:
///
///   TCP connect → tungstenite handshake → TungsteniteSocket → GatewayWrapper → PingGateway
#[serial]
#[tokio_localset_test::localset_test]
async fn websocket_separate_port_end_to_end() {
    use std::time::Duration;

    // HTTP server on a throw-away port; WS gateway listens separately on 19001.
    static HTTP_PORT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(31000);
    let http_port = HTTP_PORT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

    let local = tokio::task::LocalSet::new();
    local.spawn_local(async move {
        let mut app =
            ToniFactory::create(PingModule::module_definition(), AxumAdapter::new()).await;
        app.use_websocket_adapter(TestWsAdapter::new()).unwrap();
        app.listen(http_port, "127.0.0.1").await;
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
