//! Standalone application context for non-HTTP scenarios
//!
//! Use this for CLI tools, CRON jobs, background workers, and other
//! scenarios where you need dependency injection without an HTTP server.

use std::{cell::RefCell, rc::Rc};

use anyhow::Result;

use crate::injector::{IntoToken, ToniContainer};

/// Full DI container without an HTTP server
pub struct ToniApplicationContext {
    container: Rc<RefCell<ToniContainer>>,
}

impl ToniApplicationContext {
    pub(crate) fn new(container: Rc<RefCell<ToniContainer>>) -> Self {
        Self { container }
    }

    /// Returns an instance of `T` from the DI container, searching across all modules
    pub async fn get<T: 'static>(&self) -> Result<T> {
        let provider_token = std::any::type_name::<T>().to_string();

        let provider_instance = {
            let container = self.container.borrow();
            let modules = container.get_modules_token();

            let mut found_instance = None;
            for module_token in modules {
                if let Ok(Some(instance)) =
                    container.get_provider_instance_by_token(&module_token, &provider_token)
                {
                    found_instance = Some(instance.clone());
                    break;
                }
            }

            found_instance.ok_or_else(|| {
                anyhow::anyhow!("Provider '{}' not found in any module", provider_token)
            })?
        };

        let instance_any = provider_instance.execute(vec![], None).await;

        instance_any
            .downcast::<T>()
            .map(|boxed| *boxed)
            .map_err(|_| {
                anyhow::anyhow!(
                    "Failed to downcast provider '{}' to requested type",
                    provider_token
                )
            })
    }

    /// Returns an instance of `T` from a specific module's scope in the DI container
    pub async fn get_from<T: 'static>(&self, module_token: &str) -> Result<T> {
        let container = self.container.borrow();
        let provider_token = std::any::type_name::<T>().to_string();

        let provider_instance = container
            .get_provider_instance_by_token(&module_token.to_string(), &provider_token)?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Provider '{}' not found in module '{}'",
                    provider_token,
                    module_token,
                )
            })?;

        let instance_any = provider_instance.execute(vec![], None).await;

        instance_any
            .downcast::<T>()
            .map(|boxed| *boxed)
            .map_err(|_| {
                anyhow::anyhow!(
                    "Failed to downcast provider '{}' to requested type",
                    provider_token
                )
            })
    }

    /// Returns an instance from the DI container by token rather than type; use when providers are registered with a custom token
    pub async fn get_by_token<T: 'static>(&self, token: impl IntoToken) -> Result<T> {
        let token_str = token.into_token();

        let provider_instance = {
            let container = self.container.borrow();
            let modules = container.get_modules_token();

            let mut found_instance = None;
            for module_token in modules {
                if let Ok(Some(instance)) =
                    container.get_provider_instance_by_token(&module_token, &token_str)
                {
                    found_instance = Some(instance.clone());
                    break;
                }
            }

            found_instance.ok_or_else(|| {
                anyhow::anyhow!("Provider '{}' not found in any module", token_str)
            })?
        };

        let instance_any = provider_instance.execute(vec![], None).await;

        instance_any
            .downcast::<T>()
            .map(|boxed| *boxed)
            .map_err(|_| {
                anyhow::anyhow!(
                    "Failed to downcast provider '{}' to requested type",
                    token_str
                )
            })
    }

    /// Returns an instance by token from a specific module's scope in the DI container
    pub async fn get_from_by_token<T: 'static>(
        &self,
        module_token: &str,
        token: impl IntoToken,
    ) -> Result<T> {
        let container = self.container.borrow();
        let token_str = token.into_token();

        let provider_instance = container
            .get_provider_instance_by_token(&module_token.to_string(), &token_str)?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Provider '{}' not found in module '{}'",
                    token_str,
                    module_token,
                )
            })?;

        let instance_any = provider_instance.execute(vec![], None).await;

        instance_any
            .downcast::<T>()
            .map(|boxed| *boxed)
            .map_err(|_| {
                anyhow::anyhow!(
                    "Failed to downcast provider '{}' to requested type",
                    token_str
                )
            })
    }

    pub async fn close(&mut self) -> Result<()> {
        self.call_module_destroy_hooks().await;
        self.call_before_shutdown_hooks(None).await;
        self.call_shutdown_hooks(None).await;
        Ok(())
    }

    async fn call_before_shutdown_hooks(&self, signal: Option<String>) {
        let container = self.container.borrow();
        let modules = container.get_modules_token();

        for module_token in modules.clone() {
            if let Some(module_ref) = container.get_module_by_token(&module_token) {
                let _ = module_ref
                    .get_metadata()
                    .before_application_shutdown(signal.clone(), self.container.clone());
            }
        }

        for module_token in modules {
            if let Ok(providers) = container.get_providers_instance(&module_token) {
                for (_token, provider) in providers.iter() {
                    if provider.get_scope() == crate::ProviderScope::Request {
                        continue;
                    }
                    provider.before_application_shutdown(signal.clone()).await;
                }
            }
        }
    }

    async fn call_module_destroy_hooks(&self) {
        let container = self.container.borrow();
        let modules = container.get_modules_token();

        for module_token in modules.clone() {
            if let Some(module_ref) = container.get_module_by_token(&module_token) {
                let _ = module_ref
                    .get_metadata()
                    .on_module_destroy(self.container.clone());
            }
        }

        for module_token in modules {
            if let Ok(providers) = container.get_providers_instance(&module_token) {
                for (_token, provider) in providers.iter() {
                    if provider.get_scope() == crate::ProviderScope::Request {
                        continue;
                    }
                    provider.on_module_destroy().await;
                }
            }
        }
    }

    async fn call_shutdown_hooks(&self, signal: Option<String>) {
        let container = self.container.borrow();
        let modules = container.get_modules_token();

        for module_token in modules.clone() {
            if let Some(module_ref) = container.get_module_by_token(&module_token) {
                let _ = module_ref
                    .get_metadata()
                    .on_application_shutdown(signal.clone(), self.container.clone());
            }
        }

        for module_token in modules {
            if let Ok(providers) = container.get_providers_instance(&module_token) {
                for (_token, provider) in providers.iter() {
                    if provider.get_scope() == crate::ProviderScope::Request {
                        continue;
                    }
                    provider.on_application_shutdown(signal.clone()).await;
                }
            }
        }
    }
}
