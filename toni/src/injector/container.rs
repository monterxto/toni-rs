use std::{collections::hash_map::Drain, sync::Arc};

use anyhow::{Result, anyhow};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    middleware::MiddlewareManager,
    rpc::RpcControllerTrait,
    structs_helpers::EnhancerMetadata,
    traits_helpers::{
        Controller, ControllerFactory, Guard, Interceptor, ModuleMetadata, Pipe, Provider,
        ProviderFactory,
    },
    websocket::GatewayTrait,
};

use super::{InstanceWrapper, module::Module};

pub struct ToniContainer {
    modules: FxHashMap<String, Module>,
    middleware_manager: Option<MiddlewareManager>,
    /// Global provider registry - providers from modules marked as global
    global_providers: FxHashMap<String, Arc<Box<dyn Provider>>>,
    /// Global provider tokens - registered during scan phase (before instance creation)
    global_provider_tokens: FxHashSet<String>,
    /// Global enhancers - applied to all controllers
    global_guards: Vec<Arc<dyn Guard>>,
    global_interceptors: Vec<Arc<dyn Interceptor>>,
    global_pipes: Vec<Arc<dyn Pipe>>,
    global_error_handlers: Vec<Arc<dyn crate::traits_helpers::ErrorHandler>>,
    /// APP_* token providers - providers registered with special tokens (module_token, provider_token)
    /// These will be resolved to global enhancers after DI container is built
    app_guard_providers: Vec<(String, String)>,
    app_interceptor_providers: Vec<(String, String)>,
    app_pipe_providers: Vec<(String, String)>,
    /// WebSocket gateways — populated automatically when provider instances are added.
    /// Key is the WS path (e.g. "/chat"), value is the raw gateway ready for wrapping.
    gateways: FxHashMap<String, Arc<Box<dyn GatewayTrait>>>,
    /// RPC controllers — populated automatically when provider instances are added.
    /// Key is the controller token, value is the raw controller ready for wrapping.
    rpc_controllers: FxHashMap<String, Arc<Box<dyn RpcControllerTrait>>>,
}

impl Default for ToniContainer {
    fn default() -> Self {
        Self::new()
    }
}

impl ToniContainer {
    pub fn new() -> Self {
        Self {
            modules: FxHashMap::default(),
            middleware_manager: Some(MiddlewareManager::new()),
            global_providers: FxHashMap::default(),
            global_provider_tokens: FxHashSet::default(),
            global_guards: Vec::new(),
            global_interceptors: Vec::new(),
            global_pipes: Vec::new(),
            global_error_handlers: Vec::new(),
            app_guard_providers: Vec::new(),
            app_interceptor_providers: Vec::new(),
            app_pipe_providers: Vec::new(),
            gateways: FxHashMap::default(),
            rpc_controllers: FxHashMap::default(),
        }
    }

    pub fn add_global_guard(&mut self, guard: Arc<dyn Guard>) {
        self.global_guards.push(guard);
    }

    pub fn add_global_interceptor(&mut self, interceptor: Arc<dyn Interceptor>) {
        self.global_interceptors.push(interceptor);
    }

    pub fn add_global_pipe(&mut self, pipe: Arc<dyn Pipe>) {
        self.global_pipes.push(pipe);
    }

    pub fn add_global_error_handler(
        &mut self,
        handler: Arc<dyn crate::traits_helpers::ErrorHandler>,
    ) {
        self.global_error_handlers.push(handler);
    }

    pub fn get_global_enhancers(&self) -> EnhancerMetadata {
        EnhancerMetadata {
            guards: self.global_guards.clone(),
            interceptors: self.global_interceptors.clone(),
            pipes: self.global_pipes.clone(),
            error_handlers: self.global_error_handlers.clone(),
        }
    }

    pub fn add_module(&mut self, module_metadata: Box<dyn ModuleMetadata>) {
        let token: String = module_metadata.get_id();
        let name: String = module_metadata.get_name();
        let module = Module::new(&token, &name, module_metadata);
        self.modules.insert(token, module);
    }

    pub fn add_import(
        &mut self,
        module_ref_token: &String,
        imported_module_token: String,
    ) -> Result<()> {
        let module_ref = self
            .modules
            .get_mut(module_ref_token)
            .ok_or_else(|| anyhow!("Module not found"))?;
        module_ref.add_import(imported_module_token);
        Ok(())
    }

    pub fn add_controller(
        &mut self,
        module_ref_token: &String,
        controller: Box<dyn ControllerFactory>,
    ) -> Result<()> {
        let module_ref = self
            .modules
            .get_mut(module_ref_token)
            .ok_or_else(|| anyhow!("Module not found"))?;
        module_ref.add_controller(controller);
        Ok(())
    }

    pub fn add_provider(
        &mut self,
        module_ref_token: &String,
        provider: Box<dyn ProviderFactory>,
    ) -> Result<()> {
        let module_ref = self
            .modules
            .get_mut(module_ref_token)
            .ok_or_else(|| anyhow!("Module not found"))?;
        module_ref.add_provider(provider);
        Ok(())
    }

    pub fn add_provider_instance(
        &mut self,
        module_ref_token: &String,
        provider_instance: Arc<Box<dyn Provider>>,
    ) -> Result<()> {
        if let Some(gateway) = provider_instance.as_gateway() {
            self.gateways.insert(gateway.get_path(), gateway);
        }

        if let Some(rpc_ctrl) = provider_instance.as_rpc_controller() {
            self.rpc_controllers
                .insert(rpc_ctrl.get_token(), rpc_ctrl);
        }

        let module_ref = self
            .modules
            .get_mut(module_ref_token)
            .ok_or_else(|| anyhow!("Module not found"))?;
        module_ref.add_provider_instance(provider_instance);
        Ok(())
    }

    pub fn get_gateways(&self) -> &FxHashMap<String, Arc<Box<dyn GatewayTrait>>> {
        &self.gateways
    }

    pub fn get_rpc_controllers(
        &self,
    ) -> &FxHashMap<String, Arc<Box<dyn RpcControllerTrait>>> {
        &self.rpc_controllers
    }

    pub fn add_controller_instance(
        &mut self,
        module_ref_token: &String,
        controller_instance: Arc<Box<dyn Controller>>,
        enhancer_metadata: EnhancerMetadata,
    ) -> Result<()> {
        let global_enhancers = self.get_global_enhancers();
        let module_ref = self
            .modules
            .get_mut(module_ref_token)
            .ok_or_else(|| anyhow!("Module not found"))?;
        module_ref.add_controller_instance(
            controller_instance,
            enhancer_metadata,
            global_enhancers,
        );
        Ok(())
    }

    pub fn add_export(&mut self, module_ref_token: &String, provider_token: String) -> Result<()> {
        let module_ref = self
            .modules
            .get_mut(module_ref_token)
            .ok_or_else(|| anyhow!("Module not found"))?;
        module_ref.add_export(provider_token);
        Ok(())
    }

    pub fn add_export_instance(
        &mut self,
        module_ref_token: &String,
        provider_token: String,
    ) -> Result<()> {
        let module_ref = self
            .modules
            .get_mut(module_ref_token)
            .ok_or_else(|| anyhow!("Module not found"))?;
        module_ref.add_export_instance(provider_token);
        Ok(())
    }

    pub fn get_providers_factory(
        &self,
        module_ref_token: &String,
    ) -> Result<&FxHashMap<String, Box<dyn ProviderFactory>>> {
        let module_ref = self
            .modules
            .get(module_ref_token)
            .ok_or_else(|| anyhow!("Module not found"))?;
        Ok(module_ref.get_providers_factory())
    }

    pub fn get_controllers_factory(
        &self,
        module_ref_token: &String,
    ) -> Result<&FxHashMap<String, Box<dyn ControllerFactory>>> {
        let module_ref = self
            .modules
            .get(module_ref_token)
            .ok_or_else(|| anyhow!("Module not found"))?;
        Ok(module_ref.get_controllers_factory())
    }

    pub fn get_providers_instance(
        &self,
        module_ref_token: &String,
    ) -> Result<&FxHashMap<String, Arc<Box<dyn Provider>>>> {
        let module_ref = self
            .modules
            .get(module_ref_token)
            .ok_or_else(|| anyhow!("Module not found"))?;
        Ok(module_ref.get_providers_instances())
    }

    pub fn get_provider_instance_by_token(
        &self,
        module_ref_token: &String,
        provider_token: &String,
    ) -> Result<Option<&Arc<Box<dyn Provider>>>> {
        let module_ref = self
            .modules
            .get(module_ref_token)
            .ok_or_else(|| anyhow!("Module not found"))?;
        Ok(module_ref.get_provider_instance_by_token(provider_token))
    }

    pub fn get_provider_by_token(
        &self,
        module_ref_token: &String,
        provider_token: &String,
    ) -> Result<Option<&dyn ProviderFactory>> {
        let module_ref = self
            .modules
            .get(module_ref_token)
            .ok_or_else(|| anyhow!("Module not found"))?;
        Ok(module_ref.get_provider_by_token(provider_token))
    }

    pub fn get_controllers_instance(
        &mut self,
        module_ref_token: &String,
    ) -> Result<Drain<'_, String, Arc<InstanceWrapper>>> {
        let module_ref = self
            .modules
            .get_mut(module_ref_token)
            .ok_or_else(|| anyhow!("Module not found"))?;
        Ok(module_ref.drain_controllers_instances())
    }

    pub fn get_imported_modules(&self, module_ref_token: &String) -> Result<&FxHashSet<String>> {
        let module_ref = self
            .modules
            .get(module_ref_token)
            .ok_or_else(|| anyhow!("Module not found"))?;
        Ok(module_ref.get_imported_modules())
    }

    pub fn get_exports_instances_tokens(
        &self,
        module_ref_token: &String,
    ) -> Result<&FxHashSet<String>> {
        let module_ref = self
            .modules
            .get(module_ref_token)
            .ok_or_else(|| anyhow!("Module not found: {:?}", module_ref_token))?;
        Ok(module_ref.get_exports_instances_tokens())
    }

    pub fn get_exports_tokens_vec(&self, module_ref_token: &String) -> Result<Vec<String>> {
        let module_ref = self
            .modules
            .get(module_ref_token)
            .ok_or_else(|| anyhow!("Module not found: {:?}", module_ref_token))?;
        Ok(module_ref.get_exports_tokens().iter().cloned().collect())
    }

    pub fn get_modules_token(&self) -> Vec<String> {
        self.modules.keys().cloned().collect::<Vec<String>>()
    }

    pub fn get_ordered_modules_token(&self) -> Vec<String> {
        let mut ordered_modules: Vec<String> = Vec::new();
        let mut visited: FxHashMap<String, bool> = FxHashMap::default();

        // Standard topological sort based on explicit imports
        while ordered_modules.len() < self.modules.len() {
            let mut ready_modules: Vec<String> = Vec::new();

            for (token, module) in self.modules.iter() {
                if visited.contains_key(token) {
                    continue;
                }

                let imported_modules = module.get_imported_modules();
                let all_imports_processed = imported_modules
                    .iter()
                    .all(|import_token| visited.contains_key(import_token));

                if all_imports_processed {
                    ready_modules.push(token.clone());
                }
            }

            if ready_modules.is_empty() {
                // No modules are ready - circular dependency
                break;
            }

            for token in ready_modules {
                ordered_modules.push(token.clone());
                visited.insert(token.clone(), true);
            }
        }

        ordered_modules
    }

    pub fn get_module_by_token(&self, module_ref_token: &String) -> Option<&Module> {
        self.modules.get(module_ref_token)
    }

    /// Register all exported providers from a global module into the global registry
    pub fn register_global_providers(&mut self, module_token: &String) -> Result<()> {
        let module = self
            .modules
            .get(module_token)
            .ok_or_else(|| anyhow!("Module not found: {}", module_token))?;

        // Only register if module is marked as global
        if !module.get_metadata().is_global() {
            return Ok(());
        }

        // Register all exported providers as globally accessible
        let exports_tokens = module.get_exports_instances_tokens().clone();
        for export_token in exports_tokens.iter() {
            if let Ok(Some(instance)) =
                self.get_provider_instance_by_token(module_token, export_token)
            {
                self.global_providers
                    .insert(export_token.clone(), instance.clone());
            }
        }

        Ok(())
    }

    /// Get a provider from the global registry
    pub fn get_global_provider(&self, token: &String) -> Option<Arc<Box<dyn Provider>>> {
        self.global_providers.get(token).cloned()
    }

    /// Register a provider token as globally available (during scan phase)
    pub fn register_global_provider_token(&mut self, token: String) {
        self.global_provider_tokens.insert(token);
    }

    /// Check if a provider token is registered as globally available
    pub fn is_global_provider_token(&self, token: &String) -> bool {
        self.global_provider_tokens.contains(token)
    }

    // pub fn register_controller_enhancers(
    //     &mut self,
    //     module_ref_token: &String,
    //     controller_token: &String,
    //     controller_enhancers: &Vec<Box<dyn ControllerEnhancer>>,
    // ) -> Result<()> {
    //     let module_ref = self
    //         .modules
    //         .get_mut(module_ref_token)
    //         .ok_or_else(|| anyhow!("Module not found"))?;
    //     module_ref.register_controller_enhancers(controller_enhancers);
    //     Ok(())
    // }

    pub fn get_middleware_manager(&self) -> Option<&MiddlewareManager> {
        self.middleware_manager.as_ref()
    }

    pub fn get_middleware_manager_mut(&mut self) -> Option<&mut MiddlewareManager> {
        self.middleware_manager.as_mut()
    }

    /// Register a provider with APP_GUARD token (during scan phase)
    pub fn register_app_guard_provider(&mut self, module_token: String, provider_token: String) {
        self.app_guard_providers
            .push((module_token, provider_token));
    }

    /// Register a provider with APP_INTERCEPTOR token (during scan phase)
    pub fn register_app_interceptor_provider(
        &mut self,
        module_token: String,
        provider_token: String,
    ) {
        self.app_interceptor_providers
            .push((module_token, provider_token));
    }

    /// Register a provider with APP_PIPE token (during scan phase)
    pub fn register_app_pipe_provider(&mut self, module_token: String, provider_token: String) {
        self.app_pipe_providers.push((module_token, provider_token));
    }

    /// Get all APP_GUARD providers (after instances are created)
    pub fn get_app_guard_providers(&self) -> &[(String, String)] {
        &self.app_guard_providers
    }

    /// Get all APP_INTERCEPTOR providers (after instances are created)
    pub fn get_app_interceptor_providers(&self) -> &[(String, String)] {
        &self.app_interceptor_providers
    }

    /// Get all APP_PIPE providers (after instances are created)
    pub fn get_app_pipe_providers(&self) -> &[(String, String)] {
        &self.app_pipe_providers
    }
}
