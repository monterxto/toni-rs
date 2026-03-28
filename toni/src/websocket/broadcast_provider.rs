use std::any::Any;
use std::sync::Arc;

use crate::FxHashMap;
use crate::async_trait;
use crate::http_helpers::HttpRequest;
use crate::provider_scope::ProviderScope;
use crate::traits_helpers::{Provider, ProviderFactory};

use super::BroadcastService;

/// Singleton provider that hands out clones of the pre-built `BroadcastService`.
pub(crate) struct BroadcastServiceProvider {
    instance: BroadcastService,
}

#[async_trait]
impl Provider for BroadcastServiceProvider {
    fn get_token(&self) -> String {
        std::any::type_name::<BroadcastService>().to_string()
    }

    async fn execute(
        &self,
        _params: Vec<Box<dyn Any + Send>>,
        _req: Option<&crate::http_helpers::RequestPart>,
    ) -> Box<dyn Any + Send> {
        Box::new(self.instance.clone())
    }

    fn get_token_factory(&self) -> String {
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

pub(crate) struct BroadcastServiceManager;

#[async_trait]
impl ProviderFactory for BroadcastServiceManager {
    fn get_token(&self) -> String {
        std::any::type_name::<BroadcastService>().to_string()
    }

    async fn build(
        &self,
        _deps: FxHashMap<String, Arc<Box<dyn Provider>>>,
    ) -> Arc<Box<dyn Provider>> {
        Arc::new(Box::new(BroadcastServiceProvider {
            instance: BroadcastService::new(),
        }) as Box<dyn Provider>)
    }
}
