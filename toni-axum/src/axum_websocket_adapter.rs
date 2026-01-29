use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    http::HeaderMap,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::RwLock;

use toni::{
    DisconnectReason, GatewayWrapper, WebSocketAdapter, WsClient, WsError, WsHandshake, WsMessage,
};

/// Axum WebSocket adapter with support for both same-port and separate-port deployment
#[derive(Clone)]
pub struct AxumWebSocketAdapter {
    gateways: Arc<RwLock<HashMap<String, Arc<GatewayWrapper>>>>,
    /// Optional: standalone router for separate port
    router: Option<Router>,
}

impl AxumWebSocketAdapter {
    pub fn new() -> Self {
        Self {
            gateways: Arc::new(RwLock::new(HashMap::new())),
            router: Some(Router::new()),
        }
    }

    /// Get router with all registered WebSocket routes (for same-port deployment)
    ///
    /// This consumes the adapter and returns the router to be merged with HTTP routes
    pub fn into_router(mut self) -> Router {
        self.router.take().unwrap_or_else(|| Router::new())
    }
}

impl Default for AxumWebSocketAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl WebSocketAdapter for AxumWebSocketAdapter {
    fn add_gateway(&mut self, path: &str, gateway: Arc<GatewayWrapper>) {
        let path_clone = path.to_string();

        // Add to router if we have one
        if let Some(ref mut router) = self.router {
            let gateway_for_route = gateway.clone();
            *router = router.clone().route(
                path,
                get(|headers: HeaderMap, ws: WebSocketUpgrade| async move {
                    ws.on_upgrade(move |socket| handle_socket(socket, gateway_for_route, headers))
                }),
            );
        }

        // Store gateway for retrieval
        let gateways_clone = self.gateways.clone();
        tokio::spawn(async move {
            gateways_clone.write().await.insert(path_clone, gateway);
        });
    }

    async fn listen(self, port: u16, hostname: &str) -> Result<()> {
        let router = self
            .router
            .ok_or_else(|| anyhow::anyhow!("Router already consumed"))?;
        let addr = format!("{}:{}", hostname, port);
        let listener = TcpListener::bind(&addr).await?;

        println!("WebSocket server listening on {}", addr);
        axum::serve(listener, router).await?;
        Ok(())
    }
}

/// Handle WebSocket connection
async fn handle_socket(socket: WebSocket, gateway: Arc<GatewayWrapper>, headers: HeaderMap) {
    let (mut sender, mut receiver) = socket.split();

    // Generate client ID
    let client_id = uuid::Uuid::new_v4().to_string();

    // Extract headers from HTTP handshake
    let mut handshake_headers = HashMap::new();
    for (name, value) in headers.iter() {
        if let Ok(value_str) = value.to_str() {
            handshake_headers.insert(name.as_str().to_lowercase(), value_str.to_string());
        }
    }

    // Create WsClient from socket
    let client = WsClient {
        id: client_id.clone(),
        handshake: WsHandshake {
            query: HashMap::new(),
            headers: handshake_headers,
            remote_addr: None,
        },
        extensions: Default::default(),
    };

    // Handle connection
    if let Err(e) = gateway.handle_connect(client).await {
        eprintln!("Connection rejected: {}", e);
        let _ = sender.close().await;
        return;
    }

    println!("Client {} connected to {}", client_id, gateway.get_path());

    // Message loop
    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(axum_msg) => {
                // Convert Axum message to WsMessage
                let ws_msg = match axum_to_ws_message(axum_msg) {
                    Ok(msg) => msg,
                    Err(e) => {
                        eprintln!("Error converting message: {}", e);
                        continue;
                    }
                };

                // Handle message
                match gateway.handle_message(client_id.clone(), ws_msg).await {
                    Ok(Some(response)) => {
                        // Send response if any
                        if let Ok(axum_response) = ws_message_to_axum(response) {
                            if sender.send(axum_response).await.is_err() {
                                break;
                            }
                        }
                    }
                    Ok(None) => {
                        // No response needed
                    }
                    Err(e) => {
                        eprintln!("Error handling message: {}", e);

                        // Send error message to client
                        let error_msg = Message::Text(
                            serde_json::json!({
                                "error": e.to_string()
                            })
                            .to_string()
                            .into(),
                        );
                        if sender.send(error_msg).await.is_err() {
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("WebSocket error: {}", e);
                break;
            }
        }
    }

    // Handle disconnection
    println!(
        "Client {} disconnected from {}",
        client_id,
        gateway.get_path()
    );
    gateway
        .handle_disconnect(client_id, DisconnectReason::ClientDisconnect)
        .await;
}

/// Convert Axum WebSocket message to WsMessage
fn axum_to_ws_message(msg: Message) -> Result<WsMessage, WsError> {
    match msg {
        Message::Text(text) => Ok(WsMessage::Text(text.to_string())),
        Message::Binary(data) => Ok(WsMessage::Binary(data.to_vec())),
        Message::Ping(data) => Ok(WsMessage::Ping(data.to_vec())),
        Message::Pong(data) => Ok(WsMessage::Pong(data.to_vec())),
        Message::Close(_) => Err(WsError::ConnectionClosed("Close frame received".into())),
    }
}

/// Convert WsMessage to Axum WebSocket message
fn ws_message_to_axum(msg: WsMessage) -> Result<Message, WsError> {
    match msg {
        WsMessage::Text(text) => Ok(Message::Text(text.into())),
        WsMessage::Binary(data) => Ok(Message::Binary(data.into())),
        WsMessage::Ping(data) => Ok(Message::Ping(data.into())),
        WsMessage::Pong(data) => Ok(Message::Pong(data.into())),
    }
}
