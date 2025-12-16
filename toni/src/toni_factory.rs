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

    /// Create a new HTTP application with the specified adapter.
    ///
    /// # Example
    /// ```ignore
    /// use toni::ToniFactory;
    /// use toni_axum::AxumAdapter;
    ///
    /// let app = ToniFactory::create(AppModule, AxumAdapter::new()).await;
    /// app.listen(3000, "127.0.0.1").await;
    /// ```
    pub async fn create<A>(module: impl Into<ModuleDefinition>, adapter: A) -> ToniApplication<A>
    where
        A: HttpAdapter,
    {
        Self::new().create_with(module, adapter).await
    }

    /// Create a new HTTP application with custom factory configuration.
    ///
    /// # Example
    /// ```ignore
    /// use toni::ToniFactory;
    /// use toni_axum::AxumAdapter;
    ///
    /// let mut factory = ToniFactory::new();
    /// factory.use_global_guards(Arc::new(AuthGuard));
    /// let app = factory.create_with(AppModule, AxumAdapter::new()).await;
    /// app.listen(3000, "127.0.0.1").await;
    /// ```
    pub async fn create_with<A>(
        &self,
        module: impl Into<ModuleDefinition>,
        adapter: A,
    ) -> ToniApplication<A>
    where
        A: HttpAdapter,
    {
        let http_adapter = adapter;
        let container = Rc::new(RefCell::new(ToniContainer::new()));

        match self.initialize(module.into(), container.clone()).await {
            Ok(_) => (),
            Err(e) => {
                eprintln!("Critical error during module initialization: {}", e);
                std::process::exit(1);
            }
        };

        let mut app = ToniApplication::new(http_adapter, container);
        match app.init() {
            Ok(_) => (),
            Err(e) => {
                eprintln!("Failed to initialize application: {}", e);
                std::process::exit(1);
            }
        }

        app
    }

    /// Create a standalone DI container without an HTTP server.
    ///
    /// Useful for CLI tools, CRON jobs, and background tasks.
    ///
    /// # Example
    /// ```ignore
    /// use toni::ToniFactory;
    ///
    /// let ctx = ToniFactory::create_application_context(AppModule).await;
    /// let service = ctx.borrow().get::<MyService>().await?;
    /// service.do_work();
    /// ```
    pub async fn create_application_context(
        module: impl Into<ModuleDefinition>,
    ) -> Rc<RefCell<ToniContainer>> {
        Self::new().create_application_context_with(module).await
    }

    /// Create a standalone DI container with custom factory configuration.
    ///
    /// # Example
    /// ```ignore
    /// use toni::ToniFactory;
    ///
    /// let mut factory = ToniFactory::new();
    /// factory.use_global_guards(Arc::new(AuthGuard));
    /// let ctx = factory.create_application_context_with(AppModule).await;
    /// ```
    pub async fn create_application_context_with(
        &self,
        module: impl Into<ModuleDefinition>,
    ) -> Rc<RefCell<ToniContainer>> {
        let container = Rc::new(RefCell::new(ToniContainer::new()));

        match self.initialize(module.into(), container.clone()).await {
            Ok(_) => (),
            Err(e) => {
                eprintln!("Critical error during module initialization: {}", e);
                std::process::exit(1);
            }
        };

        container
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

        // Create instances of all dependencies (providers, controllers)
        ToniInstanceLoader::new(container.clone())
            .create_instances_of_dependencies()
            .await?;

        Ok(())
    }
}
