//! Broadcast provider managers for dependency injection

use std::any::Any;

use crate::FxHashMap;
use crate::async_trait;
use crate::http_helpers::HttpRequest;
use crate::provider_scope::ProviderScope;
use crate::traits_helpers::{Provider, ProviderTrait};

use super::{BroadcastService, ConnectionManager};
use std::sync::Arc;

/// Provider for BroadcastService
pub struct BroadcastServiceProvider {
    instance: Option<BroadcastService>,
}

impl BroadcastServiceProvider {
    pub fn new() -> Self {
        Self { instance: None }
    }

    pub fn set_instance(&mut self, service: BroadcastService) {
        self.instance = Some(service);
    }

    fn get_instance(&self) -> BroadcastService {
        self.instance
            .clone()
            .expect("BroadcastService instance not set")
    }
}

impl Default for BroadcastServiceProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProviderTrait for BroadcastServiceProvider {
    fn get_token(&self) -> String {
        std::any::type_name::<BroadcastService>().to_string()
    }

    async fn execute(
        &self,
        _params: Vec<Box<dyn Any + Send>>,
        _req: Option<&HttpRequest>,
    ) -> Box<dyn Any + Send> {
        Box::new(self.get_instance())
    }

    fn get_token_manager(&self) -> String {
        std::any::type_name::<BroadcastService>().to_string()
    }

    fn get_scope(&self) -> ProviderScope {
        ProviderScope::Singleton
    }
}

impl Clone for BroadcastServiceProvider {
    fn clone(&self) -> Self {
        Self {
            instance: self.instance.clone(),
        }
    }
}

/// Manager that provides BroadcastService instances to the DI container
pub struct BroadcastServiceManager;

impl BroadcastServiceManager {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BroadcastServiceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for BroadcastServiceManager {
    async fn get_all_providers(
        &self,
        dependencies: &FxHashMap<String, Arc<Box<dyn ProviderTrait>>>,
    ) -> FxHashMap<String, Arc<Box<dyn ProviderTrait>>> {
        let mut providers = FxHashMap::default();

        let manager = dependencies
            .get(std::any::type_name::<Arc<ConnectionManager>>())
            .expect("ConnectionManager not found in dependencies")
            .execute(vec![], None)
            .await;

        let manager = manager
            .downcast::<Arc<ConnectionManager>>()
            .expect("Failed to downcast ConnectionManager");

        let broadcast = BroadcastService::new(*manager);

        let mut provider = BroadcastServiceProvider::new();
        provider.set_instance(broadcast);

        providers.insert(
            std::any::type_name::<BroadcastService>().to_string(),
            Arc::new(Box::new(provider) as Box<dyn ProviderTrait>),
        );

        providers
    }

    fn get_name(&self) -> String {
        std::any::type_name::<BroadcastService>().to_string()
    }

    fn get_token(&self) -> String {
        std::any::type_name::<BroadcastService>().to_string()
    }

    fn get_dependencies(&self) -> Vec<String> {
        vec![std::any::type_name::<Arc<ConnectionManager>>().to_string()]
    }
}
