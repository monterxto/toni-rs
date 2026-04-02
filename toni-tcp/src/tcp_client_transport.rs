use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::{oneshot, Mutex, OnceCell};
use toni::{async_trait, RpcClientError, RpcClientTransport, RpcData};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

struct Inner {
    writer: Mutex<tokio::net::tcp::OwnedWriteHalf>,
    // Correlation id → channel waiting for the server's reply.
    pending: Mutex<HashMap<String, oneshot::Sender<Result<RpcData, RpcClientError>>>>,
}

/// TCP transport for [`RpcClient`].
///
/// Maintains a single persistent TCP connection to the remote service.
/// Request-response uses a monotonic correlation id; the background reader
/// loop matches each incoming `{"id":..., "response":...}` or `{"id":...,
/// "err":...}` frame to the waiting caller and delivers it via an in-memory
/// channel.
///
/// Fire-and-forget (`emit`) sends a frame with no `id` field and returns as
/// soon as the write completes.
///
/// # Example
///
/// ```rust,no_run
/// provider_value!(
///     "ORDERS_CLIENT",
///     toni::RpcClient::new(toni_tcp::TcpClientTransport::new("127.0.0.1", 4000))
/// )
/// ```
///
/// [`RpcClient`]: toni::RpcClient
pub struct TcpClientTransport {
    host: String,
    port: u16,
    timeout: Duration,
    inner: OnceCell<Arc<Inner>>,
}

impl TcpClientTransport {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            timeout: Duration::from_secs(5),
            inner: OnceCell::new(),
        }
    }

    /// Override the request-response timeout (default: 5 s).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    async fn get_or_connect(&self) -> Result<Arc<Inner>, RpcClientError> {
        self.inner
            .get_or_try_init(|| async {
                let addr = format!("{}:{}", self.host, self.port);
                let stream = TcpStream::connect(&addr)
                    .await
                    .map_err(|e| RpcClientError::Transport(e.to_string()))?;

                let (reader, writer) = stream.into_split();
                let inner = Arc::new(Inner {
                    writer: Mutex::new(writer),
                    pending: Mutex::new(HashMap::new()),
                });

                tokio::spawn(reader_loop(reader, inner.clone()));

                println!("[TcpClientTransport] Connected to {}", addr);
                Ok(inner)
            })
            .await
            .cloned()
    }
}

async fn reader_loop(reader: tokio::net::tcp::OwnedReadHalf, inner: Arc<Inner>) {
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let msg: serde_json::Value = match serde_json::from_str(trimmed) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let Some(id) = msg["id"].as_str() else {
                    continue;
                };

                let mut pending = inner.pending.lock().await;
                if let Some(tx) = pending.remove(id) {
                    let result = if let Some(err) = msg.get("err") {
                        let message = err["message"]
                            .as_str()
                            .unwrap_or("unknown error")
                            .to_string();
                        let status = err["status"].as_str().unwrap_or("error").to_string();
                        Err(RpcClientError::Remote { message, status })
                    } else {
                        Ok(RpcData::Json(msg["response"].clone()))
                    };
                    let _ = tx.send(result);
                }
            }
            Err(e) => {
                eprintln!("[TcpClientTransport] Read error: {}", e);
                break;
            }
        }
    }

    // Drain all pending requests so callers don't hang indefinitely.
    let mut pending = inner.pending.lock().await;
    for (_, tx) in pending.drain() {
        let _ = tx.send(Err(RpcClientError::Transport(
            "connection closed".to_string(),
        )));
    }

    eprintln!("[TcpClientTransport] Connection closed");
}

fn data_to_json(data: RpcData) -> serde_json::Value {
    match data {
        RpcData::Json(v) => v,
        RpcData::Text(s) => serde_json::Value::String(s),
        // TCP wire format is JSON; binary payloads are not supported.
        RpcData::Binary(_) => serde_json::Value::Null,
    }
}

#[async_trait]
impl RpcClientTransport for TcpClientTransport {
    async fn connect(&self) -> Result<(), RpcClientError> {
        self.get_or_connect().await?;
        Ok(())
    }

    async fn send(&self, pattern: &str, data: RpcData) -> Result<RpcData, RpcClientError> {
        let inner = self.get_or_connect().await?;
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed).to_string();
        let (tx, rx) = oneshot::channel();

        inner.pending.lock().await.insert(id.clone(), tx);

        let msg = serde_json::json!({
            "pattern": pattern,
            "data": data_to_json(data),
            "id": id,
        });
        let mut frame = msg.to_string();
        frame.push('\n');

        if let Err(e) = inner.writer.lock().await.write_all(frame.as_bytes()).await {
            inner.pending.lock().await.remove(&id);
            return Err(RpcClientError::Transport(e.to_string()));
        }

        match tokio::time::timeout(self.timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(RpcClientError::Transport("connection closed".to_string())),
            Err(_) => {
                inner.pending.lock().await.remove(&id);
                Err(RpcClientError::Timeout)
            }
        }
    }

    async fn emit(&self, pattern: &str, data: RpcData) -> Result<(), RpcClientError> {
        let inner = self.get_or_connect().await?;

        // No id field — server sends no reply.
        let msg = serde_json::json!({
            "pattern": pattern,
            "data": data_to_json(data),
        });
        let mut frame = msg.to_string();
        frame.push('\n');

        let result = inner
            .writer
            .lock()
            .await
            .write_all(frame.as_bytes())
            .await
            .map_err(|e| RpcClientError::Transport(e.to_string()));
        result
    }
}
