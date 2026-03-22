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
/// Optional type hint for string/const tokens with enhancers: provider_value!("TOKEN", value, Type, guard)
/// Note: Scope is NOT supported (values are always singleton)
pub struct ProviderValueInput {
    pub token: TokenType,
    pub value_expr: Expr,
    pub type_hint: Option<syn::Path>,
    pub enhancers: Vec<EnhancerType>,
}

impl Parse for ProviderValueInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let token: TokenType = input.parse()?;
        let _: Token![,] = input.parse()?;
        let value_expr: Expr = input.parse()?;

        // Parse optional type hint and enhancer flags
        let mut type_hint = None;
        let mut enhancers = Vec::new();

        while input.peek(Token![,]) {
            let _: Token![,] = input.parse()?;
            if input.is_empty() {
                break;
            }

            // Peek to determine if this is an enhancer keyword or type hint
            let lookahead = input.lookahead1();
            if lookahead.peek(Ident) {
                let ident: Ident = input.parse()?;
                let ident_str = ident.to_string();

                match ident_str.as_str() {
                    "guard" => enhancers.push(EnhancerType::Guard),
                    "interceptor" => enhancers.push(EnhancerType::Interceptor),
                    "pipe" => enhancers.push(EnhancerType::Pipe),
                    _ => {
                        // Not an enhancer keyword - could be start of a type hint
                        if type_hint.is_none() && enhancers.is_empty() {
                            // Parse as path (might be multi-segment like my_mod::Type)
                            let mut path_segments = syn::punctuated::Punctuated::new();
                            path_segments.push(syn::PathSegment::from(ident));

                            // Check for additional path segments (::Type)
                            while input.peek(Token![::]) {
                                input.parse::<Token![::]>()?;
                                let segment: Ident = input.parse()?;
                                path_segments.push(syn::PathSegment::from(segment));
                            }

                            type_hint = Some(syn::Path {
                                leading_colon: None,
                                segments: path_segments,
                            });
                        } else {
                            return Err(syn::Error::new_spanned(
                                ident,
                                "Type hint must come before enhancer flags, or expected 'guard', 'interceptor', or 'pipe'",
                            ));
                        }
                    }
                }
            } else {
                return Err(lookahead.error());
            }
        }

        Ok(ProviderValueInput {
            token,
            value_expr,
            type_hint,
            enhancers,
        })
    }
}

/// Generate enhancer method implementations based on the enhancer flags
fn generate_enhancer_methods(
    token: &TokenType,
    type_hint: &Option<syn::Path>,
    enhancers: &[EnhancerType],
) -> Result<TokenStream> {
    // Check if we can generate enhancer methods
    match token {
        TokenType::Type(_) => {
            // Type token - can generate enhancer methods
        }
        TokenType::String(_) | TokenType::Const(_) => {
            if !enhancers.is_empty() && type_hint.is_none() {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "Enhancer support (guard/interceptor/pipe) for String or Const tokens requires a type hint. Use: provider_value!(\"TOKEN\", value, Type, guard)",
                ));
            }
            if enhancers.is_empty() {
                return Ok(quote! {});
            }
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
        type_hint,
        enhancers,
    } = syn::parse2(input)?;

    // Generate token expression for runtime
    let token_expr = token.to_token_expr();

    // Generate unique struct names based on token for this specific provider instance
    let token_display = token.display_name();
    let sanitized_name = token_display.replace(['\"', ' ', '-', '.', ':', '/'], "_");
    let provider_name = format_ident!("__ToniValueProvider_{}", sanitized_name);
    let factory_name = format_ident!("__ToniValueProviderFactory_{}", sanitized_name);

    // Generate enhancer method implementations
    let enhancer_methods = generate_enhancer_methods(&token, &type_hint, &enhancers)?;

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

                struct #factory_name;

                // Implement Provider for the provider wrapper
                #[toni::async_trait]
                impl toni::traits_helpers::Provider for #provider_name {
                    fn get_token(&self) -> String {
                        #token_expr
                    }

                    fn get_token_factory(&self) -> String {
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

                #[toni::async_trait]
                impl toni::traits_helpers::ProviderFactory for #factory_name {
                    fn get_token(&self) -> String {
                        #token_expr
                    }

                    async fn build(
                        &self,
                        _deps: toni::FxHashMap<
                            String,
                            std::sync::Arc<Box<dyn toni::traits_helpers::Provider>>,
                        >,
                    ) -> std::sync::Arc<Box<dyn toni::traits_helpers::Provider>> {
                        let instance = std::sync::Arc::new(#value_expr);
                        std::sync::Arc::new(
                            Box::new(#provider_name { instance }) as Box<dyn toni::traits_helpers::Provider>
                        )
                    }
                }

                #factory_name
            }
        },

        TokenType::String(_) | TokenType::Const(_) => {
            if !enhancers.is_empty() {
                let type_path = type_hint.as_ref().unwrap();

                quote! {
                    {
                        #[derive(Clone)]
                        struct #provider_name {
                            instance: std::sync::Arc<#type_path>,
                        }

                        struct #factory_name;

                        #[toni::async_trait]
                        impl toni::traits_helpers::Provider for #provider_name {
                            fn get_token(&self) -> String {
                                #token_expr
                            }

                            fn get_token_factory(&self) -> String {
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
                                Box::new((*self.instance).clone())
                            }

                            #enhancer_methods
                        }

                        #[toni::async_trait]
                        impl toni::traits_helpers::ProviderFactory for #factory_name {
                            fn get_token(&self) -> String {
                                #token_expr
                            }

                            async fn build(
                                &self,
                                _deps: toni::FxHashMap<
                                    String,
                                    std::sync::Arc<Box<dyn toni::traits_helpers::Provider>>,
                                >,
                            ) -> std::sync::Arc<Box<dyn toni::traits_helpers::Provider>> {
                                let instance = std::sync::Arc::new(#value_expr);
                                std::sync::Arc::new(
                                    Box::new(#provider_name { instance }) as Box<dyn toni::traits_helpers::Provider>
                                )
                            }
                        }

                        #factory_name
                    }
                }
            } else {
                quote! {
                    {
                        struct #provider_name {
                            get_value: std::sync::Arc<dyn Fn() -> Box<dyn std::any::Any + Send> + Send + Sync>,
                        }

                        struct #factory_name;

                        #[toni::async_trait]
                        impl toni::traits_helpers::Provider for #provider_name {
                            fn get_token(&self) -> String {
                                #token_expr
                            }

                            fn get_token_factory(&self) -> String {
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
                                (self.get_value)()
                            }
                        }

                        #[toni::async_trait]
                        impl toni::traits_helpers::ProviderFactory for #factory_name {
                            fn get_token(&self) -> String {
                                #token_expr
                            }

                            async fn build(
                                &self,
                                _deps: toni::FxHashMap<
                                    String,
                                    std::sync::Arc<Box<dyn toni::traits_helpers::Provider>>,
                                >,
                            ) -> std::sync::Arc<Box<dyn toni::traits_helpers::Provider>> {
                                let value = std::sync::Arc::new(#value_expr);
                                let get_value = std::sync::Arc::new(move || {
                                    Box::new((*value).clone()) as Box<dyn std::any::Any + Send>
                                });
                                std::sync::Arc::new(
                                    Box::new(#provider_name { get_value }) as Box<dyn toni::traits_helpers::Provider>
                                )
                            }
                        }

                        #factory_name
                    }
                }
            }
        }
    };

    Ok(expanded)
}
