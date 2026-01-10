use std::{any::Any, sync::Arc};

use async_trait::async_trait;
use rustc_hash::FxHashMap;

use super::{ErrorHandler, Guard, Interceptor, Pipe, middleware::Middleware};
use crate::{ProviderScope, http_helpers::HttpRequest};

#[async_trait]
pub trait ProviderTrait: Send + Sync {
    fn get_token(&self) -> String;
    async fn execute(
        &self,
        params: Vec<Box<dyn Any + Send>>,
        req: Option<&HttpRequest>,
    ) -> Box<dyn Any + Send>;
    fn get_token_manager(&self) -> String;
    fn get_scope(&self) -> ProviderScope {
        ProviderScope::Singleton // Default to singleton
    }

    // Enhancer detection methods - return None by default
    // These are overridden by the #[injectable] macro for actual enhancers

    /// Returns this provider as a Guard if it implements the Guard trait
    fn as_guard(&self) -> Option<Arc<dyn Guard>> {
        None
    }

    /// Returns this provider as an Interceptor if it implements the Interceptor trait
    fn as_interceptor(&self) -> Option<Arc<dyn Interceptor>> {
        None
    }

    /// Returns this provider as a Pipe if it implements the Pipe trait
    fn as_pipe(&self) -> Option<Arc<dyn Pipe>> {
        None
    }

    /// Returns this provider as Middleware if it implements the Middleware trait
    fn as_middleware(&self) -> Option<Arc<dyn Middleware>> {
        None
    }

    /// Returns this provider as an ErrorHandler if it implements the ErrorHandler trait
    fn as_error_handler(&self) -> Option<Arc<dyn ErrorHandler>> {
        None
    }
}

#[async_trait]
pub trait Provider {
    async fn get_all_providers(
        &self,
        dependencies: &FxHashMap<String, Arc<Box<dyn ProviderTrait>>>,
    ) -> FxHashMap<String, Arc<Box<dyn ProviderTrait>>>;
    fn get_name(&self) -> String;
    fn get_token(&self) -> String;
    fn get_dependencies(&self) -> Vec<String>;
}
