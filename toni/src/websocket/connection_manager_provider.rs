//! ConnectionManager DI provider implementation

use std::any::Any;
use std::sync::Arc;

use crate::FxHashMap;
use crate::async_trait;
use crate::http_helpers::HttpRequest;
use crate::provider_scope::ProviderScope;
use crate::traits_helpers::{Provider, ProviderTrait};

use super::ConnectionManager;

/// Provider for ConnectionManager singleton
pub struct ConnectionManagerProvider {
    instance: Option<Arc<ConnectionManager>>,
}

impl ConnectionManagerProvider {
    pub fn new() -> Self {
        Self { instance: None }
    }

    pub fn set_instance(&mut self, instance: Arc<ConnectionManager>) {
        self.instance = Some(instance);
    }

    fn get_instance(&self) -> Arc<ConnectionManager> {
        self.instance
            .clone()
            .expect("ConnectionManager instance not set")
    }
}

impl Default for ConnectionManagerProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ProviderTrait for ConnectionManagerProvider {
    fn get_token(&self) -> String {
        std::any::type_name::<Arc<ConnectionManager>>().to_string()
    }

    async fn execute(
        &self,
        _params: Vec<Box<dyn Any + Send>>,
        _req: Option<&HttpRequest>,
    ) -> Box<dyn Any + Send> {
        Box::new(self.get_instance())
    }

    fn get_token_manager(&self) -> String {
        std::any::type_name::<Arc<ConnectionManager>>().to_string()
    }

    fn get_scope(&self) -> ProviderScope {
        ProviderScope::Singleton
    }
}

impl Clone for ConnectionManagerProvider {
    fn clone(&self) -> Self {
        Self {
            instance: self.instance.clone(),
        }
    }
}

/// Manager that creates ConnectionManager instances for DI
pub struct ConnectionManagerManager;

impl ConnectionManagerManager {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ConnectionManagerManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for ConnectionManagerManager {
    async fn get_all_providers(
        &self,
        _dependencies: &FxHashMap<String, Arc<Box<dyn ProviderTrait>>>,
    ) -> FxHashMap<String, Arc<Box<dyn ProviderTrait>>> {
        let mut providers = FxHashMap::default();

        let manager = Arc::new(ConnectionManager::new());

        let mut provider = ConnectionManagerProvider::new();
        provider.set_instance(manager);

        providers.insert(
            std::any::type_name::<Arc<ConnectionManager>>().to_string(),
            Arc::new(Box::new(provider) as Box<dyn ProviderTrait>),
        );

        providers
    }

    fn get_name(&self) -> String {
        std::any::type_name::<Arc<ConnectionManager>>().to_string()
    }

    fn get_token(&self) -> String {
        std::any::type_name::<Arc<ConnectionManager>>().to_string()
    }

    fn get_dependencies(&self) -> Vec<String> {
        vec![]
    }
}
