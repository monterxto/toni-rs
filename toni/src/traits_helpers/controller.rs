use std::sync::Arc;

use async_trait::async_trait;
use rustc_hash::FxHashMap;

use crate::http_helpers::{HttpMethod, HttpRequest, HttpResponse, RouteMetadata};

use super::{ErrorHandler, Guard, Interceptor, Pipe, provider::Provider, validate::Validatable};

#[async_trait]
pub trait Controller: Send + Sync {
    fn get_token(&self) -> String;
    async fn execute(&self, req: HttpRequest) -> HttpResponse;
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

    /// Returns the controller struct's type name, used to deduplicate lifecycle hook calls
    /// across per-route wrapper structs that share the same underlying controller instance.
    /// Returns an empty string for wrappers that have no lifecycle hooks.
    fn get_controller_type_name(&self) -> &'static str {
        ""
    }

    async fn on_module_init(&self) {}
    async fn on_application_bootstrap(&self) {}
    async fn before_application_shutdown(&self, _signal: Option<String>) {}
    async fn on_module_destroy(&self) {}
    async fn on_application_shutdown(&self, _signal: Option<String>) {}
}
#[async_trait]
pub trait ControllerFactory {
    fn get_token(&self) -> String;
    fn get_dependencies(&self) -> Vec<String> {
        vec![]
    }
    async fn build(
        &self,
        deps: FxHashMap<String, Arc<Box<dyn Provider>>>,
    ) -> Vec<Arc<Box<dyn Controller>>>;
}
