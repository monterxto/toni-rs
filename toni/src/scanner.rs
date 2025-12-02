use std::{cell::RefCell, rc::Rc};

use anyhow::{Result, anyhow};

use crate::{
    injector::ToniContainer,
    module_helpers::module_enum::ModuleDefinition,
    traits_helpers::{MiddlewareConsumer, ModuleMetadata},
};

pub struct ToniDependenciesScanner {
    container: Rc<RefCell<ToniContainer>>,
}

impl ToniDependenciesScanner {
    pub fn new(container: Rc<RefCell<ToniContainer>>) -> Self {
        Self { container }
    }
    pub fn scan(&mut self, module: ModuleDefinition) -> Result<()> {
        self.scan_for_modules_with_imports(module)?;
        self.scan_modules_for_dependencies()?;
        Ok(())
    }
    fn scan_for_modules_with_imports(&mut self, module: ModuleDefinition) -> Result<()> {
        let mut ctx_registry: Vec<String> = vec![];

        let mut stack: Vec<ModuleDefinition> = vec![module];

        while let Some(current_module_definition) = stack.pop() {
            let ModuleDefinition::DefaultModule(default_module) = current_module_definition;

            ctx_registry.push(default_module.get_name());

            let modules_imported = default_module.imports().unwrap_or_default();

            let mut modules_imported_tokens = vec![];

            for module_imported in modules_imported {
                modules_imported_tokens.push(module_imported.get_id());

                if ctx_registry
                    .iter()
                    .any(|module_imported_id| module_imported_id == &module_imported.get_name())
                {
                    continue;
                }

                stack.push(ModuleDefinition::DefaultModule(module_imported));
            }
            let default_module_id = default_module.get_id();
            self.insert_module(default_module);
            self.insert_imports(default_module_id, modules_imported_tokens)?;
        }
        Ok(())
    }

    pub fn scan_modules_for_dependencies(&mut self) -> Result<()> {
        let modules_token = self.container.borrow().get_modules_token();
        for module_token in modules_token {
            self.insert_providers(module_token.clone())?;
            self.insert_controllers(module_token.clone())?;
            self.insert_exports(module_token.clone())?;
        }

        Ok(())
    }

    fn insert_module(&mut self, module: Box<dyn ModuleMetadata>) {
        let mut container = self.container.borrow_mut();
        container.add_module(module);
    }

    pub fn insert_imports(&mut self, module_token: String, imports: Vec<String>) -> Result<()> {
        let mut container = self.container.borrow_mut();

        for import in imports {
            container.add_import(&module_token, import)?;
        }

        Ok(())
    }

    pub fn insert_controllers(&mut self, module_token: String) -> Result<()> {
        let mut container = self.container.borrow_mut();
        let module_ref = container.get_module_by_token(&module_token);
        let resolved_module_ref = match module_ref {
            Some(module_ref) => module_ref,
            None => return Err(anyhow!("Module not found")),
        };

        let controllers = resolved_module_ref.get_metadata().controllers();

        if let Some(controllers) = controllers {
            for controller in controllers {
                container.add_controller(&module_token, controller)?;
            }
        };

        Ok(())
    }

    pub fn insert_providers(&mut self, module_token: String) -> Result<()> {
        let mut container = self.container.borrow_mut();
        let module_ref = container.get_module_by_token(&module_token);
        let resolved_module_ref = match module_ref {
            Some(module_ref) => module_ref,
            None => return Err(anyhow!("Module not found")),
        };

        let providers = resolved_module_ref.get_metadata().providers();

        if let Some(providers) = providers {
            for provider in providers {
                let provider_token = provider.get_token();

                // Detect APP_* token providers and register them separately
                match provider_token.as_str() {
                    "__TONI_APP_GUARD__" => {
                        let provider_type_token = provider.get_name();
                        container
                            .register_app_guard_provider(module_token.clone(), provider_type_token);
                    }
                    "__TONI_APP_INTERCEPTOR__" => {
                        let provider_type_token = provider.get_name();
                        container.register_app_interceptor_provider(
                            module_token.clone(),
                            provider_type_token,
                        );
                    }
                    "__TONI_APP_PIPE__" => {
                        let provider_type_token = provider.get_name();
                        container
                            .register_app_pipe_provider(module_token.clone(), provider_type_token);
                    }
                    _ => {}
                }

                container.add_provider(&module_token, provider)?;
            }
        };

        Ok(())
    }

    pub fn insert_exports(&mut self, module_token: String) -> Result<()> {
        let mut container = self.container.borrow_mut();
        let module_ref = container.get_module_by_token(&module_token);
        let resolved_module_ref = match module_ref {
            Some(module_ref) => module_ref,
            None => return Err(anyhow!("Module not found")),
        };

        let is_global = resolved_module_ref.get_metadata().is_global();
        let exports = resolved_module_ref.get_metadata().exports();

        if let Some(exports) = exports {
            for export in exports {
                container.add_export(&module_token, export.clone())?;

                // If module is global, register export token as globally available
                if is_global {
                    container.register_global_provider_token(export);
                }
            }
        };

        Ok(())
    }

    pub fn scan_middleware(&mut self) -> Result<()> {
        let modules_token = self.container.borrow().get_modules_token();
        for module_token in modules_token {
            self.register_module_middleware(&module_token)?;
        }
        Ok(())
    }

    fn register_module_middleware(&mut self, module_token: &str) -> Result<()> {
        let middleware_configs = {
            let container = self.container.borrow();

            let module_ref = container
                .get_module_by_token(&module_token.to_string())
                .ok_or_else(|| anyhow!("Module not found: {}", module_token))?;

            let metadata = module_ref.get_metadata();

            let mut consumer = MiddlewareConsumer::new();
            metadata.configure_middleware(&mut consumer);
            consumer.build()
        };

        let mut container_mut = self.container.borrow_mut();

        let middleware_manager = container_mut
            .get_middleware_manager_mut()
            .ok_or_else(|| anyhow!("Middleware manager not initialized"))?;

        for config in middleware_configs {
            middleware_manager.add_for_module(module_token.to_string(), config);
        }

        Ok(())
    }
}
