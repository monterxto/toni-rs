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
        let (providers, multi_providers) = {
            let container = self.container.borrow();
            let providers_map = container.get_providers_factory(&self.module_token)?;
            let providers = providers_map
                .iter()
                .map(|(token, provider)| (token.clone(), provider.get_dependencies()))
                .collect::<Vec<(String, Vec<String>)>>();
            // Map from multi-collection base token (e.g. "PLUGINS") to the contributing
            // provider tokens within this module so the topological sort can treat all
            // contributors as implicit dependencies of any provider that injects the base token.
            let multi_providers: FxHashMap<String, Vec<String>> = container
                .get_multi_providers()
                .iter()
                .map(|(base, contribs)| {
                    let local: Vec<String> = contribs
                        .iter()
                        .filter(|(mt, _)| mt == &self.module_token)
                        .map(|(_, pt)| pt.clone())
                        .collect();
                    (base.clone(), local)
                })
                .collect();
            (providers, multi_providers)
        };
        let clone_providers = providers.clone();
        for (token, dependencies) in providers {
            if !self.visited.contains_key(&token) {
                self.visit_node(token, dependencies, &clone_providers, &multi_providers)?;
            }
        }
        Ok(self.ordered)
    }

    fn visit_node(
        &mut self,
        token: String,
        dependencies: Vec<String>,
        providers: &Vec<(String, Vec<String>)>,
        multi_providers: &FxHashMap<String, Vec<String>>,
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
            if let Some((provider_token, provider_deps)) = providers
                .iter()
                .find(|(token, _)| token.as_str() == dep_token.as_str())
            {
                self.visit_node(
                    provider_token.clone(),
                    provider_deps.clone(),
                    providers,
                    multi_providers,
                )?;
            } else if let Some(contrib_tokens) = multi_providers.get(dep_token) {
                // dep_token is a multi-collection base token: visit all contributing
                // factories in this module first so the consumer is ordered after them.
                for contrib_token in contrib_tokens {
                    if let Some((_, contrib_deps)) =
                        providers.iter().find(|(t, _)| t == contrib_token)
                    {
                        self.visit_node(
                            contrib_token.clone(),
                            contrib_deps.clone(),
                            providers,
                            multi_providers,
                        )?;
                    }
                }
            }
        }

        self.temp_mark.remove(&token);
        self.visited.insert(token.clone(), true);
        self.ordered.push(token);
        Ok(())
    }
}
