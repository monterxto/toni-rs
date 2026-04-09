use anyhow::{anyhow, Result};
use std::collections::HashMap; // still needed for ws_ports, path_params
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::watch;

use axum::{
    body::Body,
    extract::{ws::WebSocketUpgrade, Path},
    http::{HeaderMap, HeaderName, HeaderValue, Request, Response, StatusCode},
    routing::{connect, delete, get, head, options, patch, post, put, trace},
    RequestPartsExt, Router,
};
use futures_util::{SinkExt, StreamExt};
use std::str::FromStr;

use toni::websocket::{WsMessage, WsSink};
use toni::{
    async_trait,
    http_adapter::HttpRequestCallbacks,
    http_helpers::{PathParams, RequestBody},
    HttpAdapter, HttpMethod, HttpRequest, HttpResponse, WebSocketAdapter, WsConnectionCallbacks,
};

use crate::axum_websocket_adapter::{axum_to_ws_message, extract_headers, ws_message_to_axum};
use crate::tokio_sender::TokioSender;

#[derive(Clone)]
pub struct AxumAdapter {
    instance: Router,
    ws_ports: HashMap<u16, Router>,
    shutdown_tx: Arc<watch::Sender<bool>>,
}

impl AxumAdapter {
    pub fn new() -> Self {
        let (tx, _) = watch::channel(false);
        Self {
            instance: Router::new(),
            ws_ports: HashMap::new(),
            shutdown_tx: Arc::new(tx),
        }
    }
}

impl Default for AxumAdapter {
    fn default() -> Self {
        Self::new()
    }
}

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

    tracing::debug!(client_id = %client_id, "WebSocket connection established");

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

    tracing::debug!(client_id = %client_id, "WebSocket connection closed");
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

impl AxumAdapter {
    async fn adapt_request(request: Request<Body>) -> Result<HttpRequest> {
        use http_body_util::BodyExt;

        let (mut parts, body) = request.into_parts();

        let Path(path_params) = parts
            .extract::<Path<HashMap<String, String>>>()
            .await
            .map_err(|e| anyhow!("Failed to extract path parameters: {:?}", e))?;

        parts.extensions.insert(PathParams(path_params));
        let box_body = body
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
            .boxed_unsync();
        Ok(HttpRequest::from_parts(
            parts,
            RequestBody::Streaming(box_body),
        ))
    }

    async fn adapt_response(response: HttpResponse) -> Result<Response<Body>> {
        let status =
            StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

        let (body, body_content_type) = match response.body {
            Some(toni_body) => {
                let ct = toni_body.content_type().map(|s| s.to_string());
                (Body::new(toni_body.into_box_body()), ct)
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
}

impl HttpAdapter for AxumAdapter {
    fn bind(&mut self, method: HttpMethod, path: &str, callbacks: Arc<HttpRequestCallbacks>) {
        let route_handler = move |req: Request<Body>| {
            let callbacks = callbacks.clone();
            Box::pin(async move {
                let http_req = match Self::adapt_request(req).await {
                    Ok(r) => r,
                    Err(e) => {
                        let body = serde_json::json!({
                            "statusCode": 500,
                            "message": e.to_string(),
                            "error": "Internal Server Error"
                        });
                        return Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .header("Content-Type", "application/json")
                            .body(Body::from(body.to_string()))
                            .unwrap();
                    }
                };
                let http_res = callbacks.handle(http_req).await;
                Self::adapt_response(http_res).await.unwrap_or_else(|e| {
                    let body = serde_json::json!({
                        "statusCode": 500,
                        "message": e.to_string(),
                        "error": "Internal Server Error"
                    });
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .header("Content-Type", "application/json")
                        .body(Body::from(body.to_string()))
                        .unwrap()
                })
            })
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

    fn bind_ws(&mut self, path: &str, callbacks: Arc<WsConnectionCallbacks>) -> Result<()> {
        self.instance = self.instance.clone().route(path, ws_route(callbacks));
        Ok(())
    }

    fn create(
        &mut self,
        port: u16,
        hostname: &str,
    ) -> Result<Pin<Box<dyn Future<Output = ()> + Send + 'static>>> {
        let router = std::mem::replace(&mut self.instance, Router::new());
        let hostname = hostname.to_string();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        let router = router.fallback(|req: Request<Body>| async move {
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

        Ok(Box::pin(async move {
            let addr = format!("{}:{}", hostname, port);
            let listener = match TcpListener::bind(&addr).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!(addr, error = %e, "Failed to bind HTTP port");
                    std::process::exit(1);
                }
            };
            tracing::info!(addr, "HTTP listening");
            if let Err(e) = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.wait_for(|v| *v).await;
                })
                .await
            {
                tracing::error!(error = %e, "HTTP server error");
                std::process::exit(1);
            }
        }))
    }

    fn close(&mut self) -> impl std::future::Future<Output = Result<()>> + Send {
        let tx = self.shutdown_tx.clone();
        async move {
            let _ = tx.send(true);
            Ok(())
        }
    }
}

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
                    tracing::error!(addr, error = %e, "Failed to bind WebSocket port");
                    return;
                }
            };
            tracing::info!(addr, "WebSocket listening");
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.wait_for(|v| *v).await;
                })
                .await
                .ok();
        }))
    }
}
