use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};

use anyhow::Result;

use crate::{
    adapter::{ErasedWebSocketAdapter, WebSocketAdapter},
    application_context::ToniApplicationContext,
    http_adapter::HttpAdapter,
    injector::{GatewayResolver, IntoToken, ToniContainer},
    router::RoutesResolver,
    websocket::{ConnectionManager, GatewayWrapper},
};

pub struct ToniApplication<H: HttpAdapter> {
    http_adapter: H,
    routes_resolver: RoutesResolver,
    context: ToniApplicationContext,
    /// Discovered WebSocket gateways (path -> gateway)
    ws_gateways: HashMap<String, Arc<GatewayWrapper>>,
    /// WebSocket adapter (for separate port support)
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

        // TODO: for separate-port gateways ws_adapter is moved into a spawned task via
        // take() in listen(), so self.ws_adapter is None here. A shared shutdown channel
        // (like the watch::Sender used for HTTP) is needed to signal the spawned server.
        if let Some(ref mut ws) = self.ws_adapter {
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

        // Register WebSocket gateways — route each gateway based on its declared port.
        // No port (or same port as HTTP) → same-port HTTP upgrade.
        // Different port → separate WebSocket server via ws_adapter.
        let mut separate_port_gateways: Vec<(u16, Arc<GatewayWrapper>)> = Vec::new();

        if !self.ws_gateways.is_empty() {
            for (path, gateway) in &self.ws_gateways {
                let gateway_port = gateway.get_port();

                if gateway_port.is_none() || gateway_port == Some(port) {
                    // Same-port path: register as HTTP upgrade route
                    let result = if let Some(ref cm) = connection_manager {
                        self.http_adapter.bind_gateway_with_broadcast(
                            path,
                            gateway.clone(),
                            cm.clone(),
                        )
                    } else {
                        self.http_adapter.bind_gateway(path, gateway.clone())
                    };
                    if let Err(e) = result {
                        eprintln!("Failed to add WebSocket route at {}: {}", path, e);
                        eprintln!("This HTTP adapter may not support WebSocket upgrades");
                    }
                } else {
                    // Separate-port path: collect and register with ws_adapter below
                    let ws_port = gateway_port.unwrap();

                    match &mut self.ws_adapter {
                        None => {
                            eprintln!(
                                "Gateway at {} requests port {} but no WebSocketAdapter is registered. \
                                 Call use_websocket_adapter() to enable separate-port WebSocket.",
                                path, ws_port
                            );
                        }
                        Some(ws) => {
                            let result = if let Some(ref cm) = connection_manager {
                                ws.bind_gateway_with_broadcast(path, gateway.clone(), cm.clone())
                            } else {
                                ws.bind_gateway(path, gateway.clone())
                            };
                            if let Err(e) = result {
                                eprintln!(
                                    "Failed to register gateway at {} with WS adapter: {}",
                                    path, e
                                );
                            } else {
                                separate_port_gateways.push((ws_port, gateway.clone()));
                            }
                        }
                    }
                }
            }
        }

        // Start separate-port WebSocket servers for each unique port
        if !separate_port_gateways.is_empty() {
            let mut started_ports = std::collections::HashSet::new();
            for (ws_port, _) in &separate_port_gateways {
                if started_ports.insert(*ws_port) {
                    if let Some(mut ws) = self.ws_adapter.take() {
                        let ws_hostname = hostname.to_string();
                        let p = *ws_port;
                        tokio::spawn(async move {
                            if let Err(e) = ws.listen(p, &ws_hostname).await {
                                eprintln!("🚨 WebSocket server on port {} failed: {}", p, e);
                            }
                        });
                    }
                }
            }
        }

        let has_same_port_ws = self
            .ws_gateways
            .values()
            .any(|g| g.get_port().is_none() || g.get_port() == Some(port));
        let server_type = if has_same_port_ws {
            "HTTP + WebSocket"
        } else {
            "HTTP"
        };
        println!(
            "🚀 Starting {} server on {}:{}",
            server_type, hostname, port
        );

        // Clone adapter since listen() takes self by value
        //TODO: consider if `HttpAdapter::listen` should take only mut self
        if let Err(e) = self.http_adapter.clone().listen(port, hostname).await {
            eprintln!("Failed to start server: {}", e);
            std::process::exit(1);
        }
    }
}
