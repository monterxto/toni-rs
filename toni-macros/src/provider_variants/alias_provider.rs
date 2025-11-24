use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Result, Token,
    parse::{Parse, ParseStream},
};

use crate::shared::TokenType;

/// Parse provider_alias! macro input
/// Syntax: provider_alias!("ALIAS_TOKEN", "EXISTING_TOKEN")
/// or provider_alias!(AliasType, ExistingType)
///
/// This creates an alias that points to an existing provider,
/// similar to NestJS's useExisting pattern.
pub struct ProviderAliasInput {
    pub alias_token: TokenType,
    pub existing_token: TokenType,
}

impl Parse for ProviderAliasInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let alias_token: TokenType = input.parse()?;
        let _: Token![,] = input.parse()?;
        let existing_token: TokenType = input.parse()?;

        Ok(ProviderAliasInput {
            alias_token,
            existing_token,
        })
    }
}

pub fn handle_provider_alias(input: TokenStream) -> Result<TokenStream> {
    let ProviderAliasInput {
        alias_token,
        existing_token,
    } = syn::parse2(input)?;

    // Generate token expressions for runtime
    let alias_token_expr = alias_token.to_token_expr();
    let existing_token_expr = existing_token.to_token_expr();

    // Generate unique struct names based on alias token
    let token_display = alias_token.display_name();
    let sanitized_name = token_display.replace(['\"', ' ', '-', '.', ':', '/'], "_");
    let provider_name = format_ident!("__ToniAliasProvider_{}", sanitized_name);
    let manager_name = format_ident!("__ToniAliasProviderManager_{}", sanitized_name);

    // Generate the provider struct and implementation
    let expanded = quote! {
        {
            // Alias provider struct that references another provider
            #[derive(Clone)]
            struct #provider_name {
                target_provider: std::sync::Arc<Box<dyn toni::traits_helpers::ProviderTrait>>,
            }

            // Manager struct for Provider trait implementation
            struct #manager_name;

            // Implement ProviderTrait for the alias provider wrapper
            #[toni::async_trait]
            impl toni::traits_helpers::ProviderTrait for #provider_name {
                fn get_token(&self) -> String {
                    #alias_token_expr
                }

                fn get_token_manager(&self) -> String {
                    #alias_token_expr
                }

                fn get_scope(&self) -> toni::ProviderScope {
                    // Inherit scope from target provider
                    self.target_provider.get_scope()
                }

                async fn execute(
                    &self,
                    params: Vec<Box<dyn std::any::Any + Send>>,
                    req: Option<&toni::HttpRequest>,
                ) -> Box<dyn std::any::Any + Send> {
                    // Delegate to the target provider
                    self.target_provider.execute(params, req).await
                }

                // Delegate enhancer methods to target provider
                fn as_guard(&self) -> Option<std::sync::Arc<dyn toni::traits_helpers::Guard>> {
                    self.target_provider.as_guard()
                }

                fn as_interceptor(&self) -> Option<std::sync::Arc<dyn toni::traits_helpers::Interceptor>> {
                    self.target_provider.as_interceptor()
                }

                fn as_pipe(&self) -> Option<std::sync::Arc<dyn toni::traits_helpers::Pipe>> {
                    self.target_provider.as_pipe()
                }

                fn as_middleware(&self) -> Option<std::sync::Arc<dyn toni::traits_helpers::middleware::Middleware>> {
                    self.target_provider.as_middleware()
                }
            }

            // Implement Provider trait for the manager (used by module system)
            #[toni::async_trait]
            impl toni::traits_helpers::Provider for #manager_name {
                async fn get_all_providers(
                    &self,
                    _dependencies: &toni::FxHashMap<
                        String,
                        std::sync::Arc<Box<dyn toni::traits_helpers::ProviderTrait>>,
                    >,
                ) -> toni::FxHashMap<
                    String,
                    std::sync::Arc<Box<dyn toni::traits_helpers::ProviderTrait>>,
                > {
                    let mut providers = toni::FxHashMap::default();

                    // Find the existing provider
                    let existing_token = #existing_token_expr;
                    let target_provider = _dependencies
                        .get(&existing_token)
                        .expect(&format!(
                            "Provider alias target not found: {}. Make sure the target provider is registered before the alias.",
                            existing_token
                        ))
                        .clone();

                    // Create the alias provider wrapper
                    let provider_wrapper = #provider_name {
                        target_provider: target_provider.clone(),
                    };

                    // Register the alias with its token
                    let alias_token = #alias_token_expr;
                    providers.insert(
                        alias_token,
                        std::sync::Arc::new(
                            Box::new(provider_wrapper) as Box<dyn toni::traits_helpers::ProviderTrait>
                        ),
                    );

                    providers
                }

                fn get_name(&self) -> String {
                    #alias_token_expr
                }

                fn get_token(&self) -> String {
                    #alias_token_expr
                }

                fn get_dependencies(&self) -> Vec<String> {
                    // The alias depends on the existing provider
                    vec![#existing_token_expr]
                }
            }

            // Return the manager instance
            #manager_name
        }
    };

    Ok(expanded)
}
