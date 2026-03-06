use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;
use toni::async_trait;
use toni::websocket::{SendError, Sender, TrySendError, WsMessage};
use toni::{WebSocketAdapter, WsConnectionCallbacks};

// ── TokioSender ───────────────────────────────────────────────────────────────

struct TokioSender {
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

// ── TungsteniteAdapter ────────────────────────────────────────────────────────

struct PortEntry {
    // path → callbacks; raw TCP has no path info, so we use the first registered binding
    bindings: HashMap<String, Arc<WsConnectionCallbacks>>,
}

impl PortEntry {
    fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }
}

pub struct TungsteniteAdapter {
    ports: HashMap<u16, PortEntry>,
    shutdown_tx: Arc<watch::Sender<bool>>,
}

impl TungsteniteAdapter {
    pub fn new() -> Self {
        let (tx, _) = watch::channel(false);
        Self {
            ports: HashMap::new(),
            shutdown_tx: Arc::new(tx),
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
    fn create(&mut self, port: u16) -> Result<()> {
        self.ports.entry(port).or_insert_with(PortEntry::new);
        Ok(())
    }

    fn bind(&mut self, port: u16, path: &str, callbacks: Arc<WsConnectionCallbacks>) -> Result<()> {
        self.ports
            .entry(port)
            .or_insert_with(PortEntry::new)
            .bindings
            .insert(path.to_string(), callbacks);
        Ok(())
    }

    async fn listen(&mut self, hostname: &str) -> Result<()> {
        for (port, entry) in &self.ports {
            let addr = format!("{}:{}", hostname, port);
            let listener = TcpListener::bind(&addr).await?;

            // Raw TCP has no path info, so grab the first registered callbacks.
            let bindings: Vec<Arc<WsConnectionCallbacks>> =
                entry.bindings.values().cloned().collect();
            let mut shutdown_rx = self.shutdown_tx.subscribe();

            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        result = listener.accept() => {
                            let (stream, _) = match result {
                                Ok(r) => r,
                                Err(e) => { eprintln!("Accept error: {}", e); continue; }
                            };

                            if let Some(callbacks) = bindings.first().cloned() {
                                tokio::spawn(async move {
                                    let ws_stream = match tokio_tungstenite::accept_async(stream).await {
                                        Ok(ws) => ws,
                                        Err(e) => {
                                            eprintln!("WS handshake error: {}", e);
                                            return;
                                        }
                                    };
                                    run_ws_connection(ws_stream, callbacks).await;
                                });
                            }
                        }
                        _ = shutdown_rx.changed() => {
                            if *shutdown_rx.borrow() { break; }
                        }
                    }
                }
            });
        }

        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        let _ = self.shutdown_tx.send(true);
        Ok(())
    }
}

// ── Shared connection loop ────────────────────────────────────────────────────

async fn run_ws_connection(
    ws_stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    callbacks: Arc<WsConnectionCallbacks>,
) {
    let (write, read) = ws_stream.split();
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

    let sender: Arc<dyn Sender> = Arc::new(TokioSender::new(tx));

    let client_id = match callbacks.connect(HashMap::new(), sender).await {
        Ok(id) => id,
        Err(_) => return,
    };

    let mut read = read;
    while let Some(result) = read.next().await {
        match result {
            Ok(Message::Text(t)) => {
                if !callbacks
                    .message(client_id.clone(), WsMessage::Text(t.to_string()))
                    .await
                {
                    break;
                }
            }
            Ok(Message::Binary(b)) => {
                if !callbacks
                    .message(client_id.clone(), WsMessage::Binary(b.to_vec()))
                    .await
                {
                    break;
                }
            }
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) | Ok(Message::Frame(_)) => {}
        }
    }

    callbacks.disconnect(client_id).await;
}

fn ws_message_to_tungstenite(msg: WsMessage) -> Result<Message> {
    match msg {
        WsMessage::Text(t) => Ok(Message::Text(t.into())),
        WsMessage::Binary(b) => Ok(Message::Binary(b.into())),
        WsMessage::Ping(d) => Ok(Message::Ping(d.into())),
        WsMessage::Pong(d) => Ok(Message::Pong(d.into())),
        WsMessage::Close => Ok(Message::Close(None)),
    }
}
