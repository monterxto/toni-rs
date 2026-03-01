use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use toni::async_trait;
use toni::WebSocketAdapter;
use toni::websocket::{ConnectionManager, GatewayWrapper, WsSocket};

use crate::ws_socket::TungsteniteWsSocket;

/// `WebSocketAdapter` backed by tokio-tungstenite for separate-port WebSocket deployment.
///
/// # Usage
///
/// ```rust,ignore
/// app.use_websocket_adapter(TungsteniteAdapter::new()).unwrap();
/// // Any gateway decorated with `port = N` is automatically routed here.
/// ```
pub struct TungsteniteAdapter {
    gateways: HashMap<String, Arc<GatewayWrapper>>,
    broadcast_gateways: HashMap<String, (Arc<GatewayWrapper>, Arc<ConnectionManager>)>,
}

impl TungsteniteAdapter {
    pub fn new() -> Self {
        Self {
            gateways: HashMap::new(),
            broadcast_gateways: HashMap::new(),
        }
    }
}

impl Default for TungsteniteAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl WebSocketAdapter for TungsteniteAdapter {
    fn on_gateway(&mut self, path: &str, gateway: Arc<GatewayWrapper>) -> Result<()> {
        self.gateways.insert(path.to_string(), gateway);
        Ok(())
    }

    fn on_gateway_with_broadcast(
        &mut self,
        path: &str,
        gateway: Arc<GatewayWrapper>,
        connection_manager: Arc<ConnectionManager>,
    ) -> Result<()> {
        self.broadcast_gateways
            .insert(path.to_string(), (gateway, connection_manager));
        Ok(())
    }

    async fn listen(&mut self, port: u16, hostname: &str) -> Result<()> {
        let addr = format!("{}:{}", hostname, port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;

        let gateways: Arc<HashMap<String, Arc<GatewayWrapper>>> =
            Arc::new(self.gateways.clone());
        let broadcast_gateways: Arc<
            HashMap<String, (Arc<GatewayWrapper>, Arc<ConnectionManager>)>,
        > = Arc::new(self.broadcast_gateways.clone());

        loop {
            let (stream, _peer) = listener.accept().await?;
            let gateways = gateways.clone();
            let broadcast_gateways = broadcast_gateways.clone();

            tokio::spawn(async move {
                let ws_stream = match tokio_tungstenite::accept_async(stream).await {
                    Ok(ws) => ws,
                    Err(e) => {
                        eprintln!("WS handshake error: {}", e);
                        return;
                    }
                };

                // Raw TCP doesn't expose the request path; `accept_hdr_async` would be
                // needed for true path routing. One gateway per port is the current contract.
                if let Some((gateway, cm)) = broadcast_gateways.values().next() {
                    let gateway = gateway.clone();
                    let cm = cm.clone();
                    let socket = TungsteniteWsSocket::new(ws_stream);
                    socket
                        .handle_connection_with_broadcast(&gateway, HashMap::new(), &cm)
                        .await;
                } else if let Some(gateway) = gateways.values().next() {
                    let gateway = gateway.clone();
                    let mut socket = TungsteniteWsSocket::new(ws_stream);
                    socket.handle_connection(&gateway, HashMap::new()).await;
                }
            });
        }
    }
}
