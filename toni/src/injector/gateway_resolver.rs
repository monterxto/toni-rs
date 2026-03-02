use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::Result;

use crate::traits_helpers::{ErrorHandler, Guard, Interceptor, Pipe};
use crate::websocket::{GatewayTrait, GatewayWrapper};

use super::ToniContainer;

pub struct GatewayResolver {
    container: Rc<RefCell<ToniContainer>>,
}

impl GatewayResolver {
    pub fn new(container: Rc<RefCell<ToniContainer>>) -> Self {
        Self { container }
    }

    pub fn resolve(&self) -> Result<HashMap<String, Arc<GatewayWrapper>>> {
        let raw = self.container.borrow().get_gateways().clone();
        raw.into_iter()
            .map(|(path, gateway)| {
                let wrapper = self.wrap_gateway(gateway)?;
                Ok((path, Arc::new(wrapper)))
            })
            .collect()
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
        let global_guards = self.container.borrow().get_global_enhancers().guards;
        guards.extend(global_guards);
        for token in tokens {
            if let Some(guard) = self.resolve_guard_by_token(&token)? {
                guards.push(guard);
            }
        }
        Ok(guards)
    }

    fn resolve_interceptors(&self, tokens: Vec<String>) -> Result<Vec<Arc<dyn Interceptor>>> {
        let mut interceptors = Vec::new();
        let global_interceptors = self.container.borrow().get_global_enhancers().interceptors;
        interceptors.extend(global_interceptors);
        for token in tokens {
            if let Some(i) = self.resolve_interceptor_by_token(&token)? {
                interceptors.push(i);
            }
        }
        Ok(interceptors)
    }

    fn resolve_pipes(&self, tokens: Vec<String>) -> Result<Vec<Arc<dyn Pipe>>> {
        let mut pipes = Vec::new();
        let global_pipes = self.container.borrow().get_global_enhancers().pipes;
        pipes.extend(global_pipes);
        for token in tokens {
            if let Some(pipe) = self.resolve_pipe_by_token(&token)? {
                pipes.push(pipe);
            }
        }
        Ok(pipes)
    }

    fn resolve_error_handlers(&self, tokens: Vec<String>) -> Result<Vec<Arc<dyn ErrorHandler>>> {
        let mut error_handlers = Vec::new();
        let global_error_handlers = self.container.borrow().get_global_enhancers().error_handlers;
        error_handlers.extend(global_error_handlers);
        for token in tokens {
            if let Some(eh) = self.resolve_error_handler_by_token(&token)? {
                error_handlers.push(eh);
            }
        }
        Ok(error_handlers)
    }

    fn resolve_guard_by_token(&self, token: &str) -> Result<Option<Arc<dyn Guard>>> {
        let container = self.container.borrow();
        for module_token in container.get_modules_token() {
            let providers = container.get_providers_instance(&module_token)?;
            if let Some(p) = providers.get(token) {
                if let Some(g) = p.as_guard() { return Ok(Some(g)); }
            }
        }
        Ok(None)
    }

    fn resolve_interceptor_by_token(&self, token: &str) -> Result<Option<Arc<dyn Interceptor>>> {
        let container = self.container.borrow();
        for module_token in container.get_modules_token() {
            let providers = container.get_providers_instance(&module_token)?;
            if let Some(p) = providers.get(token) {
                if let Some(i) = p.as_interceptor() { return Ok(Some(i)); }
            }
        }
        Ok(None)
    }

    fn resolve_pipe_by_token(&self, token: &str) -> Result<Option<Arc<dyn Pipe>>> {
        let container = self.container.borrow();
        for module_token in container.get_modules_token() {
            let providers = container.get_providers_instance(&module_token)?;
            if let Some(p) = providers.get(token) {
                if let Some(pipe) = p.as_pipe() { return Ok(Some(pipe)); }
            }
        }
        Ok(None)
    }

    fn resolve_error_handler_by_token(&self, token: &str) -> Result<Option<Arc<dyn ErrorHandler>>> {
        let container = self.container.borrow();
        for module_token in container.get_modules_token() {
            let providers = container.get_providers_instance(&module_token)?;
            if let Some(p) = providers.get(token) {
                if let Some(eh) = p.as_error_handler() { return Ok(Some(eh)); }
            }
        }
        Ok(None)
    }
}
