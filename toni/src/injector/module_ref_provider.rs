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
    store: Arc<RwLock<ProviderStore>>,
}

impl ModuleRefProvider {
    pub fn new(module_token: String, store: Arc<RwLock<ProviderStore>>) -> Self {
        Self { module_token, store }
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
