use crate::module_helpers::module_enum::ModuleDefinition;
use crate::traits_helpers::{ControllerFactory, ModuleMetadata, ProviderFactory};
use crate::websocket::BroadcastService;
use crate::websocket::broadcast_provider::BroadcastServiceManager;

/// Opt-in module that provides `BroadcastService` for WebSocket broadcasting.
///
/// Import this in any module whose gateways need `BroadcastService`. Because the
/// module is global, any module that transitively imports it can inject the service
/// without re-exporting it.
///
/// # Example
///
/// ```rust,ignore
/// #[module(
///     imports: [BroadcastModule::new()],
///     providers: [ChatGateway],
/// )]
/// struct AppModule;
/// ```
pub struct BroadcastModule;

impl BroadcastModule {
    pub fn new() -> Self {
        Self
    }
}

impl ModuleMetadata for BroadcastModule {
    fn get_id(&self) -> String {
        "ToniBroadcastModule".to_string()
    }

    fn get_name(&self) -> String {
        "ToniBroadcastModule".to_string()
    }

    fn is_global(&self) -> bool {
        true
    }

    fn imports(&self) -> Option<Vec<Box<dyn ModuleMetadata>>> {
        None
    }

    fn controllers(&self) -> Option<Vec<Box<dyn ControllerFactory>>> {
        None
    }

    fn providers(&self) -> Option<Vec<Box<dyn ProviderFactory>>> {
        Some(vec![Box::new(BroadcastServiceManager)])
    }

    fn exports(&self) -> Option<Vec<String>> {
        Some(vec![std::any::type_name::<BroadcastService>().to_string()])
    }
}

impl From<BroadcastModule> for ModuleDefinition {
    fn from(module: BroadcastModule) -> Self {
        ModuleDefinition::DefaultModule(Box::new(module))
    }
}
