use anyhow::{Result, anyhow};
use rustc_hash::FxHashMap;
use std::{
    cell::{RefCell, RefMut},
    rc::Rc,
    sync::Arc,
};

use super::{DependencyGraph, ToniContainer};
use crate::{
    structs_helpers::EnhancerMetadata,
    traits_helpers::{ControllerTrait, ProviderTrait},
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

        // Track which modules are pending (deferred due to unready global providers)
        let mut pending_modules: Vec<String> = modules_order;
        let total_modules = pending_modules.len();
        let mut max_iterations = total_modules * 2; // Prevent infinite loops

        while !pending_modules.is_empty() && max_iterations > 0 {
            max_iterations -= 1;
            let mut successfully_created = Vec::new();
            let mut deferred_modules = Vec::new();

            for module_token in &pending_modules {
                match self.create_module_instances(module_token.clone()).await {
                    Ok(_) => {
                        // Module created successfully - register its global providers
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

        Ok(())
    }

    async fn create_module_instances(&self, module_token: String) -> Result<()> {
        self.create_instances_of_providers(module_token.clone())
            .await?;
        self.create_instances_of_controllers(module_token.clone())
            .await?;
        Ok(())
    }

    async fn create_instances_of_providers(&self, module_token: String) -> Result<()> {
        let dependency_graph = DependencyGraph::new(self.container.clone(), module_token.clone());
        let ordered_providers_token = dependency_graph.get_ordered_providers_token()?;
        let provider_instances = {
            let container = self.container.borrow();
            let mut instances = FxHashMap::default();

            for provider_token in ordered_providers_token {
                let provider_manager = container
                    .get_provider_by_token(&module_token, &provider_token)?
                    .ok_or_else(|| anyhow!("Provider not found: {}", provider_token))?;

                let dependencies = provider_manager.get_dependencies();
                let resolved_dependencies =
                    self.resolve_dependencies(&module_token, dependencies, Some(&instances))?;

                let provider_instances = provider_manager
                    .get_all_providers(&resolved_dependencies)
                    .await;
                instances.extend(provider_instances);
            }
            instances
        };
        self.add_providers_instances(&module_token, provider_instances)?;
        Ok(())
    }

    fn add_providers_instances(
        &self,
        module_token: &String,
        providers_instances: FxHashMap<String, Arc<Box<dyn ProviderTrait>>>,
    ) -> Result<()> {
        let mut container = self.container.borrow_mut();
        let mut providers_tokens = Vec::new();
        for (provider_instance_token, provider_instance) in providers_instances {
            let token_manager = provider_instance.get_token_manager().clone();
            container.add_provider_instance(module_token, provider_instance)?;
            providers_tokens.push((token_manager, provider_instance_token));
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
        for (provider_manager_token, provider_instance_token) in providers_tokens {
            if exports.contains(&provider_manager_token) {
                container.add_export_instance(module_token, provider_instance_token)?;
            }
        }
        Ok(())
    }

    async fn create_instances_of_controllers(&self, module_token: String) -> Result<()> {
        let controllers_instances = {
            let container = self.container.borrow();
            let mut instances = FxHashMap::default();
            let controllers_manager = container.get_controllers_manager(&module_token)?;

            for controller_manager in controllers_manager.values() {
                let dependencies = controller_manager.get_dependencies();
                let resolved_dependencies =
                    self.resolve_dependencies(&module_token, dependencies, None)?;
                let controllers_instances = controller_manager
                    .get_all_controllers(&resolved_dependencies)
                    .await;
                instances.extend(controllers_instances);
            }
            instances
        };
        self.add_controllers_instances(module_token, controllers_instances)?;
        Ok(())
    }

    fn add_controllers_instances(
        &self,
        module_token: String,
        controllers_instances: FxHashMap<String, Arc<Box<dyn ControllerTrait>>>,
    ) -> Result<()> {
        let mut container_mut = self.container.borrow_mut();

        // Get providers from the module to resolve enhancers
        let providers = container_mut.get_providers_instance(&module_token)?.clone();

        for (_controller_instance_token, controller_instance) in controllers_instances {
            // Resolve enhancers from DI using tokens
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

    /// Resolve enhancers from DI container using tokens
    fn resolve_enhancers_from_tokens(
        &self,
        controller: &Arc<Box<dyn ControllerTrait>>,
        providers: &FxHashMap<String, Arc<Box<dyn ProviderTrait>>>,
    ) -> Result<EnhancerMetadata> {
        let mut guards = Vec::new();
        let mut interceptors = Vec::new();
        let mut pipes = Vec::new();

        // Resolve guards from tokens
        for token in controller.get_guard_tokens() {
            if let Some(provider_box) = providers.get(&token) {
                // Call as_guard() on the provider - returns Some if it's actually a Guard
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
                     Ensure it's registered in the module's providers.",
                    token
                ));
            }
        }

        // Resolve interceptors from tokens
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
                     Ensure it's registered in the module's providers.",
                    token
                ));
            }
        }

        // Resolve pipes from tokens
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
                     Ensure it's registered in the module's providers.",
                    token
                ));
            }
        }

        Ok(EnhancerMetadata {
            guards,
            interceptors,
            pipes,
        })
    }

    fn resolve_dependencies(
        &self,
        module_token: &String,
        dependencies: Vec<String>,
        providers_instances: Option<&FxHashMap<String, Arc<Box<dyn ProviderTrait>>>>,
    ) -> Result<FxHashMap<String, Arc<Box<dyn ProviderTrait>>>> {
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
    ) -> Result<Option<Arc<Box<dyn ProviderTrait>>>> {
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
