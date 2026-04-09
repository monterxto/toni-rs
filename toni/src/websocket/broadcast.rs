//! WebSocket broadcasting infrastructure
//!
//! Provides Socket.io-style broadcasting capabilities with rooms and targeted messaging.
//! This module is runtime-agnostic through the `WsSink` trait abstraction.

use async_trait::async_trait;
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::{WsClient, WsClientMap, WsMessage};

pub type ClientId = String;
pub type RoomId = String;

// ============================================================================
// Core Traits
// ============================================================================

/// The write half of a WebSocket connection.
///
/// Adapters implement this over their concrete channel type (e.g. a tokio mpsc sender).
/// The framework stores `Arc<dyn WsSink>` per client; adapters never see framework internals.
#[async_trait]
pub trait WsSink: Send + Sync + 'static {
    /// Send a message, waiting for buffer space if needed.
    async fn send(&self, message: WsMessage) -> Result<(), SendError>;

    /// Send without blocking; returns an error if the buffer is full.
    fn try_send(&self, message: WsMessage) -> Result<(), TrySendError>;
}

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, Clone)]
pub struct SendError;

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Failed to send message")
    }
}

impl std::error::Error for SendError {}

#[derive(Debug)]
pub enum TrySendError {
    Full(WsMessage),
    Closed,
}

impl std::fmt::Display for TrySendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrySendError::Full(_) => write!(f, "Channel buffer is full"),
            TrySendError::Closed => write!(f, "Channel is closed"),
        }
    }
}

impl std::error::Error for TrySendError {}

#[derive(Debug)]
pub enum BroadcastError {
    ClientNotFound,
    SendFailed(SendError),
}

impl std::fmt::Display for BroadcastError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BroadcastError::ClientNotFound => write!(f, "Client not found"),
            BroadcastError::SendFailed(e) => write!(f, "Send failed: {}", e),
        }
    }
}

impl std::error::Error for BroadcastError {}

impl From<SendError> for BroadcastError {
    fn from(e: SendError) -> Self {
        BroadcastError::SendFailed(e)
    }
}

// ============================================================================
// Connection Manager (State Management)
// ============================================================================

/// Tracks room membership and namespace assignment for connected WebSocket clients.
///
/// Does not store write channels — those live exclusively in [`WsClientMap`].
/// All message delivery is delegated there, keeping topology state separate from transport.
pub struct ConnectionManager {
    clients: Arc<RwLock<HashMap<ClientId, ClientState>>>,
    rooms: Arc<RwLock<HashMap<RoomId, HashSet<ClientId>>>>,
    namespaces: Arc<RwLock<HashMap<String, HashSet<ClientId>>>>,
    ws_client_map: Arc<WsClientMap>,
}

pub struct ClientState {
    pub client: WsClient,
    pub rooms: HashSet<RoomId>,
    pub namespace: Option<String>,
}

impl ConnectionManager {
    pub(crate) fn new(ws_client_map: Arc<WsClientMap>) -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
            rooms: Arc::new(RwLock::new(HashMap::new())),
            namespaces: Arc::new(RwLock::new(HashMap::new())),
            ws_client_map,
        }
    }

    /// Register a client's topology (rooms, namespace). The write channel must
    /// already be in `WsClientMap` — `ConnectionManager` never holds senders.
    pub(crate) fn register(&self, client: WsClient, namespace: Option<String>) {
        let client_id = client.id.clone();

        let state = ClientState {
            client,
            rooms: HashSet::new(),
            namespace: namespace.clone(),
        };

        self.clients.write().insert(client_id.clone(), state);

        if let Some(ns) = namespace {
            self.namespaces
                .write()
                .entry(ns)
                .or_insert_with(HashSet::new)
                .insert(client_id.clone());
        }

        let _ = self.join_room(&client_id, &client_id);
    }

    /// Removes client from topology. The write channel is removed from `WsClientMap`
    /// separately via the `on_disconnect` callback.
    pub(crate) fn unregister(&self, client_id: &str) -> Option<ClientState> {
        let state = self.clients.write().remove(client_id)?;

        let mut rooms = self.rooms.write();
        for room_members in rooms.values_mut() {
            room_members.remove(client_id);
        }

        if let Some(ref ns) = state.namespace {
            if let Some(ns_members) = self.namespaces.write().get_mut(ns) {
                ns_members.remove(client_id);
            }
        }

        Some(state)
    }

    pub fn join_room(&self, client_id: &str, room_id: &str) -> Result<(), BroadcastError> {
        self.rooms
            .write()
            .entry(room_id.to_string())
            .or_insert_with(HashSet::new)
            .insert(client_id.to_string());

        let mut clients = self.clients.write();
        if let Some(state) = clients.get_mut(client_id) {
            state.rooms.insert(room_id.to_string());
            Ok(())
        } else {
            Err(BroadcastError::ClientNotFound)
        }
    }

    pub fn leave_room(&self, client_id: &str, room_id: &str) -> Result<(), BroadcastError> {
        if let Some(room_members) = self.rooms.write().get_mut(room_id) {
            room_members.remove(client_id);
        }

        let mut clients = self.clients.write();
        if let Some(state) = clients.get_mut(client_id) {
            state.rooms.remove(room_id);
            Ok(())
        } else {
            Err(BroadcastError::ClientNotFound)
        }
    }

    pub fn get_room_clients(&self, room_id: &str) -> Vec<ClientId> {
        self.rooms
            .read()
            .get(room_id)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn get_namespace_clients(&self, namespace: &str) -> Vec<ClientId> {
        self.namespaces
            .read()
            .get(namespace)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn get_all_clients(&self) -> Vec<ClientId> {
        self.clients.read().keys().cloned().collect()
    }

    pub fn get_client_rooms(&self, client_id: &str) -> Vec<RoomId> {
        self.clients
            .read()
            .get(client_id)
            .map(|state| state.rooms.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Send a message to the given clients. Returns how many write channels accepted it.
    pub async fn send_to_clients(
        &self,
        client_ids: &[ClientId],
        message: WsMessage,
    ) -> Result<usize, BroadcastError> {
        let mut sent_count = 0;
        for client_id in client_ids {
            if self.clients.read().contains_key(client_id) {
                if let Some(sink) = self.ws_client_map.get_sink(client_id) {
                    if sink.try_send(message.clone()).is_ok() {
                        sent_count += 1;
                    }
                }
            }
        }
        Ok(sent_count)
    }

    /// Sends close frames to all connected clients and clears topology state.
    pub async fn close_all(&self) {
        let client_ids: Vec<ClientId> = self.clients.read().keys().cloned().collect();
        let count = client_ids.len();

        tracing::info!(count, "Closing WebSocket connections");

        for client_id in &client_ids {
            if let Some(sink) = self.ws_client_map.get_sink(client_id) {
                let _ = sink.try_send(WsMessage::Close);
                tracing::debug!(client_id = %client_id, "Sent close frame");
            }
        }

        self.clients.write().clear();
        self.rooms.write().clear();
        self.namespaces.write().clear();

        tracing::debug!(count, "Closed connections");
    }
}

// ============================================================================
// Broadcast Service (High-Level API)
// ============================================================================

/// High-level broadcasting service with Socket.io-style API
///
/// Provides fluent API for targeting specific clients, rooms, or all clients.
/// This service can be injected into gateways via DI.
///
/// # Examples
///
/// ```rust,ignore
/// // Broadcast to room
/// broadcast.to_room("lobby").send(msg).await?;
///
/// // Private message (using auto-room)
/// broadcast.to_client(&user_id).send(msg).await?;
///
/// // Broadcast to all except sender
/// broadcast.except(&client_id).send(msg).await?;
///
/// // Multi-room broadcast
/// broadcast.to_room("r1").and_room("r2").send(msg).await?;
/// ```
pub struct BroadcastService {
    manager: Arc<ConnectionManager>,
}

impl BroadcastService {
    pub fn new() -> Self {
        Self {
            manager: Arc::new(ConnectionManager::new(Arc::new(WsClientMap::new()))),
        }
    }

    /// Expose the internal `ConnectionManager` so the framework can wire
    /// `on_connect` / `on_disconnect` callbacks into it.
    pub(crate) fn connection_manager(&self) -> Arc<ConnectionManager> {
        self.manager.clone()
    }

    /// The shared client map that holds write channels for all connected clients.
    ///
    /// The framework extracts this in `listen()` so `make_ws_callbacks` can register
    /// sinks into the same map that `BroadcastService` delegates sends through.
    pub(crate) fn ws_client_map(&self) -> Arc<WsClientMap> {
        self.manager.ws_client_map.clone()
    }

    // ========================================================================
    // Socket.io API patterns
    // ========================================================================

    /// Socket.io equivalent: `server.emit('event', data)`
    pub fn to_all(&self) -> BroadcastTarget {
        BroadcastTarget::new(self.manager.clone(), TargetType::All)
    }

    /// Socket.io equivalent: `server.to('room1').emit('event', data)`
    pub fn to_room(&self, room: impl Into<String>) -> BroadcastTarget {
        BroadcastTarget::new(self.manager.clone(), TargetType::Room(room.into()))
    }

    /// Uses auto-room pattern (each client has a room named after their ID)
    ///
    /// Socket.io equivalent: `server.to(clientId).emit('event', data)`
    pub fn to_client(&self, client_id: impl Into<String>) -> BroadcastTarget {
        BroadcastTarget::new(self.manager.clone(), TargetType::Client(client_id.into()))
    }

    /// Socket.io equivalent: `server.except(clientId).emit('event', data)`
    /// or `client.broadcast.emit('event', data)`
    pub fn except(&self, client_id: impl Into<String>) -> BroadcastTarget {
        BroadcastTarget::new(self.manager.clone(), TargetType::Except(client_id.into()))
    }

    // ========================================================================
    // Room management
    // ========================================================================

    pub fn join_room(&self, client_id: &str, room_id: &str) -> Result<(), BroadcastError> {
        self.manager.join_room(client_id, room_id)
    }

    pub fn leave_room(&self, client_id: &str, room_id: &str) -> Result<(), BroadcastError> {
        self.manager.leave_room(client_id, room_id)
    }

    pub fn get_client_rooms(&self, client_id: &str) -> Vec<RoomId> {
        self.manager.get_client_rooms(client_id)
    }

    pub fn get_room_clients(&self, room_id: &str) -> Vec<ClientId> {
        self.manager.get_room_clients(room_id)
    }

    // ========================================================================
    // Connection lifecycle
    // ========================================================================

    /// Register a client's write channel and topology state.
    ///
    /// The framework calls this from `on_connect`. Test code can call it directly
    /// to set up clients without a real TCP connection.
    pub fn connect(&self, client_id: ClientId, sink: Arc<dyn WsSink>, namespace: Option<String>) {
        self.manager.ws_client_map.register(client_id.clone(), sink);
        self.manager.register(WsClient::new(&client_id), namespace);
    }

    /// Remove a client's write channel and topology state.
    pub fn disconnect(&self, client_id: &str) {
        self.manager.ws_client_map.unregister(client_id);
        self.manager.unregister(client_id);
    }

    /// Send close frames to all clients and clear topology. Called by the framework on shutdown.
    pub(crate) async fn close_all(&self) {
        self.manager.close_all().await;
    }
}

impl Clone for BroadcastService {
    fn clone(&self) -> Self {
        Self {
            manager: self.manager.clone(),
        }
    }
}

// ============================================================================
// Broadcast Target (Fluent Builder)
// ============================================================================

/// Fluent API builder for broadcast targets
///
/// Allows chaining multiple rooms and namespace filtering before sending
pub struct BroadcastTarget {
    manager: Arc<ConnectionManager>,
    target_type: TargetType,
    namespace: Option<String>,
}

#[derive(Clone)]
enum TargetType {
    All,
    Room(String),
    Client(String),
    Except(String),
    Multiple(Vec<String>),
}

impl BroadcastTarget {
    fn new(manager: Arc<ConnectionManager>, target_type: TargetType) -> Self {
        Self {
            manager,
            target_type,
            namespace: None,
        }
    }

    /// Only clients in this namespace will receive the message
    pub fn in_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    /// Socket.io equivalent: `server.to('room1').to('room2').emit(...)`
    pub fn and_room(mut self, room: impl Into<String>) -> Self {
        match self.target_type {
            TargetType::Room(r) => {
                self.target_type = TargetType::Multiple(vec![r, room.into()]);
            }
            TargetType::Multiple(ref mut rooms) => {
                rooms.push(room.into());
            }
            _ => {}
        }
        self
    }

    /// Returns count of clients that successfully received the message
    pub async fn send(&self, message: WsMessage) -> Result<usize, BroadcastError> {
        let targets = self.resolve_targets();
        self.manager.send_to_clients(&targets, message).await
    }

    /// Wraps in JSON format: `{"event": "...", "data": ...}`
    pub async fn send_event(
        &self,
        event: impl Into<String>,
        data: impl Into<String>,
    ) -> Result<usize, BroadcastError> {
        let message_json = serde_json::json!({
            "event": event.into(),
            "data": data.into()
        })
        .to_string();

        self.send(WsMessage::Text(message_json)).await
    }

    fn resolve_targets(&self) -> Vec<String> {
        let mut targets = match &self.target_type {
            TargetType::All => self.manager.get_all_clients(),
            TargetType::Room(room) => self.manager.get_room_clients(room),
            TargetType::Client(id) => vec![id.clone()],
            TargetType::Except(exclude_id) => {
                let mut all = self.manager.get_all_clients();
                all.retain(|id| id != exclude_id);
                all
            }
            TargetType::Multiple(rooms) => {
                let mut combined = HashSet::new();
                for room in rooms {
                    combined.extend(self.manager.get_room_clients(room));
                }
                combined.into_iter().collect()
            }
        };

        if let Some(ref ns) = self.namespace {
            let ns_clients: HashSet<_> =
                self.manager.get_namespace_clients(ns).into_iter().collect();
            targets.retain(|id| ns_clients.contains(id));
        }

        targets
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct MockSender {
        sent: Arc<RwLock<Vec<WsMessage>>>,
    }

    impl MockSender {
        fn new() -> Self {
            Self {
                sent: Arc::new(RwLock::new(Vec::new())),
            }
        }

        fn get_sent(&self) -> Vec<WsMessage> {
            self.sent.read().clone()
        }
    }

    #[async_trait]
    impl WsSink for MockSender {
        async fn send(&self, message: WsMessage) -> Result<(), SendError> {
            self.sent.write().push(message);
            Ok(())
        }

        fn try_send(&self, message: WsMessage) -> Result<(), TrySendError> {
            self.sent.write().push(message);
            Ok(())
        }
    }

    fn register_client(bs: &BroadcastService, id: &str, namespace: Option<String>) -> MockSender {
        let mock = MockSender::new();
        bs.connect(
            id.to_string(),
            Arc::new(mock.clone()) as Arc<dyn WsSink>,
            namespace,
        );
        mock
    }

    #[test]
    fn test_register_and_unregister() {
        let bs = BroadcastService::new();
        let cm = bs.connection_manager();
        register_client(&bs, "client1", None);
        assert_eq!(cm.get_all_clients().len(), 1);
        bs.disconnect("client1");
        assert_eq!(cm.get_all_clients().len(), 0);
    }

    #[test]
    fn test_join_and_leave_room() {
        let bs = BroadcastService::new();
        let cm = bs.connection_manager();
        register_client(&bs, "client1", None);
        cm.join_room("client1", "lobby").unwrap();

        let room_clients = cm.get_room_clients("lobby");
        assert_eq!(room_clients.len(), 1);
        assert_eq!(room_clients[0], "client1");

        cm.leave_room("client1", "lobby").unwrap();
        assert_eq!(cm.get_room_clients("lobby").len(), 0);
    }

    #[test]
    fn test_auto_join_client_id_room() {
        let bs = BroadcastService::new();
        let cm = bs.connection_manager();
        register_client(&bs, "client1", None);

        let room_clients = cm.get_room_clients("client1");
        assert_eq!(room_clients.len(), 1);
        assert_eq!(room_clients[0], "client1");
    }

    #[tokio::test]
    async fn test_broadcast_to_room() {
        let bs = BroadcastService::new();
        let s1 = register_client(&bs, "c1", None);
        let s2 = register_client(&bs, "c2", None);
        let s3 = register_client(&bs, "c3", None);

        bs.join_room("c1", "lobby").unwrap();
        bs.join_room("c2", "lobby").unwrap();

        let sent = bs
            .to_room("lobby")
            .send(WsMessage::text("hello"))
            .await
            .unwrap();
        assert_eq!(sent, 2);
        assert_eq!(s1.get_sent().len(), 1);
        assert_eq!(s2.get_sent().len(), 1);
        assert_eq!(s3.get_sent().len(), 0);
    }

    #[tokio::test]
    async fn test_broadcast_except() {
        let bs = BroadcastService::new();
        let s1 = register_client(&bs, "c1", None);
        let s2 = register_client(&bs, "c2", None);

        bs.except("c1")
            .send(WsMessage::text("hello"))
            .await
            .unwrap();
        assert_eq!(s1.get_sent().len(), 0);
        assert_eq!(s2.get_sent().len(), 1);
    }

    #[tokio::test]
    async fn test_private_message_to_client() {
        let bs = BroadcastService::new();
        let s1 = register_client(&bs, "c1", None);
        let s2 = register_client(&bs, "c2", None);

        bs.to_client("c1")
            .send(WsMessage::text("private"))
            .await
            .unwrap();
        assert_eq!(s1.get_sent().len(), 1);
        assert_eq!(s2.get_sent().len(), 0);
    }

    #[tokio::test]
    async fn test_multi_room_broadcast() {
        let bs = BroadcastService::new();
        let s1 = register_client(&bs, "c1", None);
        let s2 = register_client(&bs, "c2", None);
        let s3 = register_client(&bs, "c3", None);

        bs.join_room("c1", "room1").unwrap();
        bs.join_room("c2", "room2").unwrap();

        let sent = bs
            .to_room("room1")
            .and_room("room2")
            .send(WsMessage::text("multi"))
            .await
            .unwrap();
        assert_eq!(sent, 2);
        assert_eq!(s1.get_sent().len(), 1);
        assert_eq!(s2.get_sent().len(), 1);
        assert_eq!(s3.get_sent().len(), 0);
    }
}
