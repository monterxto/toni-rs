use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::watch;

use axum::{
    body::{to_bytes, Body},
    extract::{ws::WebSocketUpgrade, Path, Query},
    http::{HeaderMap, HeaderName, HeaderValue, Request, Response, StatusCode},
    routing::{connect, delete, get, head, options, patch, post, put, trace},
    RequestPartsExt, Router,
};
use futures_util::{SinkExt, StreamExt};
use std::str::FromStr;

use toni::websocket::{WsMessage, WsSink};
use toni::{
    async_trait, http_helpers::{Bytes as ToniBytes, Extensions}, HttpAdapter, HttpMethod,
    HttpRequest, HttpResponse, InstanceWrapper, ToResponse, WebSocketAdapter, WsConnectionCallbacks,
};

use crate::axum_websocket_adapter::{axum_to_ws_message, extract_headers, ws_message_to_axum};
use crate::tokio_sender::TokioSender;

#[derive(Clone)]
pub struct AxumAdapter {
    instance: Router,
    ws_ports: HashMap<u16, Router>,
    shutdown_tx: Arc<watch::Sender<bool>>,
    port: u16,
    hostname: String,
}

impl AxumAdapter {
    pub fn new(hostname: &str, port: u16) -> Self {
        let (tx, _) = watch::channel(false);
        Self {
            instance: Router::new(),
            ws_ports: HashMap::new(),
            shutdown_tx: Arc::new(tx),
            port,
            hostname: hostname.to_string(),
        }
    }
}

// ── Shared connection loop ────────────────────────────────────────────────────

/// Runs the full WebSocket connection lifecycle for one connected client.
///
/// Splits the socket, spawns a writer task, then pumps the read half through
/// the framework callbacks until the connection closes.
async fn run_ws_connection(
    socket: axum::extract::ws::WebSocket,
    callbacks: Arc<WsConnectionCallbacks>,
    headers_map: HashMap<String, String>,
) {
    let (write, read) = socket.split();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<WsMessage>(32);

    tokio::spawn(async move {
        let mut write = write;
        while let Some(msg) = rx.recv().await {
            if let Ok(axum_msg) = ws_message_to_axum(msg) {
                if write.send(axum_msg).await.is_err() {
                    break;
                }
            }
        }
    });

    let sender: Arc<dyn WsSink> = Arc::new(TokioSender::new(tx));

    let client_id = match callbacks.connect(headers_map, sender).await {
        Ok(id) => id,
        Err(_) => return,
    };

    let mut read = read;
    while let Some(result) = read.next().await {
        match result {
            Ok(axum_msg) => match axum_to_ws_message(axum_msg) {
                Ok(ws_msg) => {
                    if !callbacks.message(client_id.clone(), ws_msg).await {
                        break;
                    }
                }
                Err(_) => {}
            },
            Err(_) => break,
        }
    }

    callbacks.disconnect(client_id).await;
}

fn ws_route(callbacks: Arc<WsConnectionCallbacks>) -> axum::routing::MethodRouter {
    get(move |headers: HeaderMap, ws: WebSocketUpgrade| {
        let callbacks = callbacks.clone();
        async move {
            let headers_map = extract_headers(&headers);
            ws.on_upgrade(move |socket| run_ws_connection(socket, callbacks, headers_map))
        }
    })
}

// ── HttpAdapter ───────────────────────────────────────────────────────────────

impl HttpAdapter for AxumAdapter {
    type Request = Request<Body>;
    type Response = Response<Body>;

    async fn adapt_request(request: Self::Request) -> Result<HttpRequest> {
        let (mut parts, body) = request.into_parts();
        let body_bytes = to_bytes(body, usize::MAX).await?;

        let Path(path_params) = parts
            .extract::<Path<HashMap<String, String>>>()
            .await
            .map_err(|e| anyhow!("Failed to extract path parameters: {:?}", e))?;

        let Query(query_params) = parts
            .extract::<Query<HashMap<String, String>>>()
            .await
            .map_err(|e| anyhow!("Failed to extract query parameters: {:?}", e))?;

        let headers = parts
            .headers
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_str().unwrap_or("").to_string()))
            .collect();

        Ok(HttpRequest {
            body: ToniBytes::from(body_bytes.to_vec()),
            headers,
            method: parts.method.to_string(),
            uri: parts.uri.to_string(),
            query_params,
            path_params,
            extensions: Extensions::new(),
        })
    }

    fn adapt_response(
        response: Box<dyn ToResponse<Response = HttpResponse>>,
    ) -> Result<Self::Response> {
        let response = response.to_response();

        let status =
            StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

        let (body, body_content_type) = match response.body {
            Some(toni_body) => {
                let ct = toni_body.content_type().unwrap_or("application/octet-stream").to_string();
                (Body::from(toni_body.into_bytes().to_vec()), Some(ct))
            }
            None => (Body::empty(), None),
        };

        let mut headers = HeaderMap::new();
        if let Some(ct) = body_content_type {
            headers.insert(
                HeaderName::from_str("Content-Type")
                    .map_err(|e| anyhow!("Failed to parse header name: {}", e))?,
                HeaderValue::from_str(&ct)
                    .map_err(|e| anyhow!("Failed to parse content-type value: {}", e))?,
            );
        }

        for (k, v) in &response.headers {
            if let Ok(header_name) = HeaderName::from_bytes(k.as_bytes()) {
                if let Ok(header_value) = HeaderValue::from_str(v) {
                    headers.insert(header_name, header_value);
                }
            }
        }

        let mut res = Response::builder()
            .status(status)
            .body(body)
            .map_err(|e| anyhow!("Failed to build response: {}", e))?;

        res.headers_mut().extend(headers);

        Ok(res)
    }

    fn add_route(&mut self, path: &str, method: HttpMethod, handler: Arc<InstanceWrapper>) {
        let route_handler = move |req: Request<Body>| {
            let handler = handler.clone();
            Box::pin(async move { AxumAdapter::handle_request(req, handler).await.unwrap() })
        };

        self.instance = match method {
            HttpMethod::GET => self.instance.clone().route(path, get(route_handler)),
            HttpMethod::POST => self.instance.clone().route(path, post(route_handler)),
            HttpMethod::PUT => self.instance.clone().route(path, put(route_handler)),
            HttpMethod::DELETE => self.instance.clone().route(path, delete(route_handler)),
            HttpMethod::HEAD => self.instance.clone().route(path, head(route_handler)),
            HttpMethod::PATCH => self.instance.clone().route(path, patch(route_handler)),
            HttpMethod::OPTIONS => self.instance.clone().route(path, options(route_handler)),
            HttpMethod::TRACE => self.instance.clone().route(path, trace(route_handler)),
            HttpMethod::CONNECT => self.instance.clone().route(path, connect(route_handler)),
        };
    }

    fn port(&self) -> u16 {
        self.port
    }

    fn hostname(&self) -> &str {
        &self.hostname
    }

    async fn listen(self) -> Result<()> {
        let addr = format!("{}:{}", self.hostname, self.port);
        let listener = TcpListener::bind(&addr).await?;

        println!("Listening on {}", addr);

        let router = self.instance.fallback(|req: Request<Body>| async move {
            let method = req.method().as_str().to_uppercase();
            let path = req.uri().path().to_owned();
            let body = serde_json::json!({
                "statusCode": 404,
                "message": format!("Cannot {} {}", method, path),
                "error": "Not Found"
            });
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap()
        });

        let mut shutdown_rx = self.shutdown_tx.subscribe();
        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.wait_for(|v| *v).await;
            })
            .await
            .with_context(|| "Axum server encountered an error")?;
        Ok(())
    }

    fn close(&mut self) -> impl std::future::Future<Output = Result<()>> + Send {
        let tx = self.shutdown_tx.clone();
        async move {
            let _ = tx.send(true);
            Ok(())
        }
    }

    fn bind_ws(&mut self, path: &str, callbacks: Arc<WsConnectionCallbacks>) -> Result<()> {
        self.instance = self.instance.clone().route(path, ws_route(callbacks));
        Ok(())
    }
}

// ── WebSocketAdapter (separate-port) ─────────────────────────────────────────

#[async_trait]
impl WebSocketAdapter for AxumAdapter {
    fn bind(&mut self, port: u16, path: &str, callbacks: Arc<WsConnectionCallbacks>) -> Result<()> {
        let router = self.ws_ports.entry(port).or_insert_with(Router::new);
        *router = router.clone().route(path, ws_route(callbacks));
        Ok(())
    }

    fn create(
        &mut self,
        port: u16,
        hostname: &str,
    ) -> Result<Pin<Box<dyn Future<Output = ()> + Send + 'static>>> {
        let router = self
            .ws_ports
            .remove(&port)
            .ok_or_else(|| anyhow!("No routes registered for WS port {}", port))?;
        let addr = format!("{}:{}", hostname, port);
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        Ok(Box::pin(async move {
            let listener = match TcpListener::bind(&addr).await {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("Failed to bind WS port {}: {}", addr, e);
                    return;
                }
            };
            println!("WebSocket listening on {}", addr);
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.wait_for(|v| *v).await;
                })
                .await
                .ok();
        }))
    }
}
