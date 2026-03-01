//! Broadcast module for dependency injection
//!
//! Provides a module that registers BroadcastService and ConnectionManager
//! as injectable providers, similar to ConfigModule pattern.
//!
//! WebSocket works without this module — import it only when you need broadcasting.

use std::sync::Arc;

use crate::module_helpers::module_enum::ModuleDefinition;
use crate::traits_helpers::{Controller, ModuleMetadata, Provider};

use super::{
    BroadcastService, BroadcastServiceManager, ConnectionManager, ConnectionManagerManager,
};

/// Module that provides `ConnectionManager` and `BroadcastService` for broadcasting.
///
/// WebSocket gateways work without this module — import it only when you need
/// `BroadcastService` to send messages to other clients or rooms.
///
/// When imported, `ToniApplication` automatically uses the broadcast-aware
/// connection lifecycle instead of the simple echo path.
///
/// # Example
/// ```ignore
/// #[module(
///     providers: [ChatGateway],
///     imports: [BroadcastModule::new()]
/// )]
/// struct AppModule;
/// ```
pub struct BroadcastModule;

impl BroadcastModule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BroadcastModule {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleMetadata for BroadcastModule {
    fn get_id(&self) -> String {
        "BroadcastModule".to_string()
    }

    fn get_name(&self) -> String {
        "BroadcastModule".to_string()
    }

    fn is_global(&self) -> bool {
        true
    }

    fn imports(&self) -> Option<Vec<Box<dyn ModuleMetadata>>> {
        None
    }

    fn controllers(&self) -> Option<Vec<Box<dyn Controller>>> {
        None
    }

    fn providers(&self) -> Option<Vec<Box<dyn Provider>>> {
        Some(vec![
            Box::new(ConnectionManagerManager::new()),
            Box::new(BroadcastServiceManager::new()),
        ])
    }

    fn exports(&self) -> Option<Vec<String>> {
        Some(vec![
            std::any::type_name::<Arc<ConnectionManager>>().to_string(),
            std::any::type_name::<BroadcastService>().to_string(),
        ])
    }
}

impl From<BroadcastModule> for ModuleDefinition {
    fn from(module: BroadcastModule) -> Self {
        ModuleDefinition::DefaultModule(Box::new(module))
    }
}

impl Clone for BroadcastModule {
    fn clone(&self) -> Self {
        Self
    }
}
