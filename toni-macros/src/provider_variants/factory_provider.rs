use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Expr, ExprClosure, Ident, Pat, Result, Token, Type,
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

/// Parse provider_factory! macro input
/// Syntax: provider_factory!("TOKEN", factory_fn) or provider_factory!(TOKEN, factory_fn)
/// Optional enhancers: provider_factory!(TOKEN, factory_fn, guard)
/// where factory_fn can be:
/// - || { value } - sync factory with no deps
/// - |dep1: Type1, dep2: Type2| { value } - sync factory with deps
/// - async || { value } - async factory with no deps
/// - async |dep1: Type1| { value } - async factory with deps
pub struct ProviderFactoryInput {
    pub token: TokenType,
    pub factory_expr: Expr,
    pub enhancers: Vec<EnhancerType>,
}

impl Parse for ProviderFactoryInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let token: TokenType = input.parse()?;
        let _: Token![,] = input.parse()?;
        let factory_expr: Expr = input.parse()?;

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

        Ok(ProviderFactoryInput {
            token,
            factory_expr,
            enhancers,
        })
    }
}

/// Extract dependencies from closure parameters
fn extract_closure_deps(closure: &ExprClosure) -> Vec<(syn::Ident, Type)> {
    let mut deps = Vec::new();

    for input in &closure.inputs {
        if let Pat::Type(pat_type) = input {
            // Extract parameter name
            if let Pat::Ident(pat_ident) = &*pat_type.pat {
                let param_name = pat_ident.ident.clone();
                let param_type = (*pat_type.ty).clone();
                deps.push((param_name, param_type));
            }
        }
    }

    deps
}

/// Check if an expression is an async closure or async block
fn is_async_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Async(_) => true,
        Expr::Closure(closure) => closure.asyncness.is_some(),
        _ => false,
    }
}

pub fn handle_provider_factory(input: TokenStream) -> Result<TokenStream> {
    let ProviderFactoryInput {
        token,
        factory_expr,
        enhancers,
    } = syn::parse2(input)?;

    // Generate token expression for runtime
    let token_expr = token.to_token_expr();

    // Detect if factory is async
    let is_async = is_async_expr(&factory_expr);

    // Extract dependencies if it's a closure
    let deps = if let Expr::Closure(ref closure) = factory_expr {
        extract_closure_deps(closure)
    } else {
        Vec::new()
    };

    // Generate dependency resolution code
    let dep_resolutions: Vec<_> = deps
        .iter()
        .map(|(param_name, param_type)| {
            let type_token = quote! { std::any::type_name::<#param_type>().to_string() };
            quote! {
                let #param_name = {
                    let provider = _dependencies
                        .get(&#type_token)
                        .expect(&format!("Dependency not found: {}", #type_token));
                    let instance = provider.execute(vec![], _req).await;
                    *instance
                        .downcast::<#param_type>()
                        .expect(&format!("Failed to downcast {}", #type_token))
                };
            }
        })
        .collect();

    let param_names: Vec<_> = deps.iter().map(|(name, _)| name).collect();

    // Generate the appropriate factory invocation based on async detection
    let factory_invocation = if deps.is_empty() {
        // No dependencies - simple invocation
        if is_async {
            quote! {
                {
                    let result = factory().await;
                    Box::new(result) as Box<dyn std::any::Any + Send>
                }
            }
        } else {
            quote! {
                {
                    let result = factory();
                    Box::new(result) as Box<dyn std::any::Any + Send>
                }
            }
        }
    } else {
        // With dependencies - resolve and pass them
        if is_async {
            quote! {
                {
                    #(#dep_resolutions)*
                    let result = factory(#(#param_names),*).await;
                    Box::new(result) as Box<dyn std::any::Any + Send>
                }
            }
        } else {
            quote! {
                {
                    #(#dep_resolutions)*
                    let result = factory(#(#param_names),*);
                    Box::new(result) as Box<dyn std::any::Any + Send>
                }
            }
        }
    };

    // Generate dependency tokens for get_dependencies()
    let dep_tokens: Vec<_> = deps
        .iter()
        .map(|(_, param_type)| {
            quote! { std::any::type_name::<#param_type>().to_string() }
        })
        .collect();

    // Generate unique struct names based on token
    let token_display = token.display_name();
    let sanitized_name = token_display.replace(['\"', ' ', '-', '.', ':', '/'], "_");
    let provider_name = format_ident!("__ToniFactoryProvider_{}", sanitized_name);
    let manager_name = format_ident!("__ToniFactoryProviderManager_{}", sanitized_name);

    // Generate enhancer support for Type tokens with enhancers
    let (factory_struct_fields, factory_struct_init, factory_instance_field, enhancer_methods) =
        generate_factory_enhancer_support(
            &token,
            &enhancers,
            &factory_expr,
            &dep_resolutions,
            &param_names,
        )?;

    // Generate the provider struct and implementation
    let expanded = quote! {
        {
            // Factory provider struct
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
                    // Factories are transient by default (create new instance each time)
                    toni::ProviderScope::Transient
                }

                async fn execute(
                    &self,
                    _params: Vec<Box<dyn std::any::Any + Send>>,
                    _req: Option<&toni::HttpRequest>,
                ) -> Box<dyn std::any::Any + Send> {
                    // This will be called by the DI system, but we need dependencies
                    // For now, just panic - proper resolution happens via get_all_providers
                    panic!("Factory providers must be resolved through get_all_providers with dependencies")
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

                    // Create a factory provider that captures dependencies
                    #[derive(Clone)]
                    struct FactoryProviderWithDeps {
                        deps: std::sync::Arc<toni::FxHashMap<
                            String,
                            std::sync::Arc<Box<dyn toni::traits_helpers::ProviderTrait>>,
                        >>,
                        #factory_struct_fields
                    }

                    #[toni::async_trait]
                    impl toni::traits_helpers::ProviderTrait for FactoryProviderWithDeps {
                        fn get_token(&self) -> String {
                            #token_expr
                        }

                        fn get_token_manager(&self) -> String {
                            #token_expr
                        }

                        fn get_scope(&self) -> toni::ProviderScope {
                            toni::ProviderScope::Transient
                        }

                        async fn execute(
                            &self,
                            _params: Vec<Box<dyn std::any::Any + Send>>,
                            _req: Option<&toni::HttpRequest>,
                        ) -> Box<dyn std::any::Any + Send> {
                            // Use captured dependencies
                            let _dependencies = &self.deps;
                            // Call the factory function with resolved dependencies
                            let factory = #factory_expr;
                            #factory_invocation
                        }

                        #enhancer_methods
                    }

                    // Register the provider with its token
                    let token = #token_expr;

                    // Initialize instance for enhancer support (only for Type tokens)
                    #factory_struct_init

                    let provider_instance = FactoryProviderWithDeps {
                        deps: std::sync::Arc::new(_dependencies.clone()),
                        #factory_instance_field
                    };

                    providers.insert(
                        token,
                        std::sync::Arc::new(
                            Box::new(provider_instance) as Box<dyn toni::traits_helpers::ProviderTrait>
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
                    vec![#(#dep_tokens),*]
                }
            }

            // Return the manager instance
            #manager_name
        }
    };

    Ok(expanded)
}

/// Generate enhancer support for factory providers
fn generate_factory_enhancer_support(
    token: &TokenType,
    enhancers: &[EnhancerType],
    factory_expr: &Expr,
    dep_resolutions: &[TokenStream],
    param_names: &[&syn::Ident],
) -> Result<(TokenStream, TokenStream, TokenStream, TokenStream)> {
    // Only Type tokens can have enhancer support
    let path = match token {
        TokenType::Type(p) => p,
        TokenType::String(_) | TokenType::Const(_) => {
            if !enhancers.is_empty() {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "Enhancer support (guard/interceptor/pipe) is only available for Type tokens, not String or Const tokens",
                ));
            }
            return Ok((quote! {}, quote! {}, quote! {}, quote! {}));
        }
    };

    if enhancers.is_empty() {
        // No enhancers - return empty tokens
        return Ok((quote! {}, quote! {}, quote! {}, quote! {}));
    }

    // Generate struct field for instance storage
    let struct_field = quote! {
        instance: std::sync::Arc<#path>,
    };

    // Generate initialization code for the instance
    let struct_init = quote! {
        // Create instance for enhancer support
        let factory = #factory_expr;
        let instance_result = async {
            let _req = None;
            #(#dep_resolutions)*
            factory(#(#param_names),*)
        }.await;
        let instance = std::sync::Arc::new(instance_result);
    };

    // Field initialization in struct literal
    let instance_field = quote! { instance, };

    // Generate enhancer methods
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

    let enhancer_methods = quote! {
        #(#methods)*
    };

    Ok((struct_field, struct_init, instance_field, enhancer_methods))
}
