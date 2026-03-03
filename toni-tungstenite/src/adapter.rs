use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use futures_util::{stream::SplitStream, SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};
use toni::async_trait;
use toni::websocket::{ConnectionManager, GatewayWrapper, SendError, Sender, TrySendError, WsError, WsMessage};
use toni::WebSocketAdapter;

/// `Full` is the initial state. After `split()`, the read half becomes `ReadOnly` and the
/// write half is a `TokioSender` registered with `ConnectionManager`.
pub enum TungsteniteWsConnection {
    Full(WebSocketStream<TcpStream>),
    ReadOnly(SplitStream<WebSocketStream<TcpStream>>),
}

// Safety: WebSocketStream<TcpStream> is Send
unsafe impl Send for TungsteniteWsConnection {}

pub struct TokioSender {
    inner: mpsc::Sender<WsMessage>,
}

impl TokioSender {
    fn new(tx: mpsc::Sender<WsMessage>) -> Self {
        Self { inner: tx }
    }
}

#[async_trait]
impl Sender for TokioSender {
    async fn send(&self, message: WsMessage) -> Result<(), SendError> {
        self.inner.send(message).await.map_err(|_| SendError)
    }

    fn try_send(&self, message: WsMessage) -> Result<(), TrySendError> {
        self.inner.try_send(message).map_err(|e| match e {
            mpsc::error::TrySendError::Full(msg) => TrySendError::Full(msg),
            mpsc::error::TrySendError::Closed(_) => TrySendError::Closed,
        })
    }
}

#[derive(Clone)]
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
    type Connection = TungsteniteWsConnection;
    type Sender = TokioSender;

    async fn recv(conn: &mut Self::Connection) -> Option<Result<WsMessage, WsError>> {
        loop {
            let raw = match conn {
                TungsteniteWsConnection::Full(ws) => ws.next().await,
                TungsteniteWsConnection::ReadOnly(stream) => stream.next().await,
            }?;

            match raw {
                Ok(Message::Text(t)) => return Some(Ok(WsMessage::Text(t.to_string()))),
                Ok(Message::Binary(b)) => return Some(Ok(WsMessage::Binary(b.to_vec()))),
                Ok(Message::Close(_)) => return None,
                Ok(Message::Ping(_)) | Ok(Message::Pong(_)) | Ok(Message::Frame(_)) => continue,
                Err(e) => return Some(Err(WsError::Internal(e.to_string()))),
            }
        }
    }

    async fn send(conn: &mut Self::Connection, msg: WsMessage) -> Result<(), WsError> {
        match conn {
            TungsteniteWsConnection::Full(ws) => {
                let m = ws_message_to_tungstenite(msg)?;
                ws.send(m).await.map_err(|e| WsError::Internal(e.to_string()))
            }
            TungsteniteWsConnection::ReadOnly(_) => {
                Err(WsError::Internal("Cannot send on read-only connection".into()))
            }
        }
    }

    fn split(conn: Self::Connection) -> (Self::Connection, Self::Sender) {
        match conn {
            TungsteniteWsConnection::Full(ws) => {
                let (write, read) = ws.split();
                let (tx, mut rx) = mpsc::channel::<WsMessage>(32);

                tokio::spawn(async move {
                    let mut write = write;
                    while let Some(msg) = rx.recv().await {
                        if let Ok(m) = ws_message_to_tungstenite(msg) {
                            if write.send(m).await.is_err() {
                                break;
                            }
                        }
                    }
                });

                (TungsteniteWsConnection::ReadOnly(read), TokioSender::new(tx))
            }
            TungsteniteWsConnection::ReadOnly(_) => {
                panic!("Cannot split an already-split connection")
            }
        }
    }

    fn bind_gateway(&mut self, path: &str, gateway: Arc<GatewayWrapper>) -> Result<()> {
        self.gateways.insert(path.to_string(), gateway);
        Ok(())
    }

    fn bind_gateway_with_broadcast(
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
                let conn = TungsteniteWsConnection::Full(ws_stream);

                if let Some((gateway, cm)) = broadcast_gateways.values().next() {
                    TungsteniteAdapter::handle_connection_with_broadcast(
                        conn,
                        gateway,
                        HashMap::new(),
                        cm,
                    )
                    .await;
                } else if let Some(gateway) = gateways.values().next() {
                    TungsteniteAdapter::handle_connection(conn, gateway, HashMap::new()).await;
                }
            });
        }
    }
}

fn ws_message_to_tungstenite(msg: WsMessage) -> Result<Message, WsError> {
    match msg {
        WsMessage::Text(t) => Ok(Message::Text(t.into())),
        WsMessage::Binary(b) => Ok(Message::Binary(b.into())),
        WsMessage::Ping(d) => Ok(Message::Ping(d.into())),
        WsMessage::Pong(d) => Ok(Message::Pong(d.into())),
        WsMessage::Close => Ok(Message::Close(None)),
    }
}
