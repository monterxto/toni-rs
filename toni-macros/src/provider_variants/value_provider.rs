use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Expr, Ident, Result, Token,
    parse::{Parse, ParseStream},
};

use crate::shared::TokenType;

/// Enhancer type flags
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EnhancerType {
    Guard,
    Interceptor,
    Pipe,
}

/// Parse provider_value! macro input
/// Syntax: provider_value!("TOKEN", value) or provider_value!(TOKEN, value)
/// Optional enhancers: provider_value!(TOKEN, value, guard) or provider_value!(TOKEN, value, guard, interceptor)
pub struct ProviderValueInput {
    pub token: TokenType,
    pub value_expr: Expr,
    pub enhancers: Vec<EnhancerType>,
}

impl Parse for ProviderValueInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let token: TokenType = input.parse()?;
        let _: Token![,] = input.parse()?;
        let value_expr: Expr = input.parse()?;

        // Parse optional enhancer flags
        let mut enhancers = Vec::new();
        while input.peek(Token![,]) {
            let _: Token![,] = input.parse()?;
            if input.is_empty() {
                break;
            }
            let ident: Ident = input.parse()?;
            let enhancer_str = ident.to_string();
            match enhancer_str.as_str() {
                "guard" => enhancers.push(EnhancerType::Guard),
                "interceptor" => enhancers.push(EnhancerType::Interceptor),
                "pipe" => enhancers.push(EnhancerType::Pipe),
                _ => {
                    return Err(syn::Error::new_spanned(
                        ident,
                        "Expected 'guard', 'interceptor', or 'pipe'",
                    ));
                }
            }
        }

        Ok(ProviderValueInput {
            token,
            value_expr,
            enhancers,
        })
    }
}

/// Generate enhancer method implementations based on the enhancer flags
fn generate_enhancer_methods(token: &TokenType, enhancers: &[EnhancerType]) -> Result<TokenStream> {
    // Only Type tokens can have enhancer support
    match token {
        TokenType::Type(_) => {
            // Type token - can generate enhancer methods
        }
        TokenType::String(_) | TokenType::Const(_) => {
            if !enhancers.is_empty() {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "Enhancer support (guard/interceptor/pipe) is only available for Type tokens, not String or Const tokens",
                ));
            }
            return Ok(quote! {});
        }
    };

    let mut methods = Vec::new();

    for enhancer in enhancers {
        match enhancer {
            EnhancerType::Guard => {
                methods.push(quote! {
                    fn as_guard(&self) -> Option<std::sync::Arc<dyn toni::traits_helpers::Guard>> {
                        Some(self.instance.clone() as std::sync::Arc<dyn toni::traits_helpers::Guard>)
                    }
                });
            }
            EnhancerType::Interceptor => {
                methods.push(quote! {
                    fn as_interceptor(&self) -> Option<std::sync::Arc<dyn toni::traits_helpers::Interceptor>> {
                        Some(self.instance.clone() as std::sync::Arc<dyn toni::traits_helpers::Interceptor>)
                    }
                });
            }
            EnhancerType::Pipe => {
                methods.push(quote! {
                    fn as_pipe(&self) -> Option<std::sync::Arc<dyn toni::traits_helpers::Pipe>> {
                        Some(self.instance.clone() as std::sync::Arc<dyn toni::traits_helpers::Pipe>)
                    }
                });
            }
        }
    }

    Ok(quote! {
        #(#methods)*
    })
}

pub fn handle_provider_value(input: TokenStream) -> Result<TokenStream> {
    let ProviderValueInput {
        token,
        value_expr,
        enhancers,
    } = syn::parse2(input)?;

    // Generate token expression for runtime
    let token_expr = token.to_token_expr();

    // Generate unique struct names based on token for this specific provider instance
    let token_display = token.display_name();
    let sanitized_name = token_display.replace(['\"', ' ', '-', '.', ':', '/'], "_");
    let provider_name = format_ident!("__ToniValueProvider_{}", sanitized_name);
    let manager_name = format_ident!("__ToniValueProviderManager_{}", sanitized_name);

    // Generate enhancer method implementations
    let enhancer_methods = generate_enhancer_methods(&token, &enhancers)?;

    // Generate different implementations based on token type
    let expanded = match &token {
        // For Type tokens: Store concrete type, no type erasure!
        TokenType::Type(path) => quote! {
            {
                // Value provider struct that stores the concrete type
                #[derive(Clone)]
                struct #provider_name {
                    instance: std::sync::Arc<#path>,
                }

                // Manager struct for Provider trait implementation
                struct #manager_name;

                // Implement ProviderTrait for the provider wrapper
                #[toni::async_trait]
                impl toni::traits_helpers::ProviderTrait for #provider_name {
                    fn get_token(&self) -> String {
                        #token_expr
                    }

                    fn get_token_manager(&self) -> String {
                        #token_expr
                    }

                    fn get_scope(&self) -> toni::ProviderScope {
                        toni::ProviderScope::Singleton
                    }

                    async fn execute(
                        &self,
                        _params: Vec<Box<dyn std::any::Any + Send>>,
                        _req: Option<&toni::HttpRequest>,
                    ) -> Box<dyn std::any::Any + Send> {
                        // Clone the concrete type directly - no type erasure!
                        Box::new((*self.instance).clone())
                    }

                    #enhancer_methods
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

                        // Create the value instance with concrete type
                        let value = #value_expr;
                        let instance = std::sync::Arc::new(value);

                        // Create the provider wrapper
                        let provider_wrapper = #provider_name { instance };

                        // Register the provider with its token
                        let token = #token_expr;
                        providers.insert(
                            token,
                            std::sync::Arc::new(
                                Box::new(provider_wrapper) as Box<dyn toni::traits_helpers::ProviderTrait>
                            ),
                        );

                        providers
                    }

                    fn get_name(&self) -> String {
                        #token_expr
                    }

                    fn get_token(&self) -> String {
                        #token_expr
                    }

                    fn get_dependencies(&self) -> Vec<String> {
                        Vec::new()
                    }
                }

                // Return the manager instance
                #manager_name
            }
        },

        // For String/Const tokens: Re-evaluate expression (we don't know the type)
        TokenType::String(_) | TokenType::Const(_) => quote! {
            {
                // Value provider struct that re-evaluates on each call
                #[derive(Clone)]
                struct #provider_name;

                // Manager struct for Provider trait implementation
                struct #manager_name;

                // Implement ProviderTrait for the provider wrapper
                #[toni::async_trait]
                impl toni::traits_helpers::ProviderTrait for #provider_name {
                    fn get_token(&self) -> String {
                        #token_expr
                    }

                    fn get_token_manager(&self) -> String {
                        #token_expr
                    }

                    fn get_scope(&self) -> toni::ProviderScope {
                        toni::ProviderScope::Singleton
                    }

                    async fn execute(
                        &self,
                        _params: Vec<Box<dyn std::any::Any + Send>>,
                        _req: Option<&toni::HttpRequest>,
                    ) -> Box<dyn std::any::Any + Send> {
                        // Re-evaluate the expression to get the concrete type
                        Box::new(#value_expr)
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

                        // Create the provider wrapper (no instance needed)
                        let provider_wrapper = #provider_name;

                        // Register the provider with its token
                        let token = #token_expr;
                        providers.insert(
                            token,
                            std::sync::Arc::new(
                                Box::new(provider_wrapper) as Box<dyn toni::traits_helpers::ProviderTrait>
                            ),
                        );

                        providers
                    }

                    fn get_name(&self) -> String {
                        #token_expr
                    }

                    fn get_token(&self) -> String {
                        #token_expr
                    }

                    fn get_dependencies(&self) -> Vec<String> {
                        Vec::new()
                    }
                }

                // Return the manager instance
                #manager_name
            }
        },
    };

    Ok(expanded)
}
