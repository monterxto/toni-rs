use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::net::TcpListener;

use axum::{
    body::Body,
    extract::ws::WebSocketUpgrade,
    http::{HeaderMap, Request},
    routing::{connect, delete, get, head, options, patch, post, put, trace},
    Router,
};
use toni::{HttpAdapter, HttpMethod, InstanceWrapper, RouteAdapter};

use super::AxumRouteAdapter;
use toni::websocket::{ConnectionManager, GatewayWrapper, WsSocket};

use crate::axum_websocket_adapter::{extract_headers, AxumWsSocket};

#[derive(Clone)]
pub struct AxumAdapter {
    instance: Router,
}

impl AxumAdapter {
    pub fn new() -> Self {
        Self {
            instance: Router::new(),
        }
    }
}

impl HttpAdapter for AxumAdapter {
    fn add_route(&mut self, path: &str, method: HttpMethod, handler: Arc<InstanceWrapper>) {
        let route_handler = move |req: Request<Body>| {
            let handler: Arc<InstanceWrapper> = handler.clone();
            Box::pin(async move {
                AxumRouteAdapter::handle_request(req, handler)
                    .await
                    .unwrap()
            })
        };
        println!("Adding route: {} {:?}", path, method);

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
        let listener: TcpListener = TcpListener::bind(&addr).await?;

        println!("Listening on {}", addr);

        axum::serve(listener, self.instance)
            .await
            .with_context(|| "Axum server encountered an error")?;
        Ok(())
    }

    fn on_upgrade(&mut self, path: &str, gateway: Arc<GatewayWrapper>) -> Result<()> {
        let gateway_clone = gateway.clone();

        self.instance = self.instance.clone().route(
            path,
            get(move |headers: HeaderMap, ws: WebSocketUpgrade| {
                let gateway = gateway_clone.clone();
                async move {
                    ws.on_upgrade(move |socket| async move {
                        let headers = extract_headers(&headers);
                        let mut ws_socket = AxumWsSocket::new(socket);
                        ws_socket.handle_connection(&gateway, headers).await;
                    })
                }
            }),
        );

        println!("✓ WebSocket route added: {}", path);
        Ok(())
    }

    fn on_upgrade_with_broadcast(
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
                        let ws_socket = AxumWsSocket::new(socket);
                        ws_socket
                            .handle_connection_with_broadcast(&gateway, headers, &cm)
                            .await;
                    })
                }
            }),
        );

        println!("✓ WebSocket route added (broadcast): {}", path);
        Ok(())
    }
}
