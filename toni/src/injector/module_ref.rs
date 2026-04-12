use std::sync::{Arc, RwLock};

use anyhow::Result;
use rustc_hash::FxHashMap;

use super::token::IntoToken;
use crate::traits_helpers::Provider;

pub type ProviderStore = FxHashMap<String, FxHashMap<String, Arc<Box<dyn Provider>>>>;

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
#[derive(Clone)]
pub struct ModuleRef {
    module_token: String,
    store: Arc<RwLock<ProviderStore>>,
}

impl ModuleRef {
    pub(crate) fn new(module_token: String, store: Arc<RwLock<ProviderStore>>) -> Self {
        Self {
            module_token,
            store,
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
    pub async fn execute(self) -> Result<T>
    where
        T: Send,
    {
        let provider_instance = {
            let store = self
                .module_ref
                .store
                .read()
                .map_err(|_| anyhow::anyhow!("Provider store lock poisoned"))?;

            if self.strict {
                store
                    .get(&self.module_ref.module_token)
                    .and_then(|m| m.get(&self.token))
                    .cloned()
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "Provider '{}' not found in module '{}' (strict mode)",
                            self.token,
                            self.module_ref.module_token
                        )
                    })?
            } else {
                // Try current module first, then any module
                let local = store
                    .get(&self.module_ref.module_token)
                    .and_then(|m| m.get(&self.token))
                    .cloned();

                if let Some(instance) = local {
                    instance
                } else {
                    store
                        .values()
                        .find_map(|providers| providers.get(&self.token).cloned())
                        .ok_or_else(|| {
                            anyhow::anyhow!("Provider '{}' not found in any module", self.token)
                        })?
                }
            }
        };

        provider_instance
            .execute(vec![], crate::traits_helpers::ProviderContext::None)
            .await
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
impl<'a, T: 'static + Send> std::future::IntoFuture for ModuleRefQuery<'a, T> {
    type Output = Result<T>;
    type IntoFuture =
        std::pin::Pin<Box<dyn std::future::Future<Output = Self::Output> + Send + 'a>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(self.execute())
    }
}
