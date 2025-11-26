use std::{cell::RefCell, rc::Rc};

use anyhow::Result;

use crate::{
    http_adapter::HttpAdapter,
    injector::{IntoToken, ToniContainer},
    router::RoutesResolver,
};

pub struct ToniApplication<H: HttpAdapter> {
    http_adapter: H,
    routes_resolver: RoutesResolver,
}

impl<H: HttpAdapter> ToniApplication<H> {
    pub fn new(http_adapter: H, container: Rc<RefCell<ToniContainer>>) -> Self {
        Self {
            http_adapter,
            routes_resolver: RoutesResolver::new(container.clone()),
        }
    }

    pub fn init(&mut self) -> Result<()> {
        self.routes_resolver.resolve(&mut self.http_adapter)?;
        Ok(())
    }

    /// Get a provider instance from the DI container by its type (searches all modules)
    ///
    /// # Example
    /// ```ignore
    /// // Get by type
    /// let my_service = app.get::<MyService>().await?;
    ///
    /// // Get with strict mode (only search specific module)
    /// let my_service = app.get_from::<MyService>("ModuleName").await?;
    /// ```
    pub async fn get<T: 'static>(&self) -> Result<T> {
        let provider_token = std::any::type_name::<T>().to_string();

        // Search all modules for the provider
        let provider_instance = {
            let container = self.routes_resolver.container.borrow();
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

        // Execute the provider to get the instance
        let instance_any = provider_instance.execute(vec![], None).await;

        // Downcast to the requested type
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

    /// Get a provider instance from a specific module
    ///
    /// # Example
    /// ```ignore
    /// let my_service = app.get_from::<MyService>("ModuleName").await?;
    /// ```
    pub async fn get_from<T: 'static>(&self, module_token: &str) -> Result<T> {
        let container = self.routes_resolver.container.borrow();
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

        // Execute the provider to get the instance
        let instance_any = provider_instance.execute(vec![], None).await;

        // Downcast to the requested type
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

    /// Get a provider instance by token (searches all modules)
    ///
    /// # Example
    /// ```ignore
    /// let api_key: String = app.get_by_token("API_KEY").await?;
    /// ```
    pub async fn get_by_token<T: 'static>(&self, token: impl IntoToken) -> Result<T> {
        let token_str = token.into_token();

        // Search all modules for the provider
        let provider_instance = {
            let container = self.routes_resolver.container.borrow();
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

        // Execute the provider to get the instance
        let instance_any = provider_instance.execute(vec![], None).await;

        // Downcast to the requested type
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

    /// Get a provider instance by token from a specific module
    ///
    /// # Example
    /// ```ignore
    /// let config: Config = app.get_from_by_token("ConfigModule", "APP_CONFIG").await?;
    /// ```
    pub async fn get_from_by_token<T: 'static>(
        &self,
        module_token: &str,
        token: impl IntoToken,
    ) -> Result<T> {
        let container = self.routes_resolver.container.borrow();
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

        // Execute the provider to get the instance
        let instance_any = provider_instance.execute(vec![], None).await;

        // Downcast to the requested type
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

    pub async fn listen(self, port: u16, hostname: &str) {
        if let Err(e) = self.http_adapter.listen(port, hostname).await {
            eprintln!("🚨 Failed to start server: {}", e);
            std::process::exit(1);
        }
    }
}
