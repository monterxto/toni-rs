use std::{any::Any, sync::Arc};

use async_trait::async_trait;
use rustc_hash::FxHashMap;

use super::{ErrorHandler, Guard, Interceptor, Pipe, middleware::Middleware};
use crate::{ProviderScope, http_helpers::HttpRequest};

#[async_trait]
pub trait Provider: Send + Sync {
    fn get_token(&self) -> String;
    async fn execute(
        &self,
        params: Vec<Box<dyn Any + Send>>,
        req: Option<&HttpRequest>,
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
    async fn build(
        &self,
        deps: FxHashMap<String, Arc<Box<dyn Provider>>>,
    ) -> Arc<Box<dyn Provider>>;
}
