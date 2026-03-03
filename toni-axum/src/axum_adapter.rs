use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
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
use futures_util::StreamExt;
use serde_json::Value;
use std::str::FromStr;

use toni::websocket::{ConnectionManager, GatewayWrapper, WsError, WsMessage};
use toni::{
    async_trait, http_helpers::Extensions, Body as ToniBody, HttpAdapter, HttpMethod, HttpRequest,
    HttpResponse, InstanceWrapper, ToResponse, WebSocketAdapter,
};

use crate::axum_websocket_adapter::{
    axum_to_ws_message, extract_headers, ws_message_to_axum, AxumWsConnection,
};
use crate::TokioSender;

#[derive(Clone)]
pub struct AxumAdapter {
    instance: Router,
    shutdown_tx: Arc<watch::Sender<bool>>,
}

impl AxumAdapter {
    pub fn new() -> Self {
        let (tx, _) = watch::channel(false);
        Self {
            instance: Router::new(),
            shutdown_tx: Arc::new(tx),
        }
    }
}

impl HttpAdapter for AxumAdapter {
    type Request = Request<Body>;
    type Response = Response<Body>;

    async fn adapt_request(request: Self::Request) -> Result<HttpRequest> {
        let (mut parts, body) = request.into_parts();
        let body_bytes = to_bytes(body, usize::MAX).await?;

        let content_type = parts
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        let body = if content_type.contains("application/octet-stream")
            || content_type.contains("multipart/form-data")
        {
            ToniBody::Binary(body_bytes.to_vec())
        } else if let Ok(body_str) = String::from_utf8(body_bytes.to_vec()) {
            if let Ok(json) = serde_json::from_str::<Value>(&body_str) {
                ToniBody::Json(json)
            } else {
                ToniBody::Text(body_str)
            }
        } else {
            ToniBody::Binary(body_bytes.to_vec())
        };

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
            body,
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

        let mut body_content_type = "text/plain";

        let body = match response.body {
            Some(ToniBody::Text(text)) => Body::from(text),
            Some(ToniBody::Json(json)) => {
                body_content_type = "application/json";
                let vec = serde_json::to_vec(&json)
                    .map_err(|e| anyhow!("Failed to serialize JSON: {}", e))?;
                Body::from(vec)
            }
            Some(ToniBody::Binary(bytes)) => {
                body_content_type = "application/octet-stream";
                Body::from(bytes)
            }
            _ => Body::empty(),
        };

        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_str("Content-Type")
                .map_err(|e| anyhow!("Failed to parse header name: {}", e))?,
            HeaderValue::from_static(body_content_type),
        );

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

    async fn listen(self, port: u16, hostname: &str) -> Result<()> {
        let addr = format!("{}:{}", hostname, port);
        let listener = TcpListener::bind(&addr).await?;

        println!("Listening on {}", addr);

        let mut shutdown_rx = self.shutdown_tx.subscribe();
        axum::serve(listener, self.instance)
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

    fn bind_gateway(&mut self, path: &str, gateway: Arc<GatewayWrapper>) -> Result<()> {
        let gateway_clone = gateway.clone();

        self.instance = self.instance.clone().route(
            path,
            get(move |headers: HeaderMap, ws: WebSocketUpgrade| {
                let gateway = gateway_clone.clone();
                async move {
                    ws.on_upgrade(move |socket| async move {
                        let headers = extract_headers(&headers);
                        AxumAdapter::handle_connection(
                            AxumWsConnection::Full(socket),
                            &gateway,
                            headers,
                        )
                        .await;
                    })
                }
            }),
        );

        Ok(())
    }

    fn bind_gateway_with_broadcast(
        &mut self,
        path: &str,
        gateway: Arc<GatewayWrapper>,
        connection_manager: Arc<ConnectionManager>,
    ) -> Result<()> {
        let gateway_clone = gateway.clone();
        let cm = connection_manager.clone();

        self.instance = self.instance.clone().route(
            path,
            get(move |headers: HeaderMap, ws: WebSocketUpgrade| {
                let gateway = gateway_clone.clone();
                let cm = cm.clone();
                async move {
                    ws.on_upgrade(move |socket| async move {
                        let headers = extract_headers(&headers);
                        AxumAdapter::handle_connection_with_broadcast(
                            AxumWsConnection::Full(socket),
                            &gateway,
                            headers,
                            &cm,
                        )
                        .await;
                    })
                }
            }),
        );

        Ok(())
    }
}

#[async_trait]
impl WebSocketAdapter for AxumAdapter {
    type Connection = AxumWsConnection;
    type Sender = TokioSender;

    async fn recv(conn: &mut Self::Connection) -> Option<Result<WsMessage, WsError>> {
        match conn {
            AxumWsConnection::Full(ws) => ws.recv().await.map(|r| {
                r.map_err(|e| WsError::Internal(e.to_string()))
                    .and_then(axum_to_ws_message)
            }),
            AxumWsConnection::ReadOnly(stream) => stream.next().await.map(|r| {
                r.map_err(|e| WsError::Internal(e.to_string()))
                    .and_then(axum_to_ws_message)
            }),
        }
    }

    async fn send(conn: &mut Self::Connection, msg: WsMessage) -> Result<(), WsError> {
        match conn {
            AxumWsConnection::Full(ws) => {
                let axum_msg = ws_message_to_axum(msg)?;
                ws.send(axum_msg)
                    .await
                    .map_err(|e| WsError::Internal(e.to_string()))
            }
            AxumWsConnection::ReadOnly(_) => Err(WsError::Internal(
                "Cannot send on read-only connection".into(),
            )),
        }
    }

    fn split(conn: Self::Connection) -> (Self::Connection, Self::Sender) {
        match conn {
            AxumWsConnection::Full(ws) => {
                use futures_util::SinkExt;
                let (write, read) = ws.split();
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

                (AxumWsConnection::ReadOnly(read), TokioSender::new(tx))
            }
            AxumWsConnection::ReadOnly(_) => panic!("Cannot split an already-split connection"),
        }
    }
}
