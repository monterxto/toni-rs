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
/// Note: Scope cannot be overridden on aliases.
/// The alias inherits the scope from the target provider.
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
    let factory_name = format_ident!("__ToniAliasProviderFactory_{}", sanitized_name);

    // Generate the provider struct and implementation
    let expanded = quote! {
        {
            // Alias provider struct that references another provider
            #[derive(Clone)]
            struct #provider_name {
                target_provider: std::sync::Arc<Box<dyn toni::traits_helpers::Provider>>,
            }

            struct #factory_name;

            // Implement Provider for the alias provider wrapper
            #[toni::async_trait]
            impl toni::traits_helpers::Provider for #provider_name {
                fn get_token(&self) -> String {
                    #alias_token_expr
                }

                fn get_token_factory(&self) -> String {
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

            #[toni::async_trait]
            impl toni::traits_helpers::ProviderFactory for #factory_name {
                fn get_token(&self) -> String {
                    #alias_token_expr
                }

                fn get_dependencies(&self) -> Vec<String> {
                    vec![#existing_token_expr]
                }

                async fn build(
                    &self,
                    deps: toni::FxHashMap<
                        String,
                        std::sync::Arc<Box<dyn toni::traits_helpers::Provider>>,
                    >,
                ) -> std::sync::Arc<Box<dyn toni::traits_helpers::Provider>> {
                    let existing_token = #existing_token_expr;
                    let target_provider = deps
                        .get(&existing_token)
                        .expect(&format!(
                            "Provider alias target not found: {}. Make sure the target provider is registered before the alias.",
                            existing_token
                        ))
                        .clone();

                    std::sync::Arc::new(
                        Box::new(#provider_name { target_provider }) as Box<dyn toni::traits_helpers::Provider>
                    )
                }
            }

            #factory_name
        }
    };

    Ok(expanded)
}
