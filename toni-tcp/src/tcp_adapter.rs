use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use toni::{RpcAdapter, RpcContext, RpcData, RpcError, RpcMessageCallbacks};

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
    host: String,
    port: u16,
    callbacks: Option<Arc<RpcMessageCallbacks>>,
}

impl TcpAdapter {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
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
        let host = self.host.clone();
        let port = self.port;
        let callbacks = self
            .callbacks
            .take()
            .expect("bind() must be called before create()");

        Ok(Box::pin(async move {
            let addr = format!("{}:{}", host, port);
            let listener = TcpListener::bind(&addr)
                .await
                .unwrap_or_else(|e| panic!("TcpAdapter: failed to bind {} — {}", addr, e));

            tracing::info!(addr, "TcpAdapter listening");

            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        let callbacks = callbacks.clone();
                        tokio::spawn(handle_connection(stream, addr, callbacks));
                    }
                    Err(e) => tracing::error!(error = %e, "TcpAdapter accept error"),
                }
            }
        }))
    }
}

fn error_status(e: &RpcError) -> &'static str {
    match e {
        RpcError::PatternNotFound(_) => "not_found",
        RpcError::Forbidden(_) => "forbidden",
        RpcError::Internal(_) => "error",
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
                        tracing::warn!(addr = %addr, error = %e, "TcpAdapter JSON parse error");
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
                    let outcome = callbacks.message(data, ctx).await;

                    let Some(id) = id else {
                        // Fire-and-forget: caller sent no id, never expects a reply.
                        return;
                    };

                    let payload_json = match outcome {
                        Ok(Some(reply)) => {
                            let v = match reply {
                                RpcData::Json(v) => v,
                                RpcData::Text(s) => serde_json::Value::String(s),
                                RpcData::Binary(_) => serde_json::Value::Null,
                            };
                            serde_json::json!({ "id": id, "response": v })
                        }
                        Ok(None) => {
                            // Handler is fire-and-forget (#[event_pattern]) but
                            // caller sent an id — send an explicit ack so caller
                            // can close the pending request rather than timing out.
                            serde_json::json!({ "id": id, "response": null })
                        }
                        Err(e) => {
                            let status = error_status(&e);
                            serde_json::json!({
                                "id": id,
                                "err": { "message": e.to_string(), "status": status }
                            })
                        }
                    };

                    let mut line = payload_json.to_string();
                    line.push('\n');

                    let mut w = writer.lock().await;
                    if let Err(e) = w.write_all(line.as_bytes()).await {
                        tracing::error!(error = %e, "TcpAdapter write error");
                    }
                });
            }
            Err(e) => {
                tracing::error!(addr = %addr, error = %e, "TcpAdapter read error");
                break;
            }
        }
    }
}
