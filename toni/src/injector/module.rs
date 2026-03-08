use std::{collections::hash_map::Drain, sync::Arc};

use rustc_hash::{FxHashMap, FxHashSet};

use super::InstanceWrapper;

use crate::{
    structs_helpers::EnhancerMetadata,
    traits_helpers::{Controller, ControllerFactory, ModuleMetadata, Provider, ProviderFactory},
};
pub struct Module {
    _token: String,
    _name: String,
    controllers: FxHashMap<String, Box<dyn ControllerFactory>>,
    providers: FxHashMap<String, Box<dyn ProviderFactory>>,
    imports: FxHashSet<String>,
    exports: FxHashSet<String>,
    controllers_instances: FxHashMap<String, Arc<InstanceWrapper>>,
    providers_instances: FxHashMap<String, Arc<Box<dyn Provider>>>,
    exports_instances: FxHashSet<String>,
    metadata: Box<dyn ModuleMetadata>,
}

impl Module {
    pub fn new(token: &str, name: &str, metadata: Box<dyn ModuleMetadata>) -> Self {
        Self {
            _token: token.to_owned(),
            _name: name.to_string(),
            controllers: FxHashMap::default(),
            providers: FxHashMap::default(),
            imports: FxHashSet::default(),
            exports: FxHashSet::default(),
            controllers_instances: FxHashMap::default(),
            providers_instances: FxHashMap::default(),
            exports_instances: FxHashSet::default(),
            metadata,
        }
    }
}
impl Module {
    pub fn add_controller(&mut self, controller: Box<dyn ControllerFactory>) {
        self.controllers.insert(controller.get_token(), controller);
    }

    pub fn add_provider(&mut self, provider: Box<dyn ProviderFactory>) {
        self.providers.insert(provider.get_token(), provider);
    }

    pub fn add_import(&mut self, module_token: String) {
        self.imports.insert(module_token);
    }

    pub fn add_export(&mut self, provider_token: String) {
        self.exports.insert(provider_token);
    }

    pub fn add_controller_instance(
        &mut self,
        controller: Arc<Box<dyn Controller>>,
        enhancer_metadata: EnhancerMetadata,
        global_enhancers: EnhancerMetadata,
    ) {
        let token = controller.get_token();
        let instance_wrapper =
            InstanceWrapper::new(controller, enhancer_metadata, global_enhancers);
        self.controllers_instances
            .insert(token, Arc::new(instance_wrapper));
    }

    pub fn add_provider_instance(&mut self, provider: Arc<Box<dyn Provider>>) {
        self.providers_instances
            .insert(provider.get_token(), provider);
    }
    pub fn add_export_instance(&mut self, provider_token: String) {
        self.exports_instances.insert(provider_token);
    }

    pub fn get_providers_manager(&self) -> &FxHashMap<String, Box<dyn ProviderFactory>> {
        &self.providers
    }

    pub fn get_providers_instances(&self) -> &FxHashMap<String, Arc<Box<dyn Provider>>> {
        &self.providers_instances
    }

    pub fn get_provider_by_token(&self, provider_token: &String) -> Option<&dyn ProviderFactory> {
        self.providers
            .get(provider_token)
            .map(|provider| provider.as_ref())
    }

    pub fn get_provider_instance_by_token(
        &self,
        provider_token: &String,
    ) -> Option<&Arc<Box<dyn Provider>>> {
        self.providers_instances.get(provider_token)
    }

    pub fn get_controllers_manager(&self) -> &FxHashMap<String, Box<dyn ControllerFactory>> {
        &self.controllers
    }

    pub fn drain_controllers_instances(&mut self) -> Drain<'_, String, Arc<InstanceWrapper>> {
        self.controllers_instances.drain()
    }

    pub fn get_imported_modules(&self) -> &FxHashSet<String> {
        &self.imports
    }

    pub fn get_exports_instances_tokens(&self) -> &FxHashSet<String> {
        &self.exports_instances
    }

    pub fn get_exports_tokens(&self) -> &FxHashSet<String> {
        &self.exports
    }

    pub fn get_metadata(&self) -> &dyn ModuleMetadata {
        &*self.metadata
    }

    pub fn _get_name(&self) -> &String {
        &self._name
    }

    pub fn _get_token(&self) -> &String {
        &self._token
    }

    pub fn _get_controller_by_token(
        &self,
        controller_token: &String,
    ) -> Option<&dyn ControllerFactory> {
        self.controllers
            .get(controller_token)
            .map(|controller| controller.as_ref())
    }

    pub fn _get_controllers_instances(&self) -> &FxHashMap<String, Arc<InstanceWrapper>> {
        &self.controllers_instances
    }

    pub fn _take_controllers_instances(&mut self) -> FxHashMap<String, Arc<InstanceWrapper>> {
        std::mem::take(&mut self.controllers_instances)
    }
}
