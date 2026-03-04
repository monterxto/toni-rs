use std::{cell::RefCell, collections::HashMap, collections::HashSet, rc::Rc, sync::Arc};

use anyhow::Result;

use crate::{
    adapter::{ErasedWebSocketAdapter, WebSocketAdapter},
    application_context::ToniApplicationContext,
    http_adapter::HttpAdapter,
    injector::{GatewayResolver, IntoToken, ToniContainer},
    router::RoutesResolver,
    websocket::{ConnectionManager, GatewayWrapper, WsGatewayHandle},
};

pub struct ToniApplication<H: HttpAdapter> {
    http_adapter: H,
    routes_resolver: RoutesResolver,
    context: ToniApplicationContext,
    ws_gateways: HashMap<String, Arc<GatewayWrapper>>,
    ws_adapter: Option<Box<dyn ErasedWebSocketAdapter>>,
}

impl<H: HttpAdapter> ToniApplication<H> {
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

    /// Gateway discovery is deferred to `listen()` to allow adapter configuration beforehand
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

    /// Returns an instance of `T` from the DI container, searching across all modules
    pub async fn get<T: 'static>(&self) -> Result<T> {
        self.context.get::<T>().await
    }

    /// Returns an instance of `T` from a specific module's scope in the DI container
    pub async fn get_from<T: 'static>(&self, module_token: &str) -> Result<T> {
        self.context.get_from::<T>(module_token).await
    }

    /// Returns an instance from the DI container by token rather than type; use when providers are registered with a custom token
    pub async fn get_by_token<T: 'static>(&self, token: impl IntoToken) -> Result<T> {
        self.context.get_by_token::<T>(token).await
    }

    /// Returns an instance by token from a specific module's scope in the DI container
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

        if let Ok(cm) = self.get::<Arc<ConnectionManager>>().await {
            cm.close_all().await;
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

        // Resolve ConnectionManager if BroadcastModule is imported
        let connection_manager = self.get::<Arc<ConnectionManager>>().await.ok();

        // Collect gateways for same-port (HTTP upgrade) and separate-port paths
        let (same_port, separate_port): (Vec<_>, Vec<_>) = self
            .ws_gateways
            .iter()
            .map(|(p, gw)| (p.clone(), gw.clone()))
            .partition(|(_, gw)| {
                let p = gw.get_port();
                p.is_none() || p == Some(port)
            });

        // Wire same-port gateways into the HTTP adapter as upgrade routes
        for (path, gateway) in &same_port {
            let result = if let Some(ref cm) = connection_manager {
                self.http_adapter
                    .bind_gateway_with_broadcast(path, gateway.clone(), cm.clone())
            } else {
                self.http_adapter.bind_gateway(path, gateway.clone())
            };
            if let Err(e) = result {
                eprintln!("Failed to add WebSocket route at {}: {}", path, e);
            }
        }

        // Wire separate-port gateways into the WS adapter
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
                // Create a server entry for each unique port
                let mut seen: HashSet<u16> = HashSet::new();
                for (_, gw) in &separate_port {
                    if let Some(ws_port) = gw.get_port() {
                        if seen.insert(ws_port) {
                            if let Some(ws) = &mut self.ws_adapter {
                                if let Err(e) = ws.create(ws_port) {
                                    eprintln!("Failed to create WS server on port {}: {}", ws_port, e);
                                }
                            }
                        }
                    }
                }

                // Attach each gateway with its freshly created handle
                for (path, gateway) in &separate_port {
                    if let Some(ws_port) = gateway.get_port() {
                        let handle = Arc::new(WsGatewayHandle::new());
                        if let Some(ws) = &mut self.ws_adapter {
                            let result = if let Some(ref cm) = connection_manager {
                                ws.attach_with_broadcast(
                                    ws_port,
                                    path,
                                    gateway.clone(),
                                    handle,
                                    cm.clone(),
                                )
                            } else {
                                ws.attach(ws_port, path, gateway.clone(), handle)
                            };
                            if let Err(e) = result {
                                eprintln!("Failed to attach gateway at {}: {}", path, e);
                            }
                        }
                    }
                }

                // Start all WS servers (spawns tasks, returns immediately)
                if let Some(ws) = &mut self.ws_adapter {
                    if let Err(e) = ws.listen(hostname).await {
                        eprintln!("WS adapter failed to start: {}", e);
                    }
                }
            }
        }

        let has_same_port_ws = !same_port.is_empty();
        let server_type = if has_same_port_ws { "HTTP + WebSocket" } else { "HTTP" };
        println!("Starting {} server on {}:{}", server_type, hostname, port);

        if let Err(e) = self.http_adapter.clone().listen(port, hostname).await {
            eprintln!("Failed to start server: {}", e);
            std::process::exit(1);
        }
    }
}
