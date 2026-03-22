//! Graceful shutdown integration tests
//!
//! Verifies that `app.close()`:
//!   1. Sends close frames to connected WebSocket clients
//!   2. Stops the HTTP server from accepting new connections
//!   3. Runs the `on_module_destroy` lifecycle hook

use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serial_test::serial;
use tokio::sync::oneshot;
use toni::module;
use toni::toni_factory::ToniFactory;
use toni::websocket::{BroadcastModule, BroadcastService, WsClient, WsError, WsMessage};
use toni_axum::AxumAdapter;
use toni_macros::websocket_gateway;

static PORT_COUNTER: AtomicU16 = AtomicU16::new(33000);
static DESTROY_HOOK_RAN: AtomicBool = AtomicBool::new(false);

#[websocket_gateway("/ws", pub struct CloseGateway {
    #[inject] broadcast: BroadcastService,
})]
impl CloseGateway {
    pub fn new(broadcast: BroadcastService) -> Self {
        Self { broadcast }
    }

    #[on_module_destroy]
    async fn on_destroy(&self) {
        DESTROY_HOOK_RAN.store(true, Ordering::SeqCst);
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

#[module(providers: [CloseGateway], imports: [BroadcastModule::new()])]
struct CloseModule;

/// app.close() sends WS close frames, stops the HTTP server, and fires on_module_destroy.
#[serial]
#[tokio_localset_test::localset_test]
async fn app_close_disconnects_ws_clients_and_stops_http() {
    DESTROY_HOOK_RAN.store(false, Ordering::SeqCst);

    let port = PORT_COUNTER.fetch_add(1, Ordering::SeqCst);
    let (close_tx, close_rx) = oneshot::channel::<()>();

    let local = tokio::task::LocalSet::new();
    local.spawn_local(async move {
        let mut app = ToniFactory::create(CloseModule::module_definition()).await;
        app.use_http_adapter(AxumAdapter::new("127.0.0.1", port)).unwrap();
        tokio::select! {
            _ = app.start() => {}
            _ = close_rx => {
                app.close().await.unwrap();
            }
        }
    });
    tokio::task::spawn_local(async move { local.await });

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify the WS gateway is reachable before shutdown.
    let ws_url = format!("ws://127.0.0.1:{}/ws", port);
    let (mut ws, _) = tokio_tungstenite::connect_async(&ws_url).await.unwrap();

    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        r#"{"event": "ping"}"#.to_string().into(),
    ))
    .await
    .unwrap();
    let pong = ws.next().await.unwrap().unwrap();
    assert_eq!(pong.to_text().unwrap(), "pong");

    // Trigger graceful shutdown.
    close_tx.send(()).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // The server must have sent a close frame (or dropped the connection).
    let next = ws.next().await;
    assert!(
        matches!(
            next,
            None | Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_)))
        ),
        "expected WS close after app.close(), got {:?}",
        next
    );

    // HTTP server must no longer accept new connections.
    let result = reqwest::get(format!("http://127.0.0.1:{}/", port)).await;
    assert!(
        result.is_err(),
        "HTTP server should be stopped after app.close()"
    );

    // on_module_destroy must have run.
    assert!(
        DESTROY_HOOK_RAN.load(Ordering::SeqCst),
        "on_module_destroy hook should run during app.close()"
    );
}
