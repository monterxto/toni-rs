use std::{any::Any, sync::Arc};

use async_trait::async_trait;

use crate::{
    ProviderScope,
    traits_helpers::{Provider, ProviderContext},
};

/// Holds all contributions for a given multi-provider base token.
///
/// Each item is stored as `Arc<Arc<dyn Trait + Send + Sync>>` erased to
/// `Arc<dyn Any + Send + Sync>` (the "double-Arc" pattern). The injection-site
/// codegen downcasts back to `Arc<Arc<dyn Trait + Send + Sync>>` and clones
/// the inner Arc to produce `Vec<Arc<dyn Trait + Send + Sync>>`.
pub(super) struct MultiCollectionProvider {
    pub token: String,
    pub items: Vec<Arc<dyn Any + Send + Sync>>,
}

#[async_trait]
impl Provider for MultiCollectionProvider {
    fn get_token(&self) -> String {
        self.token.clone()
    }

    fn get_token_factory(&self) -> String {
        self.token.clone()
    }

    fn get_scope(&self) -> ProviderScope {
        ProviderScope::Singleton
    }

    async fn execute(
        &self,
        _params: Vec<Box<dyn Any + Send>>,
        _ctx: ProviderContext<'_>,
    ) -> Box<dyn Any + Send> {
        Box::new(self.items.clone())
    }
}
