use anyhow::{Result, anyhow};
use rustc_hash::FxHashMap;
use std::{
    any::Any,
    cell::{RefCell, RefMut},
    rc::Rc,
    sync::Arc,
};

use super::{DependencyGraph, ToniContainer, multi_collection_provider::MultiCollectionProvider};
use crate::{
    structs_helpers::EnhancerMetadata,
    traits_helpers::{Controller, Provider},
};

pub struct ToniInstanceLoader {
    container: Rc<RefCell<ToniContainer>>,
}

impl ToniInstanceLoader {
    pub fn new(container: Rc<RefCell<ToniContainer>>) -> Self {
        Self { container }
    }

    pub async fn create_instances_of_dependencies(&self) -> Result<()> {
        // Set the container context for ModuleRef to access
        super::module_ref_provider::set_container_context(self.container.clone());

        let modules_order = self.container.borrow().get_ordered_modules_token();

        // PHASE 1: Create provider instances for all modules (with deferred retry logic)
        // Track which modules are pending (deferred due to unready global providers)
        let mut pending_modules: Vec<String> = modules_order.clone();
        let total_modules = pending_modules.len();
        let mut max_iterations = total_modules * 2; // Prevent infinite loops

        while !pending_modules.is_empty() && max_iterations > 0 {
            max_iterations -= 1;
            let mut successfully_created = Vec::new();
            let mut deferred_modules = Vec::new();

            for module_token in &pending_modules {
                match self
                    .create_instances_of_providers(module_token.clone())
                    .await
                {
                    Ok(_) => {
                        // Module providers created successfully - register its global providers
                        self.container
                            .borrow_mut()
                            .register_global_providers(module_token)?;
                        successfully_created.push(module_token.clone());
                    }
                    Err(e) if e.to_string().contains("DEFERRED:") => {
                        // Dependency not ready - defer to next iteration
                        deferred_modules.push(module_token.clone());
                        continue;
                    }
                    Err(e) => {
                        // Real error - propagate
                        return Err(e);
                    }
                }
            }

            if successfully_created.is_empty() && !pending_modules.is_empty() {
                // No progress made - circular dependency or missing provider
                return Err(anyhow!(
                    "Cannot resolve dependencies for modules: {:?}. \
                     Possible circular dependency or missing global provider.",
                    pending_modules
                ));
            }

            // Update pending list to only deferred modules
            pending_modules = deferred_modules;
        }

        if !pending_modules.is_empty() {
            return Err(anyhow!(
                "Module instantiation timed out. Remaining modules: {:?}",
                pending_modules
            ));
        }

        // PHASE 1.5: Collect multi-provider contributions into Vec collections per base token.
        // Must run after all individual providers are built so as_multi_item() is available.
        self.collect_multi_providers()?;

        // PHASE 2: Resolve APP_* token providers to global enhancers
        // This happens AFTER all provider instances are created but BEFORE controllers are instantiated
        // This allows APP_* enhancers to have injected dependencies AND be available when controllers are created
        self.resolve_app_token_enhancers()?;

        // PHASE 3: Resolve middleware tokens from DI container
        // This happens AFTER DI container is built, allowing middleware to have injected dependencies
        self.resolve_middleware_tokens(&modules_order)?;

        // PHASE 4: Create controller instances now that global enhancers are registered
        for module_token in &modules_order {
            self.create_instances_of_controllers(module_token.clone())
                .await?;
        }

        Ok(())
    }

    /// Collect multi-provider contributions into MultiCollectionProvider instances.
    ///
    /// Iterates all registered multi-provider groups (base_token -> contributions), calls
    /// as_multi_item() on each built contribution, and stores the resulting collection
    /// in the container so it can be resolved like any other provider dependency.
    fn collect_multi_providers(&self) -> Result<()> {
        let multi_map = self
            .container
            .borrow()
            .get_multi_providers()
            .clone();

        for (base_token, contributions) in multi_map {
            let mut items: Vec<Arc<dyn Any + Send + Sync>> = Vec::new();

            for (module_token, provider_token) in &contributions {
                let container = self.container.borrow();
                let provider = container
                    .get_provider_instance_by_token(module_token, provider_token)?
                    .ok_or_else(|| {
                        anyhow!(
                            "Multi-provider contribution '{}' not found in module '{}'",
                            provider_token,
                            module_token
                        )
                    })?
                    .clone();

                let item = provider.as_multi_item().ok_or_else(|| {
                    anyhow!(
                        "Provider '{}' is registered as multi but does not implement as_multi_item()",
                        provider_token
                    )
                })?;
                items.push(item);
            }

            let collection: Arc<Box<dyn Provider>> = Arc::new(Box::new(MultiCollectionProvider {
                token: base_token.clone(),
                items,
            }));
            self.container
                .borrow_mut()
                .add_multi_collection_provider(base_token, collection);
        }

        Ok(())
    }

    /// Resolve APP_* token providers to global enhancers
    fn resolve_app_token_enhancers(&self) -> Result<()> {
        let container = self.container.borrow();

        // Get APP_GUARD providers
        let app_guard_providers = container.get_app_guard_providers().to_vec();
        drop(container);

        for (module_token, provider_token) in app_guard_providers {
            let guard = {
                let container_ref = self.container.borrow();
                let provider = container_ref
                    .get_provider_instance_by_token(&module_token, &provider_token)?
                    .ok_or_else(|| {
                        anyhow!(
                            "APP_GUARD provider '{}' not found in module '{}'",
                            provider_token,
                            module_token
                        )
                    })?;

                provider.as_guard().ok_or_else(|| {
                    anyhow!(
                        "Provider '{}' with APP_GUARD token does not implement Guard trait",
                        provider_token
                    )
                })?
            };
            self.container.borrow_mut().add_global_guard(guard);
        }

        // Get APP_INTERCEPTOR providers
        let container = self.container.borrow();
        let app_interceptor_providers = container.get_app_interceptor_providers().to_vec();
        drop(container);

        for (module_token, provider_token) in app_interceptor_providers {
            let interceptor = {
                let container_ref = self.container.borrow();
                let provider = container_ref
                    .get_provider_instance_by_token(&module_token, &provider_token)?
                    .ok_or_else(|| {
                        anyhow!(
                            "APP_INTERCEPTOR provider '{}' not found in module '{}'",
                            provider_token,
                            module_token
                        )
                    })?;

                provider.as_interceptor().ok_or_else(|| {
                    anyhow!(
                        "Provider '{}' with APP_INTERCEPTOR token does not implement Interceptor trait",
                        provider_token
                    )
                })?
            };
            self.container
                .borrow_mut()
                .add_global_interceptor(interceptor);
        }

        // Get APP_PIPE providers
        let container = self.container.borrow();
        let app_pipe_providers = container.get_app_pipe_providers().to_vec();
        drop(container);

        for (module_token, provider_token) in app_pipe_providers {
            let pipe = {
                let container_ref = self.container.borrow();
                let provider = container_ref
                    .get_provider_instance_by_token(&module_token, &provider_token)?
                    .ok_or_else(|| {
                        anyhow!(
                            "APP_PIPE provider '{}' not found in module '{}'",
                            provider_token,
                            module_token
                        )
                    })?;

                provider.as_pipe().ok_or_else(|| {
                    anyhow!(
                        "Provider '{}' with APP_PIPE token does not implement Pipe trait",
                        provider_token
                    )
                })?
            };
            self.container.borrow_mut().add_global_pipe(pipe);
        }

        Ok(())
    }

    /// Resolve middleware tokens from DI container
    fn resolve_middleware_tokens(&self, modules_order: &[String]) -> Result<()> {
        for module_token in modules_order {
            // Get providers for this module
            let providers = self
                .container
                .borrow()
                .get_providers_instance(module_token)?
                .clone();

            // Resolve middleware tokens to instances
            let mut container_mut = self.container.borrow_mut();
            if let Some(middleware_manager) = container_mut.get_middleware_manager_mut() {
                middleware_manager.resolve_middleware_tokens(module_token, &providers)?;
            }
        }
        Ok(())
    }

    async fn create_instances_of_providers(&self, module_token: String) -> Result<()> {
        let dependency_graph = DependencyGraph::new(self.container.clone(), module_token.clone());
        let ordered_providers_token = dependency_graph.get_ordered_providers_token()?;
        let provider_instances = {
            let container = self.container.borrow();
            let mut instances = FxHashMap::default();

            for provider_token in ordered_providers_token {
                let provider_factory = container
                    .get_provider_by_token(&module_token, &provider_token)?
                    .ok_or_else(|| anyhow!("Provider not found: {}", provider_token))?;

                let dependencies = provider_factory.get_dependencies();
                let resolved_dependencies =
                    self.resolve_dependencies(&module_token, dependencies, Some(&instances))?;

                let provider = provider_factory.build(resolved_dependencies).await;
                instances.insert(provider.get_token(), provider);
            }
            instances
        };
        self.add_providers_instances(&module_token, provider_instances)?;
        Ok(())
    }

    fn add_providers_instances(
        &self,
        module_token: &String,
        providers_instances: FxHashMap<String, Arc<Box<dyn Provider>>>,
    ) -> Result<()> {
        let mut container = self.container.borrow_mut();
        let mut providers_tokens = Vec::new();
        for (provider_instance_token, provider_instance) in providers_instances {
            let token_factory = provider_instance.get_token_factory().clone();
            container.add_provider_instance(module_token, provider_instance)?;
            providers_tokens.push((token_factory, provider_instance_token));
        }

        self.resolve_exports(module_token, providers_tokens, container)?;
        Ok(())
    }

    fn resolve_exports(
        &self,
        module_token: &String,
        providers_tokens: Vec<(String, String)>,
        container: RefMut<'_, ToniContainer>,
    ) -> Result<()> {
        let exports = container.get_exports_tokens_vec(module_token)?;
        self.add_export_instances_tokens(module_token, providers_tokens, exports, container)?;
        Ok(())
    }

    fn add_export_instances_tokens(
        &self,
        module_token: &String,
        providers_tokens: Vec<(String, String)>,
        exports: Vec<String>,
        mut container: RefMut<'_, ToniContainer>,
    ) -> Result<()> {
        for (provider_factory_token, provider_instance_token) in providers_tokens {
            if exports.contains(&provider_factory_token) {
                container.add_export_instance(module_token, provider_instance_token)?;
            }
        }
        Ok(())
    }

    async fn create_instances_of_controllers(&self, module_token: String) -> Result<()> {
        let controllers_instances = {
            let container = self.container.borrow();
            let mut instances = Vec::new();
            let controllers_factory = container.get_controllers_factory(&module_token)?;

            for controller_factory in controllers_factory.values() {
                let dependencies = controller_factory.get_dependencies();
                let resolved_dependencies =
                    self.resolve_dependencies(&module_token, dependencies, None)?;
                let mut built = controller_factory.build(resolved_dependencies).await;
                instances.append(&mut built);
            }
            instances
        };
        self.add_controllers_instances(module_token, controllers_instances)?;
        Ok(())
    }

    fn add_controllers_instances(
        &self,
        module_token: String,
        controllers_instances: Vec<Arc<Box<dyn Controller>>>,
    ) -> Result<()> {
        let mut container_mut = self.container.borrow_mut();

        let providers = container_mut.get_providers_instance(&module_token)?.clone();

        for controller_instance in controllers_instances {
            let enhancer_metadata =
                self.resolve_enhancers_from_tokens(&controller_instance, &providers)?;

            container_mut.add_controller_instance(
                &module_token,
                controller_instance,
                enhancer_metadata,
            )?;
        }
        Ok(())
    }

    /// Resolve enhancers from both DI container and direct instantiation
    ///
    /// Enhancers can be provided in two ways:
    /// 1. **DI-based** (`#[use_guards(AuthGuard)]`):
    ///    - Generates token → looked up in DI container
    ///    - Must be registered in module providers
    ///    - Supports dependency injection
    ///
    /// 2. **Direct instantiation** (`#[use_guards(MyGuard{})]` or `#[use_guards(MyGuard::new())]`):
    ///    - Generates instance expression → directly instantiated
    ///    - No DI lookup performed
    ///    - No dependency injection support
    ///
    /// Both types are collected and combined in order:
    /// - First: DI-resolved enhancers (from tokens)
    /// - Then: Directly instantiated enhancers (from instances)
    fn resolve_enhancers_from_tokens(
        &self,
        controller: &Arc<Box<dyn Controller>>,
        providers: &FxHashMap<String, Arc<Box<dyn Provider>>>,
    ) -> Result<EnhancerMetadata> {
        // Resolve guards from DI (type name syntax: AuthGuard)
        let mut guards = Vec::new();
        for token in controller.get_guard_tokens() {
            if let Some(provider_box) = providers.get(&token) {
                if let Some(guard) = provider_box.as_guard() {
                    guards.push(guard);
                } else {
                    return Err(anyhow!(
                        "Provider '{}' was expected to be a Guard but as_guard() returned None. \
                         Ensure the provider implements the Guard trait.",
                        token
                    ));
                }
            } else {
                return Err(anyhow!(
                    "Guard provider '{}' not found in DI container. \
                     Did you forget to add it to the module's providers? \
                     Or use direct instantiation with '{{}}' or '::new()' instead.",
                    token
                ));
            }
        }
        // Add directly instantiated guards (struct literal or constructor syntax: MyGuard{} or MyGuard::new())
        guards.extend(controller.get_guards());

        // Resolve interceptors from DI (type name syntax: LoggingInterceptor)
        let mut interceptors = Vec::new();
        for token in controller.get_interceptor_tokens() {
            if let Some(provider_box) = providers.get(&token) {
                if let Some(interceptor) = provider_box.as_interceptor() {
                    interceptors.push(interceptor);
                } else {
                    return Err(anyhow!(
                        "Provider '{}' was expected to be an Interceptor but as_interceptor() returned None. \
                         Ensure the provider implements the Interceptor trait.",
                        token
                    ));
                }
            } else {
                return Err(anyhow!(
                    "Interceptor provider '{}' not found in DI container. \
                     Did you forget to add it to the module's providers? \
                     Or use direct instantiation with '{{}}' or '::new()' instead.",
                    token
                ));
            }
        }
        // Add directly instantiated interceptors (struct literal or constructor syntax)
        interceptors.extend(controller.get_interceptors());

        // Resolve pipes from DI (type name syntax: ValidationPipe)
        let mut pipes = Vec::new();
        for token in controller.get_pipe_tokens() {
            if let Some(provider_box) = providers.get(&token) {
                if let Some(pipe) = provider_box.as_pipe() {
                    pipes.push(pipe);
                } else {
                    return Err(anyhow!(
                        "Provider '{}' was expected to be a Pipe but as_pipe() returned None. \
                         Ensure the provider implements the Pipe trait.",
                        token
                    ));
                }
            } else {
                return Err(anyhow!(
                    "Pipe provider '{}' not found in DI container. \
                     Did you forget to add it to the module's providers? \
                     Or use direct instantiation with '{{}}' or '::new()' instead.",
                    token
                ));
            }
        }
        // Add directly instantiated pipes (struct literal or constructor syntax)
        pipes.extend(controller.get_pipes());

        let mut error_handlers = Vec::new();
        for token in controller.get_error_handler_tokens() {
            if let Some(provider_box) = providers.get(&token) {
                if let Some(error_handler) = provider_box.as_error_handler() {
                    error_handlers.push(error_handler);
                } else {
                    return Err(anyhow!(
                        "Provider '{}' was expected to be an ErrorHandler but as_error_handler() returned None. \
                         Ensure the provider implements the ErrorHandler trait.",
                        token
                    ));
                }
            } else {
                return Err(anyhow!(
                    "ErrorHandler provider '{}' not found in DI container. \
                     Did you forget to add it to the module's providers?",
                    token
                ));
            }
        }
        error_handlers.extend(controller.get_error_handlers());

        Ok(EnhancerMetadata {
            guards,
            interceptors,
            pipes,
            error_handlers,
        })
    }

    fn resolve_dependencies(
        &self,
        module_token: &String,
        dependencies: Vec<String>,
        providers_instances: Option<&FxHashMap<String, Arc<Box<dyn Provider>>>>,
    ) -> Result<FxHashMap<String, Arc<Box<dyn Provider>>>> {
        let container = self.container.borrow();
        let mut resolved_dependencies = FxHashMap::default();

        for dependency in dependencies {
            let instances = match providers_instances {
                Some(providers_instances) => providers_instances,
                None => container.get_providers_instance(module_token)?,
            };
            // Step 1: Check local providers
            if let Some(instance) = instances.get(&dependency) {
                resolved_dependencies.insert(dependency, instance.clone());
            }
            // Step 2: Check imported modules
            else if let Some(exported_instance) =
                self.resolve_from_imported_modules(module_token, &dependency)?
            {
                resolved_dependencies.insert(dependency, exported_instance.clone());
            }
            // Step 3: Check if it's a registered global provider token
            else if container.is_global_provider_token(&dependency) {
                // Token is registered as global, try to get the instance
                if let Some(global_instance) = container.get_global_provider(&dependency) {
                    // Instance exists - use it
                    resolved_dependencies.insert(dependency, global_instance.clone());
                } else {
                    // Token registered but instance not created yet - DEFER
                    return Err(anyhow!(
                        "DEFERRED: Global provider '{}' not yet instantiated for module '{}'",
                        dependency,
                        module_token
                    ));
                }
            }
            // Step 3.5: Check multi-collection providers (built after Phase 1)
            else if let Some(multi_instance) =
                container.get_multi_collection_provider(&dependency)
            {
                resolved_dependencies.insert(dependency, multi_instance);
            }
            // Step 4: Not found anywhere
            else {
                return Err(anyhow!(
                    "Dependency not found: {} in module {}",
                    dependency,
                    module_token
                ));
            }
        }

        Ok(resolved_dependencies)
    }

    fn resolve_from_imported_modules(
        &self,
        module_token: &String,
        dependency: &String,
    ) -> Result<Option<Arc<Box<dyn Provider>>>> {
        let container = self.container.borrow();
        let imported_modules = container.get_imported_modules(module_token)?;

        for imported_module in imported_modules {
            // Check if the imported module exports this dependency (from scan phase)
            let exports_tokens = container.get_exports_tokens_vec(imported_module)?;

            if exports_tokens.contains(dependency) {
                // Dependency is exported by this module - try to get the instance
                let exported_instances_tokens =
                    container.get_exports_instances_tokens(imported_module)?;

                if exported_instances_tokens.contains(dependency) {
                    // Instance exists - return it
                    if let Ok(Some(exported_instance)) =
                        container.get_provider_instance_by_token(imported_module, dependency)
                    {
                        return Ok(Some(exported_instance.clone()));
                    }
                } else {
                    // Module exports this dependency but instance not created yet - DEFER
                    return Err(anyhow!(
                        "DEFERRED: Imported module '{}' exports '{}' but instance not yet created for module '{}'",
                        imported_module,
                        dependency,
                        module_token
                    ));
                }
            }
        }

        Ok(None)
    }
}
