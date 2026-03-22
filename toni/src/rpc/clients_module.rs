use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use crate::FxHashMap;
use crate::adapter::RpcClientTransport;
use crate::async_trait;
use crate::http_helpers::HttpRequest;
use crate::injector::ToniContainer;
use crate::provider_scope::ProviderScope;
use crate::rpc::RpcClient;
use crate::traits_helpers::{ControllerFactory, ModuleMetadata, Provider, ProviderFactory};

/// Registers an [`RpcClient`] in the DI container and manages its connection lifecycle.
///
/// Unlike a plain `provider_value!`, `ClientsModule` eagerly calls
/// [`RpcClientTransport::connect`] during provider initialisation so that
/// connection failures surface at startup rather than on the first request.
/// To flush and close the connection on shutdown, inject the client into a
/// provider and call [`RpcClient::close`] in an `#[on_application_shutdown]` hook.
///
/// # Example
///
/// ```rust,no_run
/// #[module(
///     imports: [
///         ClientsModule::register("ORDER_CLIENT", NatsClientTransport::new("nats://localhost:4222")),
///     ],
/// )]
/// struct AppModule;
/// ```
///
/// Inject the client as usual:
///
/// ```rust,no_run
/// #[injectable(pub struct OrderService {
///     #[inject("ORDER_CLIENT")] client: RpcClient,
/// })]
/// ```
pub struct ClientsModule {
    token: String,
    transport: Arc<dyn RpcClientTransport>,
}

impl ClientsModule {
    pub fn register(token: impl Into<String>, transport: impl RpcClientTransport) -> Self {
        Self {
            token: token.into(),
            transport: Arc::new(transport),
        }
    }
}

impl ModuleMetadata for ClientsModule {
    fn get_id(&self) -> String {
        format!("ClientsModule:{}", self.token)
    }

    fn get_name(&self) -> String {
        format!("ClientsModule:{}", self.token)
    }

    fn imports(&self) -> Option<Vec<Box<dyn ModuleMetadata>>> {
        None
    }

    fn controllers(&self) -> Option<Vec<Box<dyn ControllerFactory>>> {
        None
    }

    fn providers(&self) -> Option<Vec<Box<dyn ProviderFactory>>> {
        Some(vec![Box::new(RpcClientFactory {
            token: self.token.clone(),
            transport: self.transport.clone(),
        })])
    }

    fn exports(&self) -> Option<Vec<String>> {
        Some(vec![self.token.clone()])
    }

    fn on_module_init(&self, _container: Rc<RefCell<ToniContainer>>) -> anyhow::Result<()> {
        Ok(())
    }
}

// ── provider internals ──────────────────────────────────────────────────────

struct RpcClientFactory {
    token: String,
    transport: Arc<dyn RpcClientTransport>,
}

#[async_trait]
impl ProviderFactory for RpcClientFactory {
    fn get_token(&self) -> String {
        self.token.clone()
    }

    async fn build(
        &self,
        _deps: FxHashMap<String, Arc<Box<dyn Provider>>>,
    ) -> Arc<Box<dyn Provider>> {
        if let Err(e) = self.transport.connect().await {
            eprintln!(
                "[ClientsModule] transport '{}' failed to connect at startup: {}",
                self.token, e
            );
        }

        Arc::new(Box::new(RpcClientProvider {
            token: self.token.clone(),
            client: RpcClient::from_arc(self.transport.clone()),
        }) as Box<dyn Provider>)
    }
}

struct RpcClientProvider {
    token: String,
    client: RpcClient,
}

#[async_trait]
impl Provider for RpcClientProvider {
    fn get_token(&self) -> String {
        self.token.clone()
    }

    async fn execute(
        &self,
        _params: Vec<Box<dyn Any + Send>>,
        _req: Option<&HttpRequest>,
    ) -> Box<dyn Any + Send> {
        Box::new(self.client.clone())
    }

    fn get_token_factory(&self) -> String {
        self.token.clone()
    }

    fn get_scope(&self) -> ProviderScope {
        ProviderScope::Singleton
    }
}
