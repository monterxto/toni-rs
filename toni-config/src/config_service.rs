//! ConfigService - Injectable service for accessing configuration in providers

use crate::Config;
use std::any::Any;
use std::sync::Arc;
use toni::async_trait;
use toni::http_helpers::HttpRequest;
use toni::traits_helpers::{Provider, ProviderFactory};
use toni::FxHashMap;

/// Service that provides access to configuration
///
/// This service is automatically registered when you import `ConfigModule<T>` in your module.
/// Inject it into your providers to access configuration:
///
/// ```rust,ignore
/// #[injectable(
///     pub struct DatabaseService {
///         config: ConfigService<AppConfig>
///     }
/// )]
/// impl DatabaseService {
///     pub fn connect(&self) -> String {
///         let cfg = self.config.get();
///         format!("Connecting to {}", cfg.database_url)
///     }
/// }
/// ```
#[derive(Clone)]
pub struct ConfigService<T: Config> {
    config: Arc<T>,
}

impl<T: Config + Clone + 'static> ConfigService<T> {
    /// Create a new ConfigService instance with the given config
    ///
    /// This is typically handled by the DI system.
    pub fn new(config: Arc<T>) -> Self {
        Self { config }
    }

    /// Get the loaded configuration (clones the config)
    ///
    /// # Usage Notes
    ///
    /// - When using `.get()`, always use a type annotation: `let cfg: AppConfig = self.config.get()`
    /// - Do NOT directly return `.get()` result - use a let binding first:
    ///   ```rust,ignore
    ///   // ❌ BAD - will cause lifetime errors:
    ///   pub fn get_config(&self) -> AppConfig {
    ///       self.config.get()
    ///   }
    ///
    ///   // ✅ GOOD - use type annotation and intermediate binding:
    ///   pub fn get_config(&self) -> AppConfig {
    ///       let cfg: AppConfig = self.config.get();
    ///       cfg
    ///   }
    ///   ```
    /// - Prefer using `.get_ref()` for zero-copy access when possible.
    pub fn get(&self) -> T {
        (*self.config).clone()
    }

    /// Get a reference to the configuration (zero-copy)
    pub fn get_ref(&self) -> &T {
        &self.config
    }
}

// ============================================================================
// Provider Implementation - Enables ConfigService as Injectable Dependency
// ============================================================================

/// Implement Provider so ConfigService can be injected as a dependency
#[async_trait]
impl<T: Config> Provider for ConfigService<T> {
    async fn execute(
        &self,
        _params: Vec<Box<dyn Any + Send>>,
        _req: Option<&HttpRequest>,
    ) -> Box<dyn Any + Send> {
        // Return a clone of self for injection
        Box::new(self.clone())
    }

    fn get_token(&self) -> String {
        // Return full generic type to support multiple configs of same service
        format!("ConfigService<{}>", std::any::type_name::<T>())
    }

    fn get_token_factory(&self) -> String {
        format!("ConfigService<{}>", std::any::type_name::<T>())
    }
}

// ============================================================================
// ProviderFactory Implementation for DI System
// ============================================================================

/// `ProviderFactory` for `ConfigService` — registered with the DI system by `ConfigModule`.
pub struct ConfigServiceFactory<T: Config> {
    config: Arc<T>,
}

impl<T: Config> ConfigServiceFactory<T> {
    pub fn with_config(config: Arc<T>) -> Self {
        Self { config }
    }
}

#[async_trait]
impl<T: Config + Clone + Send + Sync + 'static> ProviderFactory for ConfigServiceFactory<T> {
    fn get_token(&self) -> String {
        format!("ConfigService<{}>", std::any::type_name::<T>())
    }

    async fn build(
        &self,
        _deps: FxHashMap<String, Arc<Box<dyn Provider>>>,
    ) -> Arc<Box<dyn Provider>> {
        Arc::new(Box::new(ConfigService {
            config: self.config.clone(),
        }) as Box<dyn Provider>)
    }
}
