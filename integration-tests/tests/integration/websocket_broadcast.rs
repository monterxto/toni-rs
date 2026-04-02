//! WebSocket broadcast integration tests
//!
//! Two layers of coverage:
//!
//! 1. **Broadcast pipeline** — `BroadcastService` + `TokioSender` wired together without
//!    a real TCP connection. Verifies the full message-routing path from `BroadcastService`
//!    API down to the stored sink channel.
//!
//! 2. **DI resolution** — `BroadcastModule` registers `BroadcastService` with the correct
//!    token so the framework resolves it when a gateway declares it as a dependency.

use std::sync::Arc;
use tokio::sync::mpsc;
use toni::websocket::{BroadcastService, WsMessage, WsSink};
use toni_axum::TokioSender;

// Helpers

/// Returns the receiver so tests can assert what the client received.
fn register_client(
    bs: &BroadcastService,
    client_id: &str,
    namespace: Option<String>,
) -> mpsc::Receiver<WsMessage> {
    let (tx, rx) = mpsc::channel(16);
    let sink = Arc::new(TokioSender::new(tx)) as Arc<dyn WsSink>;
    bs.connect(client_id.to_string(), sink, namespace);
    rx
}

// Broadcast pipeline tests (no DI, no TCP)

#[tokio::test]
async fn broadcast_to_all_delivers_to_every_registered_client() {
    let bs = BroadcastService::new();

    let mut rx1 = register_client(&bs, "client-1", None);
    let mut rx2 = register_client(&bs, "client-2", None);
    let mut rx3 = register_client(&bs, "client-3", None);

    let msg = WsMessage::text("hello everyone");
    let sent = bs.to_all().send(msg.clone()).await.unwrap();

    assert_eq!(sent, 3, "should have delivered to 3 clients");
    assert_eq!(rx1.recv().await.unwrap().as_text(), Some("hello everyone"));
    assert_eq!(rx2.recv().await.unwrap().as_text(), Some("hello everyone"));
    assert_eq!(rx3.recv().await.unwrap().as_text(), Some("hello everyone"));
}

#[tokio::test]
async fn broadcast_to_room_delivers_only_to_room_members() {
    let bs = BroadcastService::new();

    let mut lobby_rx1 = register_client(&bs, "alice", None);
    let mut lobby_rx2 = register_client(&bs, "bob", None);
    let mut other_rx = register_client(&bs, "carol", None);

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
    let bs = BroadcastService::new();

    let mut target_rx = register_client(&bs, "target", None);
    let mut bystander_rx = register_client(&bs, "bystander", None);

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
    let bs = BroadcastService::new();

    let mut rx = register_client(&bs, "leaving", None);

    bs.to_all().send(WsMessage::text("before")).await.unwrap();
    assert_eq!(rx.recv().await.unwrap().as_text(), Some("before"));

    bs.disconnect("leaving");

    let sent = bs.to_all().send(WsMessage::text("after")).await.unwrap();
    assert_eq!(sent, 0, "no clients registered after disconnect");
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn leave_room_stops_room_messages() {
    let bs = BroadcastService::new();

    let mut rx = register_client(&bs, "user", None);

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

mod di_tests {
    use serial_test::serial;
    use std::sync::Arc;
    use toni::module;
    use toni::toni_factory::ToniFactory;
    use toni::websocket::{BroadcastModule, BroadcastService, WsMessage, WsSink};
    use toni_axum::TokioSender;

    #[module(imports: [BroadcastModule::new()])]
    struct WsTestModule;

    #[serial]
    #[tokio_localset_test::localset_test]
    async fn broadcast_module_provides_broadcast_service() {
        let app = ToniFactory::create(WsTestModule::module_definition()).await;
        let result = app.get::<BroadcastService>().await;
        assert!(
            result.is_ok(),
            "BroadcastService not found — likely a DI token mismatch. Got: {:?}",
            result.err()
        );
    }

    #[serial]
    #[tokio_localset_test::localset_test]
    async fn broadcast_service_can_send_to_connected_client() {
        let app = ToniFactory::create(WsTestModule::module_definition()).await;
        let bs = app
            .get::<BroadcastService>()
            .await
            .expect("BroadcastService should resolve");

        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let sink = Arc::new(TokioSender::new(tx)) as Arc<dyn WsSink>;
        bs.connect("test-client".to_string(), sink, None);

        bs.to_all()
            .send(WsMessage::text("via DI"))
            .await
            .expect("send should succeed");

        let received = rx.recv().await.expect("client should receive message");
        assert_eq!(received.as_text(), Some("via DI"));
    }
}
