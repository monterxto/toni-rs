use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
};

use anyhow::Result;

use crate::{
    adapter::{ErasedWebSocketAdapter, WebSocketAdapter, WsConnectionCallbacks},
    application_context::ToniApplicationContext,
    http_adapter::HttpAdapter,
    injector::{GatewayResolver, IntoToken, ToniContainer},
    router::RoutesResolver,
    websocket::{
        BroadcastService, DisconnectReason, GatewayWrapper, WsClient, WsClientMap, WsError,
        WsMessage, helpers::create_client_from_headers,
    },
};

pub struct ToniApplication<H: HttpAdapter> {
    http_adapter: H,
    routes_resolver: RoutesResolver,
    context: ToniApplicationContext,
    ws_gateways: HashMap<String, Arc<GatewayWrapper>>,
    ws_adapter: Option<Box<dyn ErasedWebSocketAdapter>>,
}

impl<H: HttpAdapter + 'static> ToniApplication<H> {
    pub fn new(http_adapter: H, container: Rc<RefCell<ToniContainer>>) -> Self {
        Self {
            http_adapter,
            context: ToniApplicationContext::new(container.clone()),
            routes_resolver: RoutesResolver::new(container),
            ws_gateways: HashMap::new(),
            ws_adapter: None,
        }
    }

    pub fn init(&mut self) -> Result<()> {
        self.routes_resolver.resolve(&mut self.http_adapter)?;
        Ok(())
    }

    /// Gateway discovery is deferred to `listen()` to allow adapter configuration beforehand.
    pub fn use_websocket_adapter<A>(&mut self, adapter: A) -> Result<&mut Self>
    where
        A: WebSocketAdapter,
    {
        self.ws_adapter = Some(Box::new(adapter) as Box<dyn ErasedWebSocketAdapter>);
        println!("✓ WebSocket adapter registered");
        Ok(self)
    }

    fn discover_gateways(&mut self) -> Result<()> {
        let resolver = GatewayResolver::new(self.routes_resolver.container.clone());
        self.ws_gateways = resolver.resolve()?;

        if !self.ws_gateways.is_empty() {
            println!(
                "✓ Discovered {} WebSocket gateway(s)",
                self.ws_gateways.len()
            );
        }

        Ok(())
    }

    /// Returns an instance of `T` from the DI container, searching across all modules.
    pub async fn get<T: 'static>(&self) -> Result<T> {
        self.context.get::<T>().await
    }

    /// Returns an instance of `T` from a specific module's scope in the DI container.
    pub async fn get_from<T: 'static>(&self, module_token: &str) -> Result<T> {
        self.context.get_from::<T>(module_token).await
    }

    /// Returns an instance from the DI container by token rather than type; use when providers
    /// are registered with a custom token.
    pub async fn get_by_token<T: 'static>(&self, token: impl IntoToken) -> Result<T> {
        self.context.get_by_token::<T>(token).await
    }

    /// Returns an instance by token from a specific module's scope in the DI container.
    pub async fn get_from_by_token<T: 'static>(
        &self,
        module_token: &str,
        token: impl IntoToken,
    ) -> Result<T> {
        self.context
            .get_from_by_token::<T>(module_token, token)
            .await
    }

    pub async fn close(&mut self) -> Result<()> {
        self.call_module_destroy_hooks().await;
        self.call_before_shutdown_hooks(None).await;
        self.call_shutdown_hooks(None).await;

        if let Ok(bs) = self.get::<BroadcastService>().await {
            bs.close_all().await;
        }

        let _ = self.http_adapter.close().await;

        if let Some(ws) = &mut self.ws_adapter {
            let _ = ws.close().await;
        }

        Ok(())
    }

    async fn call_before_shutdown_hooks(&self, signal: Option<String>) {
        self.context
            .call_before_shutdown_hooks(signal.clone())
            .await;

        let container = self.routes_resolver.container.borrow();
        let modules = container.get_modules_token();
        for module_token in modules {
            if let Some(module) = container.get_module_by_token(&module_token) {
                let controllers = module._get_controllers_instances();
                for (_token, wrapper) in controllers.iter() {
                    let controller = wrapper.get_instance();
                    controller.before_application_shutdown(signal.clone()).await;
                }
            }
        }
    }

    async fn call_module_destroy_hooks(&self) {
        self.context.call_module_destroy_hooks().await;

        let container = self.routes_resolver.container.borrow();
        let modules = container.get_modules_token();
        for module_token in modules {
            if let Some(module) = container.get_module_by_token(&module_token) {
                let controllers = module._get_controllers_instances();
                for (_token, wrapper) in controllers.iter() {
                    let controller = wrapper.get_instance();
                    controller.on_module_destroy().await;
                }
            }
        }
    }

    async fn call_shutdown_hooks(&self, signal: Option<String>) {
        self.context.call_shutdown_hooks(signal.clone()).await;

        let container = self.routes_resolver.container.borrow();
        let modules = container.get_modules_token();
        for module_token in modules {
            if let Some(module) = container.get_module_by_token(&module_token) {
                let controllers = module._get_controllers_instances();
                for (_token, wrapper) in controllers.iter() {
                    let controller = wrapper.get_instance();
                    controller.on_application_shutdown(signal.clone()).await;
                }
            }
        }
    }

    pub async fn listen(&mut self, port: u16, hostname: &str) {
        {
            let mut scanner = crate::scanner::ToniDependenciesScanner::new(
                self.routes_resolver.container.clone(),
            );
            if let Err(e) = scanner.call_bootstrap_hooks().await {
                eprintln!("Error during bootstrap hooks: {}", e);
            }
        }

        if let Err(e) = self.discover_gateways() {
            eprintln!("Error discovering WebSocket gateways: {}", e);
        }

        // One shared WsClientMap + ConnectionManager when BroadcastService is in DI;
        // otherwise a fresh WsClientMap per gateway (no CM needed).
        let broadcast_service = self.get::<BroadcastService>().await.ok().map(Arc::new);

        let (same_port, separate_port): (Vec<_>, Vec<_>) = self
            .ws_gateways
            .iter()
            .map(|(p, gw)| (p.clone(), gw.clone()))
            .partition(|(_, gw)| {
                let p = gw.get_port();
                p.is_none() || p == Some(port)
            });

        // Wire same-port gateways into the HTTP adapter as upgrade routes.
        for (path, gateway) in &same_port {
            let client_map = broadcast_service
                .as_ref()
                .map(|bs| bs.ws_client_map())
                .unwrap_or_else(|| Arc::new(WsClientMap::new()));
            let callbacks = Arc::new(make_ws_callbacks(
                gateway.clone(),
                client_map,
                broadcast_service.clone(),
            ));
            if let Err(e) = self.http_adapter.bind_ws(path, callbacks) {
                eprintln!("Failed to add WebSocket route at {}: {}", path, e);
            }
        }

        let mut ws_futures: Vec<Pin<Box<dyn Future<Output = ()> + Send + 'static>>> = vec![];

        // Wire separate-port gateways into the standalone WS adapter.
        if !separate_port.is_empty() {
            if self.ws_adapter.is_none() {
                for (path, gw) in &separate_port {
                    eprintln!(
                        "Gateway at {} requests port {:?} but no WebSocket adapter registered. \
                         Call use_websocket_adapter() to add one.",
                        path,
                        gw.get_port()
                    );
                }
            } else {
                // Bind all paths first.
                for (path, gateway) in &separate_port {
                    if let Some(ws_port) = gateway.get_port() {
                        let client_map = broadcast_service
                            .as_ref()
                            .map(|bs| bs.ws_client_map())
                            .unwrap_or_else(|| Arc::new(WsClientMap::new()));
                        let callbacks = Arc::new(make_ws_callbacks(
                            gateway.clone(),
                            client_map,
                            broadcast_service.clone(),
                        ));
                        if let Some(ws) = &mut self.ws_adapter {
                            if let Err(e) = ws.bind(ws_port, path, callbacks) {
                                eprintln!("Failed to bind gateway at {}: {}", path, e);
                            }
                        }
                    }
                }

                // Then seal each port — create returns the server future.
                let mut seen: HashSet<u16> = HashSet::new();
                for (_, gw) in &separate_port {
                    if let Some(ws_port) = gw.get_port() {
                        if seen.insert(ws_port) {
                            if let Some(ws) = &mut self.ws_adapter {
                                match ws.create(ws_port, hostname) {
                                    Ok(fut) => ws_futures.push(fut),
                                    Err(e) => eprintln!(
                                        "Failed to create WS server on port {}: {}",
                                        ws_port, e
                                    ),
                                }
                            }
                        }
                    }
                }
            }
        }

        let has_same_port_ws = !same_port.is_empty();
        let server_type = if has_same_port_ws {
            "HTTP + WebSocket"
        } else {
            "HTTP"
        };
        println!("Starting {} server on {}:{}", server_type, hostname, port);

        let hostname = hostname.to_string();
        let http_adapter = self.http_adapter.clone();
        ws_futures.push(Box::pin(async move {
            if let Err(e) = http_adapter.listen(port, &hostname).await {
                eprintln!("Failed to start server: {}", e);
                std::process::exit(1);
            }
        }));

        futures::future::join_all(ws_futures).await;
    }
}

/// Build the connection callbacks for one gateway.
///
/// `client_map` is either the shared map from `BroadcastService` (when BS is in DI) or
/// a fresh per-gateway map (when the user hasn't imported `BroadcastModule`).
/// `broadcast_service` is `Some` only when BS is in DI; the CM is wired through it.
fn make_ws_callbacks(
    gateway: Arc<GatewayWrapper>,
    client_map: Arc<WsClientMap>,
    broadcast_service: Option<Arc<BroadcastService>>,
) -> WsConnectionCallbacks {
    let g_connect = gateway.clone();
    let g_message = gateway.clone();
    let g_disconnect = gateway.clone();
    let h_message = client_map.clone();
    let h_disconnect = client_map.clone();
    let bs_connect = broadcast_service.clone();
    let bs_disconnect = broadcast_service;

    WsConnectionCallbacks::new(
        move |headers, sink| {
            let gateway = g_connect.clone();
            let bs = bs_connect.clone();
            let map = client_map.clone();
            Box::pin(async move {
                let client = create_client_from_headers(headers);
                let client_id = client.id.clone();
                gateway.begin_connect(client).await?;
                if let Some(bs) = &bs {
                    bs.connect(client_id.clone(), sink, gateway.get_namespace());
                } else {
                    map.register(client_id.clone(), sink);
                }
                gateway.complete_connect(&client_id).await?;
                Ok(client_id)
            })
        },
        move |client_id, msg| {
            let gateway = g_message.clone();
            let handle = h_message.clone();
            Box::pin(async move {
                match gateway.handle_message(client_id.clone(), msg).await {
                    Ok(Some(response)) => {
                        handle.send_to(&client_id, response).await;
                        true
                    }
                    Ok(None) => true,
                    Err(e) => {
                        let error_msg = WsMessage::text(
                            serde_json::json!({ "error": e.to_string() }).to_string(),
                        );
                        match &e {
                            WsError::ConnectionClosed(_) | WsError::AuthFailed(_) => false,
                            _ => {
                                handle.send_to(&client_id, error_msg).await;
                                true
                            }
                        }
                    }
                }
            })
        },
        move |client_id| {
            let gateway = g_disconnect.clone();
            let handle = h_disconnect.clone();
            let bs = bs_disconnect.clone();
            Box::pin(async move {
                if let Some(bs) = &bs {
                    bs.disconnect(&client_id);
                } else {
                    handle.unregister(&client_id);
                }
                gateway
                    .handle_disconnect(client_id, DisconnectReason::ClientDisconnect)
                    .await;
            })
        },
    )
}
