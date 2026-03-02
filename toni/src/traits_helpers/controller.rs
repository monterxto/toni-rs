use std::sync::Arc;

use async_trait::async_trait;
use rustc_hash::FxHashMap;

use crate::http_helpers::{HttpMethod, HttpRequest, HttpResponse, RouteMetadata, ToResponse};

use super::{
    ErrorHandler, Guard, Interceptor, Pipe, provider::ProviderTrait, validate::Validatable,
};

#[async_trait]
pub trait ControllerTrait: Send + Sync {
    fn get_token(&self) -> String;
    async fn execute(
        &self,
        req: HttpRequest,
    ) -> Box<dyn ToResponse<Response = HttpResponse> + Send>;
    fn get_path(&self) -> String;
    fn get_method(&self) -> HttpMethod;

    /// Get guard instances (deprecated - use get_guard_tokens instead)
    fn get_guards(&self) -> Vec<Arc<dyn Guard>>;

    /// Get pipe instances (deprecated - use get_pipe_tokens instead)
    fn get_pipes(&self) -> Vec<Arc<dyn Pipe>>;

    /// Get interceptor instances (deprecated - use get_interceptor_tokens instead)
    fn get_interceptors(&self) -> Vec<Arc<dyn Interceptor>>;

    /// Get guard tokens for DI resolution
    fn get_guard_tokens(&self) -> Vec<String> {
        vec![]
    }

    /// Get interceptor tokens for DI resolution
    fn get_interceptor_tokens(&self) -> Vec<String> {
        vec![]
    }

    /// Get pipe tokens for DI resolution
    fn get_pipe_tokens(&self) -> Vec<String> {
        vec![]
    }

    /// Get error handler tokens for DI resolution
    fn get_error_handler_tokens(&self) -> Vec<String> {
        vec![]
    }

    fn get_error_handlers(&self) -> Vec<Arc<dyn ErrorHandler>> {
        vec![]
    }

    /// Get route metadata (roles, permissions, custom config)
    fn get_route_metadata(&self) -> Arc<RouteMetadata> {
        Arc::new(RouteMetadata::new())
    }

    fn get_body_dto(&self, req: &HttpRequest) -> Option<Box<dyn Validatable>>;

    // Lifecycle Hooks

    /// Called after dependency injection is complete.
    ///
    /// Use when initialization requires injected dependencies.
    async fn on_module_init(&self) {}

    /// Called after the application is fully initialized but before it starts listening.
    ///
    /// This is the last hook before the server begins accepting connections.
    async fn on_application_bootstrap(&self) {}

    /// Called before application shutdown begins.
    ///
    /// Use to stop accepting new work and prepare for shutdown.
    async fn before_application_shutdown(&self, _signal: Option<String>) {}

    /// Called during module destruction.
    async fn on_module_destroy(&self) {}

    /// Called during application shutdown.
    async fn on_application_shutdown(&self, _signal: Option<String>) {}
}
#[async_trait]
pub trait Controller {
    async fn get_all_controllers(
        &self,
        dependencies: &FxHashMap<String, Arc<Box<dyn ProviderTrait>>>,
    ) -> FxHashMap<String, Arc<Box<dyn ControllerTrait>>>;
    fn get_name(&self) -> String;
    fn get_token(&self) -> String;
    fn get_dependencies(&self) -> Vec<String>;
}
