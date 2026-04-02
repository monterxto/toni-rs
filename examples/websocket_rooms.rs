//! WebSocket rooms and broadcasting example
//!
//! Demonstrates Socket.io-style broadcasting patterns:
//! - Broadcasting to rooms
//! - Private messaging
//! - Broadcasting except sender
//! - Multi-room broadcasting
//! - Room management (join/leave)
//!
//! Run with: cargo run --example websocket_rooms
//! Connect with: websocat ws://localhost:3000/chat

use serde::{Deserialize, Serialize};
use toni::websocket::{BroadcastModule, BroadcastService, WsClient, WsError, WsMessage};
use toni::*;
use toni_macros::{module, websocket_gateway};

// Message types

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
enum ClientMessage {
    #[serde(rename = "join")]
    Join { room: String, username: String },

    #[serde(rename = "leave")]
    Leave { room: String },

    #[serde(rename = "message")]
    Message { room: String, text: String },

    #[serde(rename = "dm")]
    DirectMessage { to: String, text: String },

    #[serde(rename = "typing")]
    Typing { room: String },
}

#[derive(Debug, Serialize)]
struct ServerMessage<T> {
    event: String,
    data: T,
}

#[derive(Debug, Serialize)]
struct UserJoined {
    user_id: String,
    username: String,
}

#[derive(Debug, Serialize)]
struct UserLeft {
    user_id: String,
}

#[derive(Debug, Serialize)]
struct ChatMessage {
    user_id: String,
    text: String,
}

#[derive(Debug, Serialize)]
struct TypingIndicator {
    user_id: String,
}

#[websocket_gateway("/chat", pub struct ChatGateway {
    broadcast: BroadcastService,
})]
impl ChatGateway {
    pub fn new(broadcast: BroadcastService) -> Self {
        Self { broadcast }
    }

    fn json_message<T: Serialize>(event: &str, data: T) -> WsMessage {
        let msg = ServerMessage {
            event: event.to_string(),
            data,
        };
        WsMessage::text(&serde_json::to_string(&msg).unwrap())
    }

    #[on_connect]
    async fn handle_connect(&self, client: &WsClient) -> Result<(), WsError> {
        println!("✅ Client {} connected", client.id);

        // Socket.io: server.to(clientId).emit('welcome', ...)
        let welcome = Self::json_message(
            "welcome",
            serde_json::json!({
                "message": "Welcome to the chat!",
                "your_id": client.id
            }),
        );

        self.broadcast
            .to_client(&client.id)
            .send(welcome)
            .await
            .ok();

        Ok(())
    }

    #[on_disconnect]
    async fn handle_disconnect(&self, client: &WsClient) {
        println!("❌ Client {} disconnected", client.id);

        let rooms = self.broadcast.get_client_rooms(&client.id);

        for room in rooms {
            let msg = Self::json_message(
                "user_left",
                UserLeft {
                    user_id: client.id.clone(),
                },
            );

            self.broadcast.to_room(&room).send(msg).await.ok();
        }
    }

    #[subscribe_message("join")]
    async fn handle_join(
        &self,
        client: WsClient,
        message: WsMessage,
    ) -> Result<Option<WsMessage>, WsError> {
        let text = message
            .as_text()
            .ok_or_else(|| WsError::InvalidMessage("Expected text message".into()))?;

        let data: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| WsError::InvalidMessage(format!("Invalid JSON: {}", e)))?;

        let room = data["data"]["room"]
            .as_str()
            .ok_or_else(|| WsError::InvalidMessage("Missing room field".into()))?;
        let username = data["data"]["username"]
            .as_str()
            .ok_or_else(|| WsError::InvalidMessage("Missing username field".into()))?;

        println!("👤 {} joining room: {}", client.id, room);

        self.broadcast.join_room(&client.id, room)?;

        // Broadcast includes the sender (unlike typing indicator)
        let msg = Self::json_message(
            "user_joined",
            UserJoined {
                user_id: client.id.clone(),
                username: username.to_string(),
            },
        );

        let sent = self.broadcast.to_room(room).send(msg).await?;
        println!("📢 Notified {} clients in room '{}'", sent, room);

        Ok(None)
    }

    #[subscribe_message("leave")]
    async fn handle_leave(
        &self,
        client: WsClient,
        message: WsMessage,
    ) -> Result<Option<WsMessage>, WsError> {
        let text = message
            .as_text()
            .ok_or_else(|| WsError::InvalidMessage("Expected text message".into()))?;

        let data: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| WsError::InvalidMessage(format!("Invalid JSON: {}", e)))?;

        let room = data["data"]["room"]
            .as_str()
            .ok_or_else(|| WsError::InvalidMessage("Missing room field".into()))?;

        println!("👋 {} leaving room: {}", client.id, room);

        self.broadcast.leave_room(&client.id, room)?;

        let msg = Self::json_message(
            "user_left",
            UserLeft {
                user_id: client.id.clone(),
            },
        );

        self.broadcast.to_room(room).send(msg).await?;

        Ok(None)
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

        let data: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| WsError::InvalidMessage(format!("Invalid JSON: {}", e)))?;

        let room = data["data"]["room"]
            .as_str()
            .ok_or_else(|| WsError::InvalidMessage("Missing room field".into()))?;
        let msg_text = data["data"]["text"]
            .as_str()
            .ok_or_else(|| WsError::InvalidMessage("Missing text field".into()))?;

        println!("💬 {} says in {}: {}", client.id, room, msg_text);

        let msg = Self::json_message(
            "message",
            ChatMessage {
                user_id: client.id.clone(),
                text: msg_text.to_string(),
            },
        );

        let sent = self.broadcast.to_room(room).send(msg).await?;
        println!("📨 Delivered to {} clients", sent);

        Ok(None)
    }

    #[subscribe_message("dm")]
    async fn handle_dm(
        &self,
        client: WsClient,
        message: WsMessage,
    ) -> Result<Option<WsMessage>, WsError> {
        let text = message
            .as_text()
            .ok_or_else(|| WsError::InvalidMessage("Expected text message".into()))?;

        let data: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| WsError::InvalidMessage(format!("Invalid JSON: {}", e)))?;

        let to = data["data"]["to"]
            .as_str()
            .ok_or_else(|| WsError::InvalidMessage("Missing to field".into()))?;
        let msg_text = data["data"]["text"]
            .as_str()
            .ok_or_else(|| WsError::InvalidMessage("Missing text field".into()))?;

        println!("📧 {} → {}: {}", client.id, to, msg_text);

        // Private message using auto-room
        let msg = Self::json_message(
            "dm",
            serde_json::json!({
                "from": client.id,
                "text": msg_text
            }),
        );

        self.broadcast.to_client(to).send(msg).await?;

        Ok(None)
    }

    #[subscribe_message("typing")]
    async fn handle_typing(
        &self,
        client: WsClient,
        message: WsMessage,
    ) -> Result<Option<WsMessage>, WsError> {
        let text = message
            .as_text()
            .ok_or_else(|| WsError::InvalidMessage("Expected text message".into()))?;

        let data: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| WsError::InvalidMessage(format!("Invalid JSON: {}", e)))?;

        let room = data["data"]["room"]
            .as_str()
            .ok_or_else(|| WsError::InvalidMessage("Missing room field".into()))?;

        println!("⌨️  {} is typing in {}", client.id, room);

        let msg = Self::json_message(
            "typing",
            TypingIndicator {
                user_id: client.id.clone(),
            },
        );

        // Manual broadcast to exclude sender (no need to notify them they're typing)
        let room_clients = self.broadcast.get_room_clients(room);
        for client_id in room_clients {
            if client_id != client.id {
                self.broadcast
                    .to_client(&client_id)
                    .send(msg.clone())
                    .await
                    .ok();
            }
        }

        Ok(None)
    }
}

// Module and bootstrap

#[module(providers: [ChatGateway], imports:[BroadcastModule::new()])]
struct ChatModule;

#[tokio::main]
async fn main() {
    println!("🚀 Starting WebSocket chat server with rooms...\n");

    let mut app = ToniFactory::new().create_with(ChatModule).await;

    app.use_http_adapter(toni_axum::AxumAdapter::new(), 3000, "127.0.0.1")
        .unwrap();

    // Adapter auto-discovers and registers all gateways from the container
    println!("📡 WebSocket server running on ws://localhost:3000/chat");
    println!("\nTry these commands with websocat:");
    println!("  websocat ws://localhost:3000/chat");
    println!("\nExample messages:");
    println!(r#"  {{"event": "join", "data": {{"room": "lobby", "username": "Alice"}}}}"#);
    println!(r#"  {{"event": "message", "data": {{"room": "lobby", "text": "Hello!"}}}}"#);
    println!(r#"  {{"event": "typing", "data": {{"room": "lobby"}}}}"#);
    println!(r#"  {{"event": "dm", "data": {{"to": "CLIENT_ID", "text": "Secret!"}}}}"#);
    println!(r#"  {{"event": "leave", "data": {{"room": "lobby"}}}}"#);

    app.start().await;
}
