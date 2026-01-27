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
            // TODO: Try to downcast to GatewayTrait
            // This is a simplified approach - in production, we'd want a marker trait
            // or registry pattern to identify gateway providers
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
    ///  In production, use a marker trait or registry pattern to identify gateway providers
    fn try_get_gateway(
        &self,
        _provider: &Arc<Box<dyn crate::traits_helpers::ProviderTrait>>,
    ) -> Option<Arc<Box<dyn GatewayTrait>>> {
        // TODO: Implement proper gateway detection
        // For now, this returns None - gateways need to be registered via a different mechanism
        // or we need a marker trait to identify them
        None
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

    fn resolve_guards(&self, _tokens: Vec<String>) -> Result<Vec<Arc<dyn Guard>>> {
        // TODO: Implement proper guard resolution from DI container
        // For now, return empty list - guards will be added manually or via macros
        Ok(Vec::new())
    }

    fn resolve_interceptors(&self, _tokens: Vec<String>) -> Result<Vec<Arc<dyn Interceptor>>> {
        // TODO: Implement proper interceptor resolution from DI container
        Ok(Vec::new())
    }

    fn resolve_pipes(&self, _tokens: Vec<String>) -> Result<Vec<Arc<dyn Pipe>>> {
        // TODO: Implement proper pipe resolution from DI container
        Ok(Vec::new())
    }

    fn resolve_error_handlers(
        &self,
        _tokens: Vec<String>,
    ) -> Result<Vec<Arc<dyn ErrorHandler>>> {
        // TODO: Implement proper error handler resolution from DI container
        Ok(Vec::new())
    }
}
