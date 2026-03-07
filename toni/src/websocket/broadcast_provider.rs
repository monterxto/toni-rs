use std::any::Any;
use std::sync::Arc;

use crate::FxHashMap;
use crate::async_trait;
use crate::http_helpers::HttpRequest;
use crate::provider_scope::ProviderScope;
use crate::traits_helpers::{Provider, ProviderTrait};

use super::BroadcastService;

/// Singleton provider that hands out clones of the pre-built `BroadcastService`.
pub(crate) struct BroadcastServiceProvider {
    instance: BroadcastService,
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
        Box::new(self.instance.clone())
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

/// Framework-internal manager that creates the `BroadcastService` singleton during
/// DI initialisation. Registered by `BuiltinModule` so no user module is needed.
pub(crate) struct BroadcastServiceManager;

#[async_trait]
impl Provider for BroadcastServiceManager {
    async fn get_all_providers(
        &self,
        _dependencies: &FxHashMap<String, Arc<Box<dyn ProviderTrait>>>,
    ) -> FxHashMap<String, Arc<Box<dyn ProviderTrait>>> {
        let mut providers = FxHashMap::default();

        let service = BroadcastService::new();

        providers.insert(
            std::any::type_name::<BroadcastService>().to_string(),
            Arc::new(
                Box::new(BroadcastServiceProvider { instance: service }) as Box<dyn ProviderTrait>
            ),
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
        vec![]
    }
}
