use std::sync::Arc;

use async_trait::async_trait;
use rustc_hash::FxHashMap;

use crate::http_helpers::{HttpMethod, HttpRequest, HttpResponse, IntoResponse};

use super::{
    ErrorHandler, Guard, Interceptor, Pipe, provider::ProviderTrait, validate::Validatable,
};

#[async_trait]
pub trait ControllerTrait: Send + Sync {
    fn get_token(&self) -> String;
    async fn execute(
        &self,
        req: HttpRequest,
    ) -> Box<dyn IntoResponse<Response = HttpResponse> + Send>;
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

    /// Get error handler instances
    fn get_error_handlers(&self) -> Vec<Arc<dyn ErrorHandler>> {
        vec![]
    }

    fn get_body_dto(&self, req: &HttpRequest) -> Option<Box<dyn Validatable>>;
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
