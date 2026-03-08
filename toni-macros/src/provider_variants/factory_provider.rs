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
/// Optional scope: provider_factory!("TOKEN", factory_fn, scope = "transient")
/// Optional enhancers: provider_factory!(TOKEN, factory_fn, guard)
/// Optional type hint for string/const tokens with enhancers: provider_factory!("TOKEN", factory_fn, Type, guard)
/// where factory_fn can be:
/// - || { value } - sync factory with no deps
/// - |dep1: Type1, dep2: Type2| { value } - sync factory with deps
/// - async || { value } - async factory with no deps
/// - async |dep1: Type1| { value } - async factory with deps
pub struct ProviderFactoryInput {
    pub token: TokenType,
    pub factory_expr: Expr,
    pub type_hint: Option<syn::Path>,
    pub scope: Option<String>,
    pub enhancers: Vec<EnhancerType>,
}

impl Parse for ProviderFactoryInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let token: TokenType = input.parse()?;
        let _: Token![,] = input.parse()?;
        let factory_expr: Expr = input.parse()?;

        // Parse optional type hint, scope, and enhancer flags
        let mut type_hint = None;
        let mut scope = None;
        let mut enhancers = Vec::new();

        while input.peek(Token![,]) {
            let _: Token![,] = input.parse()?;
            if input.is_empty() {
                break;
            }

            // Peek to determine if this is an enhancer keyword, scope, or type hint
            let lookahead = input.lookahead1();
            if lookahead.peek(Ident) {
                let ident: Ident = input.parse()?;
                let ident_str = ident.to_string();

                match ident_str.as_str() {
                    "guard" => enhancers.push(EnhancerType::Guard),
                    "interceptor" => enhancers.push(EnhancerType::Interceptor),
                    "pipe" => enhancers.push(EnhancerType::Pipe),
                    "scope" => {
                        // Parse: scope = "transient" or scope = "singleton" or scope = "request"
                        input.parse::<Token![=]>()?;
                        let scope_lit: syn::LitStr = input.parse()?;
                        scope = Some(scope_lit.value());
                    }
                    _ => {
                        // Not an enhancer keyword - could be start of a type hint
                        if type_hint.is_none() && enhancers.is_empty() && scope.is_none() {
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
                                "Type hint must come before scope and enhancer flags, or expected 'guard', 'interceptor', 'pipe', or 'scope'",
                            ));
                        }
                    }
                }
            } else {
                return Err(lookahead.error());
            }
        }

        Ok(ProviderFactoryInput {
            token,
            factory_expr,
            type_hint,
            scope,
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
        type_hint,
        scope,
        enhancers,
    } = syn::parse2(input)?;

    // Parse scope or default to Singleton
    let scope_expr = match scope.as_deref() {
        Some("request") => quote! { toni::ProviderScope::Request },
        Some("singleton") => quote! { toni::ProviderScope::Singleton },
        Some("transient") => quote! { toni::ProviderScope::Transient },
        None => quote! { toni::ProviderScope::Singleton }, // Default
        Some(other) => {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                format!(
                    "Invalid scope '{}'. Expected 'singleton', 'request', or 'transient'",
                    other
                ),
            ));
        }
    };

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
    let manager_name = format_ident!("__ToniFactoryProviderFactory_{}", sanitized_name);

    // Determine if we need to cache the instance (singleton scope or enhancers)
    let needs_caching = scope.as_deref() == Some("singleton") || !enhancers.is_empty();

    // Generate enhancer support for Type tokens with enhancers or singleton scope
    let (
        factory_struct_fields,
        factory_struct_init,
        factory_instance_field,
        enhancer_methods,
        execute_body,
    ) = generate_factory_enhancer_support(
        &token,
        &type_hint,
        &enhancers,
        &factory_expr,
        &dep_resolutions,
        &param_names,
        needs_caching,
        &factory_invocation,
    )?;

    let expanded = quote! {
        {
            struct #manager_name;

            #[toni::async_trait]
            impl toni::traits_helpers::ProviderFactory for #manager_name {
                fn get_token(&self) -> String {
                    #token_expr
                }

                fn get_dependencies(&self) -> Vec<String> {
                    vec![#(#dep_tokens),*]
                }

                async fn build(
                    &self,
                    _dependencies: toni::FxHashMap<
                        String,
                        std::sync::Arc<Box<dyn toni::traits_helpers::Provider>>,
                    >,
                ) -> std::sync::Arc<Box<dyn toni::traits_helpers::Provider>> {
                    #[derive(Clone)]
                    struct FactoryProviderWithDeps {
                        deps: std::sync::Arc<toni::FxHashMap<
                            String,
                            std::sync::Arc<Box<dyn toni::traits_helpers::Provider>>,
                        >>,
                        #factory_struct_fields
                    }

                    #[toni::async_trait]
                    impl toni::traits_helpers::Provider for FactoryProviderWithDeps {
                        fn get_token(&self) -> String {
                            #token_expr
                        }

                        fn get_token_manager(&self) -> String {
                            #token_expr
                        }

                        fn get_scope(&self) -> toni::ProviderScope {
                            #scope_expr
                        }

                        async fn execute(
                            &self,
                            _params: Vec<Box<dyn std::any::Any + Send>>,
                            _req: Option<&toni::HttpRequest>,
                        ) -> Box<dyn std::any::Any + Send> {
                            #execute_body
                        }

                        #enhancer_methods
                    }

                    #factory_struct_init

                    std::sync::Arc::new(Box::new(FactoryProviderWithDeps {
                        deps: std::sync::Arc::new(_dependencies),
                        #factory_instance_field
                    }) as Box<dyn toni::traits_helpers::Provider>)
                }
            }

            #manager_name
        }
    };

    Ok(expanded)
}

/// Generate enhancer support for factory providers
fn generate_factory_enhancer_support(
    token: &TokenType,
    type_hint: &Option<syn::Path>,
    enhancers: &[EnhancerType],
    factory_expr: &Expr,
    dep_resolutions: &[TokenStream],
    param_names: &[&syn::Ident],
    needs_caching: bool,
    factory_invocation: &TokenStream,
) -> Result<(
    TokenStream,
    TokenStream,
    TokenStream,
    TokenStream,
    TokenStream,
)> {
    if !needs_caching {
        // No caching needed - execute calls factory directly
        let execute_body = quote! {
            let _dependencies = &self.deps;
            let factory = #factory_expr;
            #factory_invocation
        };
        return Ok((quote! {}, quote! {}, quote! {}, quote! {}, execute_body));
    }

    // Determine the type path to use
    let path = match token {
        TokenType::Type(p) => p.clone(),
        TokenType::String(_) | TokenType::Const(_) => {
            if type_hint.is_none() {
                return Err(syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "provider_factory! with singleton scope or enhancers (guard/interceptor/pipe) for String or Const tokens requires a type hint. Use: provider_factory!(\"TOKEN\", factory_fn, Type, guard) or provider_factory!(\"TOKEN\", factory_fn, Type, scope = \"singleton\")",
                ));
            }
            type_hint.clone().unwrap()
        }
    };

    // Generate struct field for instance storage
    let struct_field = quote! {
        instance: std::sync::Arc<#path>,
    };

    // Generate initialization code for the instance
    // For sync factories with no deps, call directly to avoid type inference issues
    let is_async = is_async_expr(factory_expr);
    let has_deps = !dep_resolutions.is_empty();

    let struct_init = if is_async || has_deps {
        quote! {
            // Create instance (async or with dependencies)
            let factory = #factory_expr;
            let instance_result = async {
                let _req = None;
                #(#dep_resolutions)*
                factory(#(#param_names),*)
            }.await;
            let instance = std::sync::Arc::new(instance_result);
        }
    } else {
        quote! {
            // Create instance (sync, no dependencies)
            let factory = #factory_expr;
            let instance = std::sync::Arc::new(factory());
        }
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

    // Generate execute body that uses cached instance
    let execute_body = quote! {
        // Clone the inner value (requires T: Clone)
        // This is consistent with provider_value! behavior
        Box::new((*self.instance).clone())
    };

    Ok((
        struct_field,
        struct_init,
        instance_field,
        enhancer_methods,
        execute_body,
    ))
}
