use std::{any::Any, cell::RefCell, rc::Rc};

use crate::{
    ProviderScope, async_trait,
    traits_helpers::{Provider, ProviderContext},
};

use super::{ModuleRef, ToniContainer};

/// Thread-local storage for the DI container
/// This is set by the instance loader before provider instantiation
thread_local! {
    static CONTAINER_CONTEXT: RefCell<Option<Rc<RefCell<ToniContainer>>>> = const { RefCell::new(None) };
}

/// Set the container context for the current thread
pub(crate) fn set_container_context(container: Rc<RefCell<ToniContainer>>) {
    CONTAINER_CONTEXT.with(|ctx| {
        *ctx.borrow_mut() = Some(container);
    });
}

/// Access the container context with a closure
pub(crate) fn with_container<F, R>(f: F) -> anyhow::Result<R>
where
    F: FnOnce(&Rc<RefCell<ToniContainer>>) -> anyhow::Result<R>,
{
    CONTAINER_CONTEXT.with(|ctx| {
        let container_opt = ctx.borrow();
        let container = container_opt.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Container context not set - this is a framework bug")
        })?;
        f(container)
    })
}

/// Provider wrapper for ModuleRef
///
/// This provider stores the module_token and creates ModuleRef instances
pub struct ModuleRefProvider {
    module_token: String,
}

impl ModuleRefProvider {
    pub fn new(module_token: String) -> Self {
        Self { module_token }
    }
}

#[async_trait]
impl Provider for ModuleRefProvider {
    async fn execute(
        &self,
        _params: Vec<Box<dyn Any + Send>>,
        _ctx: ProviderContext<'_>,
    ) -> Box<dyn Any + Send> {
        // Create ModuleRef with just the module token
        // The container will be accessed via thread-local when needed
        let module_ref = ModuleRef::new(self.module_token.clone());

        Box::new(module_ref)
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
