use parking_lot::Mutex;

use crate::traits_helpers::{ControllerFactory, ModuleMetadata, ProviderFactory};

/// A module whose providers and exports are determined at runtime rather than compile time.
///
/// Integration crates (e.g. `toni-seaorm`) use this to implement `forRoot`/`forFeature`-style
/// factory functions without having to implement all of `ModuleMetadata` manually.
///
/// # Example
/// ```ignore
/// pub struct SeaOrmModule;
///
/// impl SeaOrmModule {
///     pub fn for_root(database_url: &str) -> DynamicModule {
///         DynamicModule::builder("SeaOrmModule")
///             .provider(SeaOrmConnectionFactory::new(database_url))
///             .export::<DatabaseConnection>()
///             .build()
///     }
/// }
/// ```
///
/// Then in the application module:
/// ```ignore
/// #[module(imports: [SeaOrmModule::for_root(DATABASE_URL)])]
/// pub struct AppModule;
/// ```
pub struct DynamicModule {
    id: String,
    // Wrapped in Mutex<Option<...>> so ownership can be moved out on the first call to
    // providers(), which takes &self. The scanner calls providers() exactly once per module
    // during scan_modules_for_dependencies, so draining on first call is safe.
    providers: Mutex<Option<Vec<Box<dyn ProviderFactory>>>>,
    exports: Vec<String>,
    global: bool,
}

impl ModuleMetadata for DynamicModule {
    fn get_id(&self) -> String {
        self.id.clone()
    }

    fn get_name(&self) -> String {
        self.id.clone()
    }

    fn is_global(&self) -> bool {
        self.global
    }

    fn imports(&self) -> Option<Vec<Box<dyn ModuleMetadata>>> {
        None
    }

    fn controllers(&self) -> Option<Vec<Box<dyn ControllerFactory>>> {
        None
    }

    fn providers(&self) -> Option<Vec<Box<dyn ProviderFactory>>> {
        self.providers.lock().take()
    }

    fn exports(&self) -> Option<Vec<String>> {
        Some(self.exports.clone())
    }
}

pub struct DynamicModuleBuilder {
    id: String,
    providers: Vec<Box<dyn ProviderFactory>>,
    exports: Vec<String>,
    global: bool,
}

impl DynamicModuleBuilder {
    pub fn provider<F: ProviderFactory + 'static>(mut self, factory: F) -> Self {
        self.providers.push(Box::new(factory));
        self
    }

    /// Export a provider by its Rust type. Uses `std::any::type_name::<T>()` as the token,
    /// which matches how `#[injectable]`-generated factories register their tokens.
    pub fn export<T: 'static>(mut self) -> Self {
        self.exports.push(std::any::type_name::<T>().to_string());
        self
    }

    /// Export a provider by an explicit string token (for `provide!`-style value providers).
    pub fn export_token(mut self, token: impl Into<String>) -> Self {
        self.exports.push(token.into());
        self
    }

    /// Make this module global so its exports are available to every module without importing.
    pub fn global(mut self) -> Self {
        self.global = true;
        self
    }

    pub fn build(self) -> DynamicModule {
        DynamicModule {
            id: self.id,
            providers: Mutex::new(Some(self.providers)),
            exports: self.exports,
            global: self.global,
        }
    }
}

impl DynamicModule {
    pub fn builder(id: impl Into<String>) -> DynamicModuleBuilder {
        DynamicModuleBuilder {
            id: id.into(),
            providers: Vec::new(),
            exports: Vec::new(),
            global: false,
        }
    }
}
