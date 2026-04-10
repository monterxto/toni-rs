use std::{
    any::Any,
    sync::{Arc, RwLock},
};


use crate::{
    ProviderScope, async_trait,
    traits_helpers::{Provider, ProviderContext},
};

use super::{ModuleRef, module_ref::ProviderStore};

pub struct ModuleRefProvider {
    module_token: String,
    // Starts empty; populated via inject_provider_store after Phase 1.
    // Shared with every ModuleRef instance this provider ever produces, so
    // writing the full store here makes it visible to all existing clones.
    store: Arc<RwLock<ProviderStore>>,
}

impl ModuleRefProvider {
    pub fn new(module_token: String) -> Self {
        Self {
            module_token,
            store: Arc::new(RwLock::new(ProviderStore::default())),
        }
    }

    pub(crate) fn populate_store(&self, store: Arc<RwLock<ProviderStore>>) {
        // Write the full store into our shared Arc so every ModuleRef that
        // already cloned it (during Phase 1 dependency resolution) sees the data.
        *self
            .store
            .write()
            .expect("ModuleRefProvider store lock poisoned") =
            store.read().expect("provider store lock poisoned").clone();
    }
}

#[async_trait]
impl Provider for ModuleRefProvider {
    async fn execute(
        &self,
        _params: Vec<Box<dyn Any + Send>>,
        _ctx: ProviderContext<'_>,
    ) -> Box<dyn Any + Send> {
        Box::new(ModuleRef::new(self.module_token.clone(), self.store.clone()))
    }

    fn get_token(&self) -> String {
        std::any::type_name::<ModuleRef>().to_string()
    }

    fn get_token_factory(&self) -> String {
        std::any::type_name::<ModuleRef>().to_string()
    }

    fn get_scope(&self) -> ProviderScope {
        ProviderScope::Singleton
    }
}
