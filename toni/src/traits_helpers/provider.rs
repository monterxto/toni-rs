use std::{any::Any, sync::Arc};

use async_trait::async_trait;
use rustc_hash::FxHashMap;

use super::{ErrorHandler, ProviderContext, Guard, Interceptor, Pipe, middleware::Middleware};
use crate::ProviderScope;

#[async_trait]
pub trait Provider: Send + Sync {
    fn get_token(&self) -> String;
    async fn execute(
        &self,
        params: Vec<Box<dyn Any + Send>>,
        ctx: ProviderContext<'_>,
    ) -> Box<dyn Any + Send>;
    fn get_token_factory(&self) -> String;
    fn get_scope(&self) -> ProviderScope {
        ProviderScope::Singleton
    }

    // Enhancer detection — overridden by the macro for guards, interceptors, etc.
    fn as_guard(&self) -> Option<Arc<dyn Guard>> {
        None
    }
    fn as_interceptor(&self) -> Option<Arc<dyn Interceptor>> {
        None
    }
    fn as_pipe(&self) -> Option<Arc<dyn Pipe>> {
        None
    }
    fn as_middleware(&self) -> Option<Arc<dyn Middleware>> {
        None
    }
    fn as_error_handler(&self) -> Option<Arc<dyn ErrorHandler>> {
        None
    }

    // Multi-provider support — overridden by generated multi-contribution providers.
    // Returns the base token this contribution belongs to (e.g. "PLUGINS").
    fn get_multi_base_token(&self) -> Option<String> {
        None
    }
    // Returns the type-erased contribution item (double-Arc: Arc<Arc<dyn Trait+Send+Sync>>
    // stored as Arc<dyn Any+Send+Sync>) so the instance loader can collect them.
    fn as_multi_item(&self) -> Option<Arc<dyn Any + Send + Sync>> {
        None
    }

    // Lifecycle hooks — overridden by the macro when the user annotates a method.
    // Default implementations are no-ops so providers without hooks incur no overhead.
    async fn on_module_init(&self) {}
    async fn on_application_bootstrap(&self) {}
    async fn on_module_destroy(&self) {}
    async fn before_application_shutdown(&self, _signal: Option<String>) {}
    async fn on_application_shutdown(&self, _signal: Option<String>) {}

    fn as_gateway(&self) -> Option<Arc<Box<dyn crate::websocket::GatewayTrait>>> {
        None
    }

    fn as_rpc_controller(&self) -> Option<Arc<Box<dyn crate::rpc::RpcControllerTrait>>> {
        None
    }
}

#[async_trait]
pub trait ProviderFactory {
    fn get_token(&self) -> String;
    fn get_dependencies(&self) -> Vec<String> {
        vec![]
    }
    // For multi-contribution factories: returns the base token under which all contributions
    // for the same logical multi-provider are grouped (e.g. "PLUGINS"). Returns None for
    // regular (non-multi) factories.
    fn get_multi_base_token(&self) -> Option<String> {
        None
    }
    async fn build(
        &self,
        deps: FxHashMap<String, Arc<Box<dyn Provider>>>,
    ) -> Arc<Box<dyn Provider>>;
}
