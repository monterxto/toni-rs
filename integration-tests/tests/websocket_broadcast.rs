//! WebSocket broadcast integration tests
//!
//! Two layers of coverage:
//!
//! 1. **Broadcast pipeline** — `ConnectionManager` + `BroadcastService` + `TokioSender`
//!    wired together without a real TCP connection. Verifies the full message-routing
//!    path from `BroadcastService` API down to the stored `Sender` channel.
//!
//! 2. **DI resolution** — `BroadcastModule` registers `ConnectionManager` and
//!    `BroadcastService` with `type_name`-based tokens that the framework resolves
//!    correctly. Verifies the token fix that replaced simple string tokens.

use std::sync::Arc;
use tokio::sync::mpsc;
use toni::websocket::{BroadcastService, ConnectionManager, WsMessage};
use toni::WsClient;
use toni_axum::TokioSender;

// Helpers

/// Returns the receiver so tests can assert what the client received.
fn register_client(
    cm: &Arc<ConnectionManager>,
    client_id: &str,
    namespace: Option<String>,
) -> mpsc::Receiver<WsMessage> {
    let (tx, rx) = mpsc::channel(16);
    let sender = Arc::new(TokioSender::new(tx));
    cm.register(WsClient::new(client_id), sender, namespace);
    rx
}

// Broadcast pipeline tests (no DI, no TCP)

#[tokio::test]
async fn broadcast_to_all_delivers_to_every_registered_client() {
    let cm = Arc::new(ConnectionManager::new());
    let bs = BroadcastService::new(cm.clone());

    let mut rx1 = register_client(&cm, "client-1", None);
    let mut rx2 = register_client(&cm, "client-2", None);
    let mut rx3 = register_client(&cm, "client-3", None);

    let msg = WsMessage::text("hello everyone");
    let sent = bs.to_all().send(msg.clone()).await.unwrap();

    assert_eq!(sent, 3, "should have delivered to 3 clients");
    assert_eq!(rx1.recv().await.unwrap().as_text(), Some("hello everyone"));
    assert_eq!(rx2.recv().await.unwrap().as_text(), Some("hello everyone"));
    assert_eq!(rx3.recv().await.unwrap().as_text(), Some("hello everyone"));
}

#[tokio::test]
async fn broadcast_to_room_delivers_only_to_room_members() {
    let cm = Arc::new(ConnectionManager::new());
    let bs = BroadcastService::new(cm.clone());

    let mut lobby_rx1 = register_client(&cm, "alice", None);
    let mut lobby_rx2 = register_client(&cm, "bob", None);
    let mut other_rx = register_client(&cm, "carol", None);

    bs.join_room("alice", "lobby").unwrap();
    bs.join_room("bob", "lobby").unwrap();
    // carol is NOT in lobby

    let sent = bs
        .to_room("lobby")
        .send(WsMessage::text("room message"))
        .await
        .unwrap();

    assert_eq!(sent, 2, "should have delivered to 2 room members");
    assert_eq!(
        lobby_rx1.recv().await.unwrap().as_text(),
        Some("room message")
    );
    assert_eq!(
        lobby_rx2.recv().await.unwrap().as_text(),
        Some("room message")
    );
    assert!(
        other_rx.try_recv().is_err(),
        "client not in room should not receive message"
    );
}

#[tokio::test]
async fn broadcast_to_client_delivers_only_to_target() {
    let cm = Arc::new(ConnectionManager::new());
    let bs = BroadcastService::new(cm.clone());

    let mut target_rx = register_client(&cm, "target", None);
    let mut bystander_rx = register_client(&cm, "bystander", None);

    bs.to_client("target")
        .send(WsMessage::text("private"))
        .await
        .unwrap();

    assert_eq!(target_rx.recv().await.unwrap().as_text(), Some("private"));
    assert!(
        bystander_rx.try_recv().is_err(),
        "bystander should not receive private message"
    );
}

#[tokio::test]
async fn unregistered_client_stops_receiving_after_disconnect() {
    let cm = Arc::new(ConnectionManager::new());
    let bs = BroadcastService::new(cm.clone());

    let mut rx = register_client(&cm, "leaving", None);

    bs.to_all().send(WsMessage::text("before")).await.unwrap();
    assert_eq!(rx.recv().await.unwrap().as_text(), Some("before"));

    cm.unregister("leaving");

    let sent = bs.to_all().send(WsMessage::text("after")).await.unwrap();
    assert_eq!(sent, 0, "no clients registered after unregister");
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn leave_room_stops_room_messages() {
    let cm = Arc::new(ConnectionManager::new());
    let bs = BroadcastService::new(cm.clone());

    let mut rx = register_client(&cm, "user", None);

    bs.join_room("user", "general").unwrap();
    bs.to_room("general")
        .send(WsMessage::text("while in room"))
        .await
        .unwrap();
    assert_eq!(rx.recv().await.unwrap().as_text(), Some("while in room"));

    bs.leave_room("user", "general").unwrap();
    let sent = bs
        .to_room("general")
        .send(WsMessage::text("after leave"))
        .await
        .unwrap();
    assert_eq!(sent, 0);
    assert!(rx.try_recv().is_err());
}

// DI resolution tests — verify BroadcastModule tokens are correct

#[allow(unused_imports)]
mod di_tests {
    use serial_test::serial;
    use std::sync::Arc;
    use toni::module;
    use toni::toni_factory::ToniFactory;
    use toni::websocket::{BroadcastModule, BroadcastService, ConnectionManager};
    use toni_axum::AxumAdapter;

    #[module(imports: [BroadcastModule::new()])]
    struct WsTestModule;

    /// `BroadcastModule` must register `Arc<ConnectionManager>` under the token
    /// `type_name::<Arc<ConnectionManager>>()` — the same token the macro generates
    /// when a gateway field is typed `connection_manager: Arc<ConnectionManager>`.
    #[serial]
    #[tokio_localset_test::localset_test]
    async fn websocket_module_provides_connection_manager_via_type_name_token() {
        let app = ToniFactory::create(WsTestModule::module_definition(), AxumAdapter::new()).await;
        let result = app.get::<Arc<ConnectionManager>>().await;
        assert!(
            result.is_ok(),
            "ConnectionManager not found — likely a DI token mismatch. Got: {:?}",
            result.err()
        );
    }

    /// `BroadcastService` must be resolvable by `type_name::<BroadcastService>()`.
    #[serial]
    #[tokio_localset_test::localset_test]
    async fn websocket_module_provides_broadcast_service_via_type_name_token() {
        let app = ToniFactory::create(WsTestModule::module_definition(), AxumAdapter::new()).await;
        let result = app.get::<BroadcastService>().await;
        assert!(
            result.is_ok(),
            "BroadcastService not found — likely a DI token mismatch. Got: {:?}",
            result.err()
        );
    }

    /// Both `ConnectionManager` and `BroadcastService` are resolvable in the same app,
    /// and the `BroadcastService` instance correctly holds a reference to the same
    /// `ConnectionManager` (same Arc pointer).
    #[serial]
    #[tokio_localset_test::localset_test]
    async fn broadcast_service_shares_connection_manager_instance() {
        let app = ToniFactory::create(WsTestModule::module_definition(), AxumAdapter::new()).await;

        let cm = app
            .get::<Arc<ConnectionManager>>()
            .await
            .expect("ConnectionManager should resolve");
        let bs = app
            .get::<BroadcastService>()
            .await
            .expect("BroadcastService should resolve");

        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let sender = Arc::new(toni_axum::TokioSender::new(tx));
        cm.register(toni::WsClient::new("test-client"), sender, None);

        // Must reach the same ConnectionManager instance the test registered with.
        bs.to_all()
            .send(toni::websocket::WsMessage::text("via DI"))
            .await
            .expect("send should succeed");

        let received = rx.recv().await.expect("client should receive message");
        assert_eq!(received.as_text(), Some("via DI"));
    }
}
