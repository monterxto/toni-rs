//! Built-in Global Module
//!
//! This module provides built-in providers that should be globally available
//! to all modules without requiring explicit imports.

use crate::RequestManager;
use crate::module_helpers::module_enum::ModuleDefinition;
use crate::traits_helpers::{ControllerFactory, ModuleMetadata, ProviderFactory};

/// Built-in global module that provides core framework functionality
///
/// Currently provides:
/// - Request: HTTP request data access for handlers
pub struct BuiltinModule;

impl ModuleMetadata for BuiltinModule {
    fn get_id(&self) -> String {
        "ToniBuiltinModule".to_string()
    }

    fn get_name(&self) -> String {
        "ToniBuiltinModule".to_string()
    }

    fn is_global(&self) -> bool {
        true // Global module - exports are available everywhere
    }

    fn imports(&self) -> Option<Vec<Box<dyn ModuleMetadata>>> {
        None
    }

    fn controllers(&self) -> Option<Vec<Box<dyn ControllerFactory>>> {
        None
    }

    fn providers(&self) -> Option<Vec<Box<dyn ProviderFactory>>> {
        Some(vec![Box::new(RequestManager)])
    }

    fn exports(&self) -> Option<Vec<String>> {
        Some(vec![
            std::any::type_name::<crate::request::Request>().to_string(),
        ])
    }
}

impl From<BuiltinModule> for ModuleDefinition {
    fn from(module: BuiltinModule) -> Self {
        ModuleDefinition::DefaultModule(Box::new(module))
    }
}
