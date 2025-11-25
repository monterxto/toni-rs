use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::Result;

use crate::middleware::Middleware;
use crate::module_helpers::module_enum::ModuleDefinition;
use crate::toni_application::ToniApplication;
use crate::traits_helpers::{Guard, Interceptor, Pipe};
use crate::{
    http_adapter::HttpAdapter,
    injector::{ToniContainer, ToniInstanceLoader},
    scanner::ToniDependenciesScanner,
};

#[derive(Default)]
pub struct ToniFactory {
    global_middleware: Vec<Arc<dyn Middleware>>,
    global_guards: Vec<Arc<dyn Guard>>,
    global_interceptors: Vec<Arc<dyn Interceptor>>,
    global_pipes: Vec<Arc<dyn Pipe>>,
}

impl ToniFactory {
    #[inline]
    pub fn new() -> Self {
        Self {
            global_middleware: Vec::new(),
            global_guards: Vec::new(),
            global_interceptors: Vec::new(),
            global_pipes: Vec::new(),
        }
    }

    pub fn use_global_middleware(&mut self, middleware: Arc<dyn Middleware>) -> &mut Self {
        self.global_middleware.push(middleware);
        self
    }

    pub fn use_global_guards(&mut self, guard: Arc<dyn Guard>) -> &mut Self {
        self.global_guards.push(guard);
        self
    }

    pub fn use_global_interceptors(&mut self, interceptor: Arc<dyn Interceptor>) -> &mut Self {
        self.global_interceptors.push(interceptor);
        self
    }

    pub fn use_global_pipes(&mut self, pipe: Arc<dyn Pipe>) -> &mut Self {
        self.global_pipes.push(pipe);
        self
    }

    pub async fn create(
        &self,
        module: ModuleDefinition,
        http_adapter: impl HttpAdapter,
    ) -> ToniApplication<impl HttpAdapter> {
        let container = Rc::new(RefCell::new(ToniContainer::new()));

        match self.initialize(module, container.clone()).await {
            Ok(_) => (),
            Err(e) => {
                eprintln!("Falha crítica na inicialização do módulo: {}", e);
                std::process::exit(1);
            }
        };

        let mut app = ToniApplication::new(http_adapter, container);
        match app.init() {
            Ok(_) => (),
            Err(e) => {
                eprintln!("Falha na inicialização da aplicação: {}", e);
                std::process::exit(1);
            }
        }

        app
    }

    async fn initialize(
        &self,
        module: ModuleDefinition,
        container: Rc<RefCell<ToniContainer>>,
    ) -> Result<()> {
        let mut scanner = ToniDependenciesScanner::new(container.clone());

        // Register built-in global module
        scanner.scan(crate::builtin_module::BuiltinModule.into())?;

        // Scan user's root module
        scanner.scan(module)?;

        // Register global middleware
        {
            let mut container_mut = container.borrow_mut();
            if let Some(middleware_manager) = container_mut.get_middleware_manager_mut() {
                for middleware in &self.global_middleware {
                    middleware_manager.add_global(middleware.clone());
                }
            }
        }

        // Register global enhancers
        {
            let mut container_mut = container.borrow_mut();
            for guard in &self.global_guards {
                container_mut.add_global_guard(guard.clone());
            }
            for interceptor in &self.global_interceptors {
                container_mut.add_global_interceptor(interceptor.clone());
            }
            for pipe in &self.global_pipes {
                container_mut.add_global_pipe(pipe.clone());
            }
        }

        scanner.scan_middleware()?;

        ToniInstanceLoader::new(container.clone())
            .create_instances_of_dependencies()
            .await?;

        // Resolve middleware tokens from DI container
        // This happens AFTER DI container is built, allowing middleware to have injected dependencies
        {
            let modules_order = container.borrow().get_ordered_modules_token();
            for module_token in &modules_order {
                // Get providers for this module
                let providers = container
                    .borrow()
                    .get_providers_instance(module_token)?
                    .clone();

                // Resolve middleware tokens to instances
                let mut container_mut = container.borrow_mut();
                if let Some(middleware_manager) = container_mut.get_middleware_manager_mut() {
                    middleware_manager.resolve_middleware_tokens(module_token, &providers)?;
                }
            }
        }

        Ok(())
    }
}
