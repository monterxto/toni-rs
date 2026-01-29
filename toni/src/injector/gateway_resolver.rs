use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::Result;

use crate::traits_helpers::{ErrorHandler, Guard, Interceptor, Pipe};
use crate::websocket::{GatewayTrait, GatewayWrapper};

use super::ToniContainer;

/// Resolves WebSocket gateways from DI container
///
/// Similar to RoutesResolver for HTTP, this discovers gateway providers
/// and wraps them with guards, interceptors, pipes, and error handlers.
pub struct GatewayResolver {
    container: Rc<RefCell<ToniContainer>>,
}

impl GatewayResolver {
    pub fn new(container: Rc<RefCell<ToniContainer>>) -> Self {
        Self { container }
    }

    /// Resolve gateways and register with WebSocket adapter
    pub fn resolve<W>(&self, ws_adapter: &mut W) -> Result<()>
    where
        W: crate::adapter::WebSocketAdapter,
    {
        let modules_tokens = self.container.borrow().get_modules_token();

        for module_token in modules_tokens {
            self.register_gateways(&module_token, ws_adapter)?;
        }

        Ok(())
    }

    fn register_gateways<W>(
        &self,
        module_token: &str,
        ws_adapter: &mut W,
    ) -> Result<()>
    where
        W: crate::adapter::WebSocketAdapter,
    {
        // Get all providers from module
        let container = self.container.borrow();
        let providers = container.get_providers_instance(&module_token.to_string())?;

        // Filter providers that implement GatewayTrait
        for (_token, provider) in providers {
            if let Some(gateway) = self.try_get_gateway(&provider) {
                // Get path before moving gateway
                let path = gateway.get_path();

                // Wrap gateway with enhancers
                let wrapper = self.wrap_gateway(gateway)?;

                // Register with adapter
                ws_adapter.add_gateway(&path, Arc::new(wrapper));

                println!("Registered WebSocket gateway at path: {}", path);
            }
        }

        Ok(())
    }

    /// Try to extract gateway from provider
    fn try_get_gateway(
        &self,
        provider: &Arc<Box<dyn crate::traits_helpers::ProviderTrait>>,
    ) -> Option<Arc<Box<dyn GatewayTrait>>> {
        provider.as_gateway()
    }

    fn wrap_gateway(&self, gateway: Arc<Box<dyn GatewayTrait>>) -> Result<GatewayWrapper> {
        let guards = self.resolve_guards(gateway.get_guard_tokens())?;
        let interceptors = self.resolve_interceptors(gateway.get_interceptor_tokens())?;
        let pipes = self.resolve_pipes(gateway.get_pipe_tokens())?;
        let error_handlers = self.resolve_error_handlers(gateway.get_error_handler_tokens())?;
        let route_metadata = gateway.get_route_metadata();

        Ok(GatewayWrapper::new(
            gateway,
            guards,
            interceptors,
            pipes,
            error_handlers,
            route_metadata,
        ))
    }

    fn resolve_guards(&self, tokens: Vec<String>) -> Result<Vec<Arc<dyn Guard>>> {
        let mut guards = Vec::new();

        // Add global guards first
        let global_guards = self.container.borrow().get_global_enhancers().guards;
        guards.extend(global_guards);

        // Resolve gateway-specific guards from tokens
        for token in tokens {
            if let Some(guard) = self.resolve_guard_by_token(&token)? {
                guards.push(guard);
            }
        }

        Ok(guards)
    }

    fn resolve_interceptors(&self, tokens: Vec<String>) -> Result<Vec<Arc<dyn Interceptor>>> {
        let mut interceptors = Vec::new();

        // Add global interceptors first
        let global_interceptors = self.container.borrow().get_global_enhancers().interceptors;
        interceptors.extend(global_interceptors);

        // Resolve gateway-specific interceptors from tokens
        for token in tokens {
            if let Some(interceptor) = self.resolve_interceptor_by_token(&token)? {
                interceptors.push(interceptor);
            }
        }

        Ok(interceptors)
    }

    fn resolve_pipes(&self, tokens: Vec<String>) -> Result<Vec<Arc<dyn Pipe>>> {
        let mut pipes = Vec::new();

        // Add global pipes first
        let global_pipes = self.container.borrow().get_global_enhancers().pipes;
        pipes.extend(global_pipes);

        // Resolve gateway-specific pipes from tokens
        for token in tokens {
            if let Some(pipe) = self.resolve_pipe_by_token(&token)? {
                pipes.push(pipe);
            }
        }

        Ok(pipes)
    }

    fn resolve_error_handlers(
        &self,
        tokens: Vec<String>,
    ) -> Result<Vec<Arc<dyn ErrorHandler>>> {
        let mut error_handlers = Vec::new();

        // Add global error handlers first
        let global_error_handlers = self.container.borrow().get_global_enhancers().error_handlers;
        error_handlers.extend(global_error_handlers);

        // Resolve gateway-specific error handlers from tokens
        for token in tokens {
            if let Some(error_handler) = self.resolve_error_handler_by_token(&token)? {
                error_handlers.push(error_handler);
            }
        }

        Ok(error_handlers)
    }

    /// Resolve a guard by its token from the DI container
    fn resolve_guard_by_token(&self, token: &str) -> Result<Option<Arc<dyn Guard>>> {
        let container = self.container.borrow();
        let modules_tokens = container.get_modules_token();

        for module_token in modules_tokens {
            let providers = container.get_providers_instance(&module_token)?;
            if let Some(provider) = providers.get(token) {
                if let Some(guard) = provider.as_guard() {
                    return Ok(Some(guard));
                }
            }
        }

        Ok(None)
    }

    /// Resolve an interceptor by its token from the DI container
    fn resolve_interceptor_by_token(&self, token: &str) -> Result<Option<Arc<dyn Interceptor>>> {
        let container = self.container.borrow();
        let modules_tokens = container.get_modules_token();

        for module_token in modules_tokens {
            let providers = container.get_providers_instance(&module_token)?;
            if let Some(provider) = providers.get(token) {
                if let Some(interceptor) = provider.as_interceptor() {
                    return Ok(Some(interceptor));
                }
            }
        }

        Ok(None)
    }

    /// Resolve a pipe by its token from the DI container
    fn resolve_pipe_by_token(&self, token: &str) -> Result<Option<Arc<dyn Pipe>>> {
        let container = self.container.borrow();
        let modules_tokens = container.get_modules_token();

        for module_token in modules_tokens {
            let providers = container.get_providers_instance(&module_token)?;
            if let Some(provider) = providers.get(token) {
                if let Some(pipe) = provider.as_pipe() {
                    return Ok(Some(pipe));
                }
            }
        }

        Ok(None)
    }

    /// Resolve an error handler by its token from the DI container
    fn resolve_error_handler_by_token(
        &self,
        token: &str,
    ) -> Result<Option<Arc<dyn ErrorHandler>>> {
        let container = self.container.borrow();
        let modules_tokens = container.get_modules_token();

        for module_token in modules_tokens {
            let providers = container.get_providers_instance(&module_token)?;
            if let Some(provider) = providers.get(token) {
                if let Some(error_handler) = provider.as_error_handler() {
                    return Ok(Some(error_handler));
                }
            }
        }

        Ok(None)
    }
}
