use anyhow::{Result, anyhow};
use rustc_hash::FxHashMap;
use std::sync::Arc;

use crate::traits_helpers::Provider;
use crate::traits_helpers::middleware::{Middleware, MiddlewareConfiguration};

/// Middleware manager for organizing middleware by module
///
/// This manages both global middleware (applies to all routes) and
/// module-specific middleware (scoped to certain routes)
pub struct MiddlewareManager {
    /// Global middleware that applies to all routes
    global_middleware: Vec<Arc<dyn Middleware>>,

    /// Module-specific middleware configurations
    /// Key: module token, Value: list of middleware configurations for that module
    module_middleware: FxHashMap<String, Vec<MiddlewareConfiguration>>,
}

impl MiddlewareManager {
    /// Create a new middleware manager
    pub fn new() -> Self {
        Self {
            global_middleware: Vec::new(),
            module_middleware: FxHashMap::default(),
        }
    }

    /// Add global middleware that applies to all routes
    ///
    /// # Example
    /// ```ignore
    /// manager.add_global(Arc::new(MyLoggerMiddleware::new()));
    /// ```
    pub fn add_global(&mut self, middleware: Arc<dyn Middleware>) {
        self.global_middleware.push(middleware);
    }

    /// Add middleware configuration for a specific module
    ///
    /// This is called internally by the framework when modules configure their middleware.
    /// Users typically don't call this directly - instead use `configure_middleware` in your module.
    pub fn add_for_module(&mut self, module_token: String, config: MiddlewareConfiguration) {
        self.module_middleware
            .entry(module_token)
            .or_insert_with(Vec::new)
            .push(config);
    }

    /// Get all middleware that should apply to a specific route
    ///
    /// This combines global middleware with module-specific middleware
    /// that matches the route pattern and HTTP method
    ///
    /// # Arguments
    /// * `module_token` - The token of the module the route belongs to
    /// * `route_path` - The path of the route (e.g., "/api/users")
    /// * `method` - The HTTP method (e.g., "GET", "POST")
    ///
    /// # Returns
    /// A vector of middleware that should be applied to this route
    pub fn get_middleware_for_route(
        &self,
        module_token: &str,
        route_path: &str,
        method: &str,
    ) -> Vec<Arc<dyn Middleware>> {
        let mut middleware = Vec::new();

        // Add global middleware first (executes first in chain)
        middleware.extend(self.global_middleware.iter().cloned());

        // Add module-specific middleware if applicable
        if let Some(configs) = self.module_middleware.get(module_token) {
            for config in configs {
                if config.should_apply(route_path, method) {
                    middleware.extend(config.middleware.iter().cloned());
                }
            }
        }

        middleware
    }

    /// Get reference to global middleware
    pub fn get_global_middleware(&self) -> &[Arc<dyn Middleware>] {
        &self.global_middleware
    }

    /// Get reference to module middleware map
    pub fn get_module_middleware(&self) -> &FxHashMap<String, Vec<MiddlewareConfiguration>> {
        &self.module_middleware
    }

    /// Resolve middleware tokens to actual instances from DI container
    ///
    /// This is called after the DI container is built, allowing middleware
    /// to have constructor dependencies injected.
    ///
    /// # Arguments
    /// * `module_token` - The token of the module
    /// * `providers` - Map of all provider instances in the module's DI container
    ///
    /// # Returns
    /// Result indicating success or error with details about which middleware failed
    pub fn resolve_middleware_tokens(
        &mut self,
        module_token: &str,
        providers: &FxHashMap<String, Arc<Box<dyn Provider>>>,
    ) -> Result<()> {
        if let Some(configs) = self.module_middleware.get_mut(module_token) {
            for config in configs {
                // Resolve each token to a middleware instance
                for token in &config.middleware_tokens {
                    if let Some(provider_box) = providers.get(token) {
                        // Call as_middleware() to get Arc<dyn Middleware>
                        if let Some(middleware) = provider_box.as_middleware() {
                            config.middleware.push(middleware);
                        } else {
                            return Err(anyhow!(
                                "Provider '{}' was expected to be Middleware but as_middleware() returned None. \
                                 Ensure the provider implements the Middleware trait.",
                                token
                            ));
                        }
                    } else {
                        return Err(anyhow!(
                            "Middleware provider '{}' not found in DI container for module '{}'. \
                             Ensure it's registered in the module's providers.",
                            token,
                            module_token
                        ));
                    }
                }
                // Clear tokens after resolution
                config.middleware_tokens.clear();
            }
        }
        Ok(())
    }
}

impl Default for MiddlewareManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        http_helpers::HttpRequest,
        traits_helpers::middleware::{Middleware, MiddlewareResult, Next},
    };
    use async_trait::async_trait;

    // Dummy middleware for testing
    struct DummyMiddleware {
        name: String,
    }

    impl DummyMiddleware {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    #[async_trait]
    impl Middleware for DummyMiddleware {
        async fn handle(&self, req: HttpRequest, next: Box<dyn Next>) -> MiddlewareResult {
            println!("DummyMiddleware {} executed", self.name);
            next.run(req).await
        }
    }

    #[test]
    fn test_middleware_manager_creation() {
        let manager = MiddlewareManager::new();
        assert_eq!(manager.get_global_middleware().len(), 0);
    }

    #[test]
    fn test_add_global_middleware() {
        let mut manager = MiddlewareManager::new();
        manager.add_global(Arc::new(DummyMiddleware::new("global")));

        assert_eq!(manager.get_global_middleware().len(), 1);
    }

    #[test]
    fn test_get_middleware_for_route_with_global_only() {
        let mut manager = MiddlewareManager::new();
        manager.add_global(Arc::new(DummyMiddleware::new("global")));

        let middleware = manager.get_middleware_for_route("TestModule", "/api/test", "GET");
        assert_eq!(middleware.len(), 1);
    }
}
