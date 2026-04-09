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
    adapter::{
        ErasedRpcAdapter, ErasedWebSocketAdapter, RpcAdapter, RpcMessageCallbacks,
        WebSocketAdapter, WsConnectionCallbacks,
    },
    application_context::ToniApplicationContext,
    http_adapter::{ErasedHttpAdapter, HttpAdapter},
    injector::{GatewayResolver, IntoToken, RpcControllerResolver, ToniContainer},
    router::RoutesResolver,
    rpc::{RpcContext, RpcControllerWrapper, RpcData, RpcError},
    websocket::{
        BroadcastService, DisconnectReason, GatewayWrapper, WsClientMap, WsError, WsMessage,
        helpers::create_client_from_headers,
    },
};

pub struct ToniApplication {
    http_adapter: Option<Box<dyn ErasedHttpAdapter>>,
    http_port: Option<u16>,
    http_hostname: Option<String>,
    routes_resolver: RoutesResolver,
    context: ToniApplicationContext,
    ws_gateways: HashMap<String, Arc<GatewayWrapper>>,
    ws_adapter: Option<Box<dyn ErasedWebSocketAdapter>>,
    rpc_adapter: Option<Box<dyn ErasedRpcAdapter>>,
    rpc_controllers: Vec<Arc<RpcControllerWrapper>>,
}

impl ToniApplication {
    pub fn new(container: Rc<RefCell<ToniContainer>>) -> Self {
        Self {
            http_adapter: None,
            http_port: None,
            http_hostname: None,
            context: ToniApplicationContext::new(container.clone()),
            routes_resolver: RoutesResolver::new(container),
            ws_gateways: HashMap::new(),
            ws_adapter: None,
            rpc_adapter: None,
            rpc_controllers: Vec::new(),
        }
    }

    pub fn use_http_adapter<A: HttpAdapter + 'static>(
        &mut self,
        adapter: A,
        port: u16,
        hostname: &str,
    ) -> Result<&mut Self> {
        let mut boxed = Box::new(adapter) as Box<dyn ErasedHttpAdapter>;
        self.routes_resolver.resolve(boxed.as_mut())?;
        self.http_adapter = Some(boxed);
        self.http_port = Some(port);
        self.http_hostname = Some(hostname.to_string());
        tracing::debug!("HTTP adapter registered");
        Ok(self)
    }

    /// Gateway discovery is deferred to `start()` to allow adapter configuration beforehand.
    pub fn use_websocket_adapter<A>(&mut self, adapter: A) -> Result<&mut Self>
    where
        A: WebSocketAdapter,
    {
        self.ws_adapter = Some(Box::new(adapter) as Box<dyn ErasedWebSocketAdapter>);
        tracing::debug!("WebSocket adapter registered");
        Ok(self)
    }

    pub fn use_rpc_adapter<A>(&mut self, adapter: A) -> Result<&mut Self>
    where
        A: RpcAdapter,
    {
        self.rpc_adapter = Some(Box::new(adapter) as Box<dyn ErasedRpcAdapter>);
        tracing::debug!("RPC adapter registered");
        Ok(self)
    }

    fn discover_gateways(&mut self) -> Result<()> {
        let resolver = GatewayResolver::new(self.routes_resolver.container.clone());
        self.ws_gateways = resolver.resolve()?;

        if !self.ws_gateways.is_empty() {
            tracing::debug!(
                count = self.ws_gateways.len(),
                "WebSocket gateways discovered"
            );
        }

        Ok(())
    }

    fn discover_rpc_controllers(&mut self) -> Result<()> {
        let resolver = RpcControllerResolver::new(self.routes_resolver.container.clone());
        self.rpc_controllers = resolver.resolve()?;

        if !self.rpc_controllers.is_empty() {
            tracing::debug!(
                count = self.rpc_controllers.len(),
                "RPC controllers discovered"
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
        tracing::info!("Application shutting down");
        self.call_module_destroy_hooks().await;
        self.call_before_shutdown_hooks(None).await;
        self.call_shutdown_hooks(None).await;

        if let Ok(bs) = self.get::<BroadcastService>().await {
            bs.close_all().await;
        }

        if let Some(http) = &mut self.http_adapter {
            let _ = http.close().await;
        }

        if let Some(ws) = &mut self.ws_adapter {
            let _ = ws.close().await;
        }

        if let Some(rpc) = &mut self.rpc_adapter {
            let _ = rpc.close().await;
        }

        tracing::info!("Application shutdown complete");
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

    pub async fn start(&mut self) {
        {
            let mut scanner = crate::scanner::ToniDependenciesScanner::new(
                self.routes_resolver.container.clone(),
            );
            if let Err(e) = scanner.call_bootstrap_hooks().await {
                tracing::error!(error = %e, "Bootstrap hooks failed");
            }
        }

        if let Err(e) = self.discover_gateways() {
            tracing::error!(error = %e, "WebSocket gateway discovery failed");
        }

        if let Err(e) = self.discover_rpc_controllers() {
            tracing::error!(error = %e, "RPC controller discovery failed");
        }

        let http_port = self.http_port;
        let hostname = self
            .http_hostname
            .clone()
            .unwrap_or_else(|| "0.0.0.0".to_string());

        // One shared WsClientMap + ConnectionManager when BroadcastService is in DI;
        // otherwise a fresh WsClientMap per gateway (no CM needed).
        let broadcast_service = self.get::<BroadcastService>().await.ok().map(Arc::new);

        let (same_port, separate_port): (Vec<_>, Vec<_>) = self
            .ws_gateways
            .iter()
            .map(|(p, gw)| (p.clone(), gw.clone()))
            .partition(|(_, gw)| {
                let p = gw.get_port();
                p.is_none() || http_port.map_or(false, |hp| p == Some(hp))
            });

        // Wire same-port gateways into the HTTP adapter as upgrade routes.
        if !same_port.is_empty() {
            if self.http_adapter.is_none() {
                for (path, gw) in &same_port {
                    tracing::error!(
                        path,
                        "Gateway requests same-port WebSocket but no HTTP adapter registered; \
                         call use_http_adapter() to add one"
                    );
                    let _ = gw;
                }
            } else {
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
                    if let Some(http) = &mut self.http_adapter {
                        if let Err(e) = http.bind_ws(path, callbacks) {
                            tracing::error!(path, error = %e, "Failed to add WebSocket route");
                        } else {
                            tracing::debug!(path, "WebSocket gateway bound");
                            gateway.call_after_init().await;
                        }
                    }
                }
            }
        }

        let mut server_futures: Vec<Pin<Box<dyn Future<Output = ()> + Send + 'static>>> = vec![];

        // Wire separate-port gateways into the standalone WS adapter.
        if !separate_port.is_empty() {
            if self.ws_adapter.is_none() {
                for (path, gw) in &separate_port {
                    tracing::error!(
                        path,
                        port = ?gw.get_port(),
                        "Gateway requests separate-port WebSocket but no WebSocket adapter registered; \
                         call use_websocket_adapter() to add one"
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
                                tracing::error!(path, error = %e, "Failed to bind gateway");
                            } else {
                                tracing::debug!(port = ws_port, path, "WebSocket gateway bound");
                                gateway.call_after_init().await;
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
                                match ws.create(ws_port, &hostname) {
                                    Ok(fut) => server_futures.push(fut),
                                    Err(e) => tracing::error!(
                                        port = ws_port,
                                        error = %e,
                                        "Failed to create WebSocket server"
                                    ),
                                }
                            }
                        }
                    }
                }
            }
        }

        // Wire RPC controllers into the RPC adapter.
        if !self.rpc_controllers.is_empty() {
            if self.rpc_adapter.is_none() {
                tracing::error!(
                    count = self.rpc_controllers.len(),
                    "RPC controllers discovered but no RPC adapter registered; \
                     call use_rpc_adapter() to add one"
                );
            } else {
                let all_patterns: Vec<String> = self
                    .rpc_controllers
                    .iter()
                    .flat_map(|w| w.get_patterns())
                    .collect();

                for pattern in &all_patterns {
                    tracing::debug!(pattern = %pattern, "RPC pattern registered");
                }

                let callbacks = Arc::new(make_rpc_callbacks(self.rpc_controllers.clone()));

                if let Some(rpc) = &mut self.rpc_adapter {
                    if let Err(e) = rpc.bind(&all_patterns, callbacks) {
                        tracing::error!(error = %e, "Failed to bind RPC controllers");
                    } else if let Ok(fut) = rpc.create() {
                        server_futures.push(fut);
                    }
                }
            }
        }

        if let Some(http_adapter) = &mut self.http_adapter {
            let port = self.http_port.unwrap();
            let has_same_port_ws = !same_port.is_empty();
            let server_type = if has_same_port_ws {
                "HTTP + WebSocket"
            } else {
                "HTTP"
            };
            tracing::info!(server_type, host = %hostname, port, "Starting server");
            match http_adapter.create(port, &hostname) {
                Ok(fut) => server_futures.push(fut),
                Err(e) => {
                    tracing::error!(error = %e, "Failed to create HTTP server");
                    return;
                }
            }
        } else if server_futures.is_empty() {
            tracing::error!(
                "No adapters configured; register at least one adapter before calling start()"
            );
            return;
        }

        futures::future::join_all(server_futures).await;
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

/// Build the message callbacks for all RPC controllers.
///
/// Constructs a pattern → wrapper index at call time so the hot path
/// (per-message dispatch) is a single HashMap lookup.
fn make_rpc_callbacks(wrappers: Vec<Arc<RpcControllerWrapper>>) -> RpcMessageCallbacks {
    let mut pattern_map: HashMap<String, Arc<RpcControllerWrapper>> = HashMap::new();
    for wrapper in &wrappers {
        for pattern in wrapper.get_patterns() {
            pattern_map.insert(pattern, wrapper.clone());
        }
    }
    let pattern_map = Arc::new(pattern_map);

    RpcMessageCallbacks::new(move |data: RpcData, ctx: RpcContext| {
        let pattern_map = pattern_map.clone();
        Box::pin(async move {
            let pattern = ctx.pattern.clone();
            if let Some(wrapper) = pattern_map.get(&pattern) {
                wrapper.handle_message(data, ctx).await
            } else {
                Err(RpcError::PatternNotFound(pattern))
            }
        })
    })
}
