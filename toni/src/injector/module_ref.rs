use anyhow::Result;
use std::{cell::RefCell, rc::Rc};

use super::{token::IntoToken, ToniContainer};

/// Provides runtime dependency resolution within a module context
///
/// `ModuleRef` is scoped to a specific module and allows dynamic resolution
/// of providers at runtime. It supports both strict (module-only, default) and
/// global (fallback) resolution modes.
///
/// # Examples
///
/// ```ignore
/// #[injectable(pub struct PluginLoader {
///     #[inject]
///     module_ref: ModuleRef,
/// })]
/// impl PluginLoader {
///     pub async fn load_plugin(&self, name: &str) {
///         // Strict mode (default): only search current module
///         let plugin = self.module_ref.get_by_token(name).await?;
///
///         // Global mode: search current module first, then fallback globally
///         let config = self.module_ref.get::<Config>().global().await?;
///     }
/// }
/// ```
pub struct ModuleRef {
    container: Rc<RefCell<ToniContainer>>,
    module_token: String,
}

impl ModuleRef {
    pub(crate) fn new(container: Rc<RefCell<ToniContainer>>, module_token: String) -> Self {
        Self {
            container,
            module_token,
        }
    }

    /// Get a provider instance by its type
    ///
    /// By default, searches only the current module (strict mode).
    /// Use `.global()` to search current module first, then fall back to global search.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Strict (default): only current module
    /// let service = module_ref.get::<MyService>().await?;
    ///
    /// // Global: searches current module, then globally
    /// let shared = module_ref.get::<SharedService>().global().await?;
    /// ```
    pub fn get<T: 'static>(&self) -> ModuleRefQuery<'_, T> {
        ModuleRefQuery {
            module_ref: self,
            token: std::any::type_name::<T>().to_string(),
            strict: true,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Get a provider instance by token
    ///
    /// Accepts any type that implements `IntoToken` (strings, type tokens, etc.).
    /// By default, searches only the current module (strict mode).
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Strict (default): only current module
    /// let api_key: String = module_ref.get_by_token("API_KEY").await?;
    ///
    /// // Global: searches current module, then globally
    /// let key: String = module_ref.get_by_token("SHARED_SECRET").global().await?;
    /// ```
    pub fn get_by_token<T: 'static>(&self, token: impl IntoToken) -> ModuleRefQuery<'_, T> {
        ModuleRefQuery {
            module_ref: self,
            token: token.into_token(),
            strict: true,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Get the current module's token
    pub fn current_module(&self) -> &str {
        &self.module_token
    }
}

/// Builder for ModuleRef queries with strict mode support
pub struct ModuleRefQuery<'a, T: 'static> {
    module_ref: &'a ModuleRef,
    token: String,
    strict: bool,
    _phantom: std::marker::PhantomData<T>,
}

impl<'a, T: 'static> ModuleRefQuery<'a, T> {
    /// Enable global mode: search current module first, then globally
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let service = module_ref.get::<Service>().global().await?;
    /// ```
    pub fn global(mut self) -> Self {
        self.strict = false;
        self
    }

    /// Execute the query and return the provider instance
    pub async fn execute(self) -> Result<T> {
        let provider_instance = {
            let container = self.module_ref.container.borrow();

            if self.strict {
                // Strict mode: only search current module
                container
                    .get_provider_instance_by_token(&self.module_ref.module_token, &self.token)?
                    .cloned()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Provider '{}' not found in module '{}' (strict mode)",
                            self.token,
                            self.module_ref.module_token
                        )
                    })?
            } else {
                // Non-strict mode: search current module first, then global
                // Try current module first
                if let Ok(Some(instance)) =
                    container.get_provider_instance_by_token(&self.module_ref.module_token, &self.token)
                {
                    instance.clone()
                } else {
                    // Fallback to global search
                    let modules = container.get_modules_token();
                    let mut found_instance = None;

                    for module_token in modules {
                        if let Ok(Some(instance)) =
                            container.get_provider_instance_by_token(&module_token, &self.token)
                        {
                            found_instance = Some(instance.clone());
                            break;
                        }
                    }

                    found_instance.ok_or_else(|| {
                        anyhow::anyhow!("Provider '{}' not found in any module", self.token)
                    })?
                }
            }
        };

        // Execute the provider (respects scope: singleton cached, request/transient creates new)
        let instance_any = provider_instance.execute(vec![], None).await;

        instance_any
            .downcast::<T>()
            .map(|boxed| *boxed)
            .map_err(|_| {
                anyhow::anyhow!(
                    "Failed to downcast provider '{}' to requested type",
                    self.token
                )
            })
    }
}

// Implement IntoFuture for ergonomic .await syntax
impl<'a, T: 'static> std::future::IntoFuture for ModuleRefQuery<'a, T> {
    type Output = Result<T>;
    type IntoFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Self::Output> + 'a>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.execute())
    }
}
