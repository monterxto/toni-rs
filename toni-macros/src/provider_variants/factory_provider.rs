use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Expr, ExprClosure, Ident, Pat, Result, Token, Type,
    parse::{Parse, ParseStream},
};

use crate::shared::TokenType;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EnhancerType {
    Guard,
    Interceptor,
    Pipe,
}

pub struct ProviderFactoryInput {
    pub token: TokenType,
    pub factory_expr: Expr,
    pub scope: Option<String>,
    pub enhancers: Vec<EnhancerType>,
    pub lifecycle: bool,
}

impl Parse for ProviderFactoryInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let token: TokenType = input.parse()?;
        let _: Token![,] = input.parse()?;
        let factory_expr: Expr = input.parse()?;

        let mut scope = None;
        let mut enhancers = Vec::new();
        let mut lifecycle = false;

        while input.peek(Token![,]) {
            let _: Token![,] = input.parse()?;
            if input.is_empty() {
                break;
            }

            let lookahead = input.lookahead1();
            if lookahead.peek(Ident) {
                let ident: Ident = input.parse()?;
                let ident_str = ident.to_string();

                match ident_str.as_str() {
                    "guard" => enhancers.push(EnhancerType::Guard),
                    "interceptor" => enhancers.push(EnhancerType::Interceptor),
                    "pipe" => enhancers.push(EnhancerType::Pipe),
                    "lifecycle" => lifecycle = true,
                    "scope" => {
                        input.parse::<Token![=]>()?;
                        let scope_lit: syn::LitStr = input.parse()?;
                        scope = Some(scope_lit.value());
                    }
                    _ => {
                        // Type hint — parsed and discarded; no longer needed for type inference
                        let mut path_segments: syn::punctuated::Punctuated<syn::PathSegment, syn::token::PathSep> = syn::punctuated::Punctuated::new();
                        path_segments.push(syn::PathSegment::from(ident));
                        while input.peek(Token![::]) {
                            input.parse::<Token![::]>()?;
                            let segment: Ident = input.parse()?;
                            path_segments.push(syn::PathSegment::from(segment));
                        }
                        // Consume generic args if present (e.g. MyType<Foo>)
                        if input.peek(Token![<]) {
                            let _: syn::AngleBracketedGenericArguments = input.parse()?;
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
            scope,
            enhancers,
            lifecycle,
        })
    }
}

fn extract_closure_deps(closure: &ExprClosure) -> Vec<(syn::Ident, Type)> {
    let mut deps = Vec::new();
    for input in &closure.inputs {
        if let Pat::Type(pat_type) = input {
            if let Pat::Ident(pat_ident) = &*pat_type.pat {
                deps.push((pat_ident.ident.clone(), (*pat_type.ty).clone()));
            }
        }
    }
    deps
}

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
        scope,
        enhancers,
        lifecycle,
    } = syn::parse2(input)?;

    let scope_expr = match scope.as_deref() {
        Some("request") => quote! { toni::ProviderScope::Request },
        Some("singleton") => quote! { toni::ProviderScope::Singleton },
        Some("transient") => quote! { toni::ProviderScope::Transient },
        None => quote! { toni::ProviderScope::Singleton },
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

    if lifecycle && !enhancers.is_empty() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "lifecycle cannot be combined with guard/interceptor/pipe enhancers",
        ));
    }
    if lifecycle && matches!(scope.as_deref(), Some("request") | Some("transient")) {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "lifecycle is only compatible with singleton scope",
        ));
    }

    let token_expr = token.to_token_expr();
    let is_async = is_async_expr(&factory_expr);

    let deps = if let Expr::Closure(ref closure) = factory_expr {
        extract_closure_deps(closure)
    } else {
        Vec::new()
    };

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

    let factory_invocation = if deps.is_empty() {
        if is_async {
            quote! { { let result = factory().await; Box::new(result) as Box<dyn std::any::Any + Send> } }
        } else {
            quote! { { let result = factory(); Box::new(result) as Box<dyn std::any::Any + Send> } }
        }
    } else if is_async {
        quote! { { #(#dep_resolutions)* let result = factory(#(#param_names),*).await; Box::new(result) as Box<dyn std::any::Any + Send> } }
    } else {
        quote! { { #(#dep_resolutions)* let result = factory(#(#param_names),*); Box::new(result) as Box<dyn std::any::Any + Send> } }
    };

    let dep_tokens: Vec<_> = deps
        .iter()
        .map(|(_, param_type)| quote! { std::any::type_name::<#param_type>().to_string() })
        .collect();

    let token_display = token.display_name();
    let sanitized_name = token_display.replace(['\"', ' ', '-', '.', ':', '/'], "_");
    let factory_name = format_ident!("__ToniFactoryProviderFactory_{}", sanitized_name);

    let needs_caching = !matches!(scope.as_deref(), Some("request") | Some("transient"));

    let build_body = if !needs_caching {
        quote! {
            struct FactoryProviderWithDeps {
                deps: std::sync::Arc<toni::FxHashMap<
                    String,
                    std::sync::Arc<Box<dyn toni::traits_helpers::Provider>>,
                >>,
            }

            #[toni::async_trait]
            impl toni::traits_helpers::Provider for FactoryProviderWithDeps {
                fn get_token(&self) -> String { #token_expr }
                fn get_token_factory(&self) -> String { #token_expr }
                fn get_scope(&self) -> toni::ProviderScope { #scope_expr }

                async fn execute(
                    &self,
                    _params: Vec<Box<dyn std::any::Any + Send>>,
                    _req: Option<&toni::http_helpers::RequestPart>,
                ) -> Box<dyn std::any::Any + Send> {
                    let _dependencies = &self.deps;
                    let factory = #factory_expr;
                    #factory_invocation
                }
            }

            std::sync::Arc::new(Box::new(FactoryProviderWithDeps {
                deps: std::sync::Arc::new(_dependencies),
            }) as Box<dyn toni::traits_helpers::Provider>)
        }
    } else {
        let (type_bounds, struct_init, extra_methods, execute_body) =
            generate_caching_support(&enhancers, &factory_expr, &dep_resolutions, &param_names, lifecycle)?;

        quote! {
            struct FactoryProviderWithDeps<__T> {
                deps: std::sync::Arc<toni::FxHashMap<
                    String,
                    std::sync::Arc<Box<dyn toni::traits_helpers::Provider>>,
                >>,
                instance: std::sync::Arc<__T>,
            }

            #[toni::async_trait]
            impl<__T: #type_bounds> toni::traits_helpers::Provider for FactoryProviderWithDeps<__T> {
                fn get_token(&self) -> String { #token_expr }
                fn get_token_factory(&self) -> String { #token_expr }
                fn get_scope(&self) -> toni::ProviderScope { #scope_expr }

                async fn execute(
                    &self,
                    _params: Vec<Box<dyn std::any::Any + Send>>,
                    _req: Option<&toni::http_helpers::RequestPart>,
                ) -> Box<dyn std::any::Any + Send> {
                    #execute_body
                }

                #extra_methods
            }

            #struct_init

            std::sync::Arc::new(Box::new(FactoryProviderWithDeps {
                deps: std::sync::Arc::new(_dependencies),
                instance,
            }) as Box<dyn toni::traits_helpers::Provider>)
        }
    };

    let expanded = quote! {
        {
            struct #factory_name;

            #[toni::async_trait]
            impl toni::traits_helpers::ProviderFactory for #factory_name {
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
                    #build_body
                }
            }

            #factory_name
        }
    };

    Ok(expanded)
}

fn generate_caching_support(
    enhancers: &[EnhancerType],
    factory_expr: &Expr,
    dep_resolutions: &[TokenStream],
    param_names: &[&syn::Ident],
    lifecycle: bool,
) -> Result<(TokenStream, TokenStream, TokenStream, TokenStream)> {
    let is_async = is_async_expr(factory_expr);
    let has_deps = !dep_resolutions.is_empty();

    let type_bounds = if lifecycle {
        quote! { toni::traits_helpers::Provider + 'static }
    } else {
        let mut enhancer_bounds: Vec<TokenStream> = Vec::new();
        for enhancer in enhancers {
            let bound = match enhancer {
                EnhancerType::Guard => quote! { toni::traits_helpers::Guard },
                EnhancerType::Interceptor => quote! { toni::traits_helpers::Interceptor },
                EnhancerType::Pipe => quote! { toni::traits_helpers::Pipe },
            };
            enhancer_bounds.push(bound);
        }
        if enhancer_bounds.is_empty() {
            quote! { Clone + Send + Sync + 'static }
        } else {
            quote! { Clone + Send + Sync + 'static + #(#enhancer_bounds)+* }
        }
    };

    let factory_call = if is_async {
        quote! { factory(#(#param_names),*).await }
    } else {
        quote! { factory(#(#param_names),*) }
    };

    let struct_init = if is_async || has_deps {
        quote! {
            let factory = #factory_expr;
            let instance_raw = async {
                let _req = None;
                #(#dep_resolutions)*
                #factory_call
            }.await;
            let instance = std::sync::Arc::new(instance_raw);
        }
    } else {
        quote! {
            let factory = #factory_expr;
            let instance = std::sync::Arc::new(factory());
        }
    };

    let execute_body = if lifecycle {
        quote! { self.instance.execute(_params, _req).await }
    } else {
        quote! { Box::new((*self.instance).clone()) }
    };

    let extra_methods = if lifecycle {
        quote! {
            async fn on_module_init(&self) {
                self.instance.on_module_init().await;
            }
            async fn on_application_bootstrap(&self) {
                self.instance.on_application_bootstrap().await;
            }
            async fn on_module_destroy(&self) {
                self.instance.on_module_destroy().await;
            }
            async fn before_application_shutdown(&self, signal: Option<String>) {
                self.instance.before_application_shutdown(signal).await;
            }
            async fn on_application_shutdown(&self, signal: Option<String>) {
                self.instance.on_application_shutdown(signal).await;
            }
        }
    } else {
        let mut methods = Vec::new();
        for enhancer in enhancers {
            match enhancer {
                EnhancerType::Guard => methods.push(quote! {
                    fn as_guard(&self) -> Option<std::sync::Arc<dyn toni::traits_helpers::Guard>> {
                        Some(self.instance.clone())
                    }
                }),
                EnhancerType::Interceptor => methods.push(quote! {
                    fn as_interceptor(&self) -> Option<std::sync::Arc<dyn toni::traits_helpers::Interceptor>> {
                        Some(self.instance.clone())
                    }
                }),
                EnhancerType::Pipe => methods.push(quote! {
                    fn as_pipe(&self) -> Option<std::sync::Arc<dyn toni::traits_helpers::Pipe>> {
                        Some(self.instance.clone())
                    }
                }),
            }
        }
        quote! { #(#methods)* }
    };

    Ok((type_bounds, struct_init, extra_methods, execute_body))
}
