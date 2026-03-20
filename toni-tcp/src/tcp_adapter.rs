use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use toni::{RpcAdapter, RpcContext, RpcData, RpcMessageCallbacks};

/// TCP transport adapter for the Toni RPC gateway.
///
/// Uses newline-delimited JSON. Each message is a single JSON object followed
/// by `\n`. Inbound:
///
/// ```json
/// {"pattern":"order.create","data":{...},"id":"<correlation-id>"}
/// ```
///
/// `id` is optional. When present and the handler returns a reply
/// (request-response pattern), the response is written back:
///
/// ```json
/// {"id":"<correlation-id>","response":{...}}
/// ```
///
/// Fire-and-forget events (no `id`, or handlers declared with `#[event_pattern]`)
/// produce no response on the wire.
pub struct TcpAdapter {
    port: u16,
    callbacks: Option<Arc<RpcMessageCallbacks>>,
}

impl TcpAdapter {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            callbacks: None,
        }
    }
}

impl RpcAdapter for TcpAdapter {
    fn bind(&mut self, _patterns: &[String], callbacks: Arc<RpcMessageCallbacks>) -> Result<()> {
        self.callbacks = Some(callbacks);
        Ok(())
    }

    fn create(&mut self) -> Result<Pin<Box<dyn Future<Output = ()> + Send + 'static>>> {
        let port = self.port;
        let callbacks = self
            .callbacks
            .take()
            .expect("bind() must be called before create()");

        Ok(Box::pin(async move {
            let listener = TcpListener::bind(format!("0.0.0.0:{}", port))
                .await
                .unwrap_or_else(|e| panic!("TcpAdapter: failed to bind :{} — {}", port, e));

            println!("[TcpAdapter] Listening on :{}", port);

            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        let callbacks = callbacks.clone();
                        tokio::spawn(handle_connection(stream, addr, callbacks));
                    }
                    Err(e) => eprintln!("[TcpAdapter] Accept error: {}", e),
                }
            }
        }))
    }
}

async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    callbacks: Arc<RpcMessageCallbacks>,
) {
    let (reader, writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    // Shared across per-message spawns on this connection
    let writer = Arc::new(Mutex::new(writer));
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break, // clean close
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let msg: serde_json::Value = match serde_json::from_str(trimmed) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("[TcpAdapter] {}: JSON parse error: {}", addr, e);
                        continue;
                    }
                };

                let pattern = msg["pattern"].as_str().unwrap_or("").to_string();
                let data = RpcData::Json(msg["data"].clone());
                // `id` present → caller expects a response
                let id = msg["id"].as_str().map(|s| s.to_string());
                let ctx = RpcContext::new(pattern);

                let callbacks = callbacks.clone();
                let writer = writer.clone();

                tokio::spawn(async move {
                    let reply = callbacks.message(data, ctx).await;

                    if let (Some(id), Some(reply)) = (id, reply) {
                        let response_value = match reply {
                            RpcData::Json(v) => v,
                            RpcData::Text(s) => serde_json::Value::String(s),
                            RpcData::Binary(_) => serde_json::Value::Null,
                        };
                        let mut payload =
                            serde_json::json!({ "id": id, "response": response_value })
                                .to_string();
                        payload.push('\n');

                        let mut w = writer.lock().await;
                        if let Err(e) = w.write_all(payload.as_bytes()).await {
                            eprintln!("[TcpAdapter] Write error: {}", e);
                        }
                    }
                });
            }
            Err(e) => {
                eprintln!("[TcpAdapter] {}: read error: {}", addr, e);
                break;
            }
        }
    }
}
