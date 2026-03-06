use std::{cell::RefCell, collections::HashMap, collections::HashSet, pin::Pin, rc::Rc, sync::Arc};

use anyhow::Result;

use crate::{
    adapter::{ErasedWebSocketAdapter, WebSocketAdapter, WsConnectionCallbacks},
    application_context::ToniApplicationContext,
    http_adapter::HttpAdapter,
    injector::{GatewayResolver, IntoToken, ToniContainer},
    router::RoutesResolver,
    websocket::{
        ConnectionManager, DisconnectReason, GatewayWrapper, WsClient, WsError, WsGatewayHandle,
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

        let connection_manager = self.get::<Arc<ConnectionManager>>().await.ok();

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
            let callbacks = Arc::new(make_ws_callbacks(
                gateway.clone(),
                connection_manager.clone(),
            ));
            if let Err(e) = self.http_adapter.bind_ws(path, callbacks) {
                eprintln!("Failed to add WebSocket route at {}: {}", path, e);
            }
        }

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
                let mut seen: HashSet<u16> = HashSet::new();
                for (_, gw) in &separate_port {
                    if let Some(ws_port) = gw.get_port() {
                        if seen.insert(ws_port) {
                            if let Some(ws) = &mut self.ws_adapter {
                                if let Err(e) = ws.create(ws_port) {
                                    eprintln!(
                                        "Failed to create WS server on port {}: {}",
                                        ws_port, e
                                    );
                                }
                            }
                        }
                    }
                }

                for (path, gateway) in &separate_port {
                    if let Some(ws_port) = gateway.get_port() {
                        let callbacks = Arc::new(make_ws_callbacks(
                            gateway.clone(),
                            connection_manager.clone(),
                        ));
                        if let Some(ws) = &mut self.ws_adapter {
                            if let Err(e) = ws.bind(ws_port, path, callbacks) {
                                eprintln!("Failed to bind gateway at {}: {}", path, e);
                            }
                        }
                    }
                }

                if let Some(ws) = &mut self.ws_adapter {
                    if let Err(e) = ws.listen(hostname).await {
                        eprintln!("WS adapter failed to start: {}", e);
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

        if let Err(e) = self.http_adapter.clone().listen(port, hostname).await {
            eprintln!("Failed to start server: {}", e);
            std::process::exit(1);
        }
    }
}

/// Build the connection callbacks for one gateway, embedding the handle and optional
/// ConnectionManager inside the closures so the adapter never sees framework internals.
fn make_ws_callbacks(
    gateway: Arc<GatewayWrapper>,
    connection_manager: Option<Arc<ConnectionManager>>,
) -> WsConnectionCallbacks {
    let handle = Arc::new(WsGatewayHandle::new());

    let g_connect = gateway.clone();
    let g_message = gateway.clone();
    let g_disconnect = gateway.clone();
    let h_connect = handle.clone();
    let h_message = handle.clone();
    let h_disconnect = handle.clone();
    let cm_connect = connection_manager.clone();
    let cm_disconnect = connection_manager.clone();

    WsConnectionCallbacks::new(
        move |headers, sender| {
            let gateway = g_connect.clone();
            let handle = h_connect.clone();
            let cm = cm_connect.clone();
            Box::pin(async move {
                let client = create_client_from_headers(headers);
                let client_id = client.id.clone();
                gateway.begin_connect(client).await?;
                handle.register(client_id.clone(), sender.clone());
                if let Some(cm) = &cm {
                    cm.register(WsClient::new(&client_id), sender, gateway.get_namespace());
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
                        handle.emit(&client_id, response).await;
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
                                handle.emit(&client_id, error_msg).await;
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
            let cm = cm_disconnect.clone();
            Box::pin(async move {
                handle.unregister(&client_id);
                if let Some(cm) = &cm {
                    cm.unregister(&client_id);
                }
                gateway
                    .handle_disconnect(client_id, DisconnectReason::ClientDisconnect)
                    .await;
            })
        },
    )
}
