use super::ToniContainer;
use anyhow::{Result, anyhow};
use rustc_hash::FxHashMap;
use std::{cell::RefCell, rc::Rc};

pub struct DependencyGraph {
    container: Rc<RefCell<ToniContainer>>,
    module_token: String,
    visited: FxHashMap<String, bool>,
    temp_mark: FxHashMap<String, bool>,
    ordered: Vec<String>,
}

impl DependencyGraph {
    pub fn new(container: Rc<RefCell<ToniContainer>>, module_token: String) -> Self {
        Self {
            container,
            module_token,
            visited: FxHashMap::default(),
            temp_mark: FxHashMap::default(),
            ordered: Vec::new(),
        }
    }

    pub fn get_ordered_providers_token(mut self) -> Result<Vec<String>> {
        let providers = {
            let container = self.container.borrow();
            let providers_map = container.get_providers_manager(&self.module_token)?;
            providers_map
                .iter()
                .map(|(token, provider)| (token.clone(), provider.get_dependencies()))
                .collect::<Vec<(String, Vec<String>)>>()
        };
        let clone_providers = providers.clone();
        for (token, dependencies) in providers {
            if !self.visited.contains_key(&token) {
                self.visit_node(token, dependencies, &clone_providers)?;
            }
        }
        Ok(self.ordered)
    }

    fn visit_node(
        &mut self,
        token: String,
        dependencies: Vec<String>,
        providers: &Vec<(String, Vec<String>)>,
    ) -> Result<()> {
        if self.temp_mark.contains_key(&token) {
            return Err(anyhow!(
                "Circular dependency detected for provider: {}",
                token
            ));
        }

        if self.visited.contains_key(&token) {
            return Ok(());
        }

        self.temp_mark.insert(token.clone(), true);

        for dep_token in &dependencies {
            // Find the provider that matches this dependency token
            // The dep_token is the full type name (e.g., "module_or_crate::TypeName")
            // and we need to find the provider whose token matches it
            if let Some((provider_token, provider_deps)) = providers
                .iter()
                .find(|(token, _)| token.as_str() == dep_token.as_str())
            {
                self.visit_node(provider_token.clone(), provider_deps.clone(), providers)?;
            }
        }

        self.temp_mark.remove(&token);
        self.visited.insert(token.clone(), true);
        self.ordered.push(token);
        Ok(())
    }
}
