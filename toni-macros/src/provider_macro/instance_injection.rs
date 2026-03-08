//! Singleton Instance Injection Implementation
//!
//! Architecture:
//! 1. User struct with REAL fields (unchanged)
//! 2. `AppServiceProvider` (implements `Provider`) — holds `Arc<AppService>`, created once at startup
//! 3. `AppServiceProviderFactory` (implements `ProviderFactory`) — zero-sized descriptor; resolves
//!    deps and calls `build()` once to produce the `Provider` instance

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, ItemImpl, ItemStruct, Result, Type};

use crate::shared::{
    dependency_info::DependencyInfo,
    enhancer_markers::EnhancerMarkers,
    lifecycle_hooks::{LifecycleHooks, detect_lifecycle_hooks, strip_lifecycle_attrs},
    scope_parser::ProviderScope,
};

/// Detected enhancer traits that a struct implements
#[derive(Debug, Clone, Default)]
pub struct EnhancerTraits {
    pub is_guard: bool,
    pub is_interceptor: bool,
    pub is_pipe: bool,
    pub is_middleware: bool,
    pub is_error_handler: bool,
    pub is_gateway: bool,
}

/// Detect which enhancer traits a struct implements.
///
/// Checks marker attributes on the struct (`#[guard]`, `#[interceptor]`, etc.) and on the
/// impl block, as well as trait impl blocks for backwards compatibility.
fn detect_enhancer_traits(struct_attrs: &ItemStruct, impl_block: &ItemImpl) -> EnhancerTraits {
    let mut traits = EnhancerTraits::default();

    let markers = EnhancerMarkers::detect(struct_attrs);
    traits.is_guard = markers.is_guard;
    traits.is_interceptor = markers.is_interceptor;
    traits.is_middleware = markers.is_middleware;
    traits.is_pipe = markers.is_pipe;
    traits.is_error_handler = markers.is_error_handler;

    for attr in &impl_block.attrs {
        if let Some(ident) = attr.path().get_ident() {
            match ident.to_string().as_str() {
                "guard" => traits.is_guard = true,
                "interceptor" => traits.is_interceptor = true,
                "middleware" => traits.is_middleware = true,
                "pipe" => traits.is_pipe = true,
                "error_handler" => traits.is_error_handler = true,
                _ => {}
            }
        }
    }

    if let Some((_, path, _)) = &impl_block.trait_ {
        let trait_name = path
            .segments
            .last()
            .map(|seg| seg.ident.to_string())
            .unwrap_or_default();

        match trait_name.as_str() {
            "Guard" => traits.is_guard = true,
            "Interceptor" => traits.is_interceptor = true,
            "Pipe" => traits.is_pipe = true,
            "Middleware" => traits.is_middleware = true,
            "ErrorHandler" => traits.is_error_handler = true,
            _ => {}
        }
    }

    traits
}

/// Detect lifecycle hooks by scanning for method-level attributes in the impl block.
///
pub fn generate_instance_provider_system(
    struct_attrs: &ItemStruct,
    impl_block: &ItemImpl,
    dependencies: &DependencyInfo,
    scope: ProviderScope,
    is_gateway: bool,
) -> Result<TokenStream> {
    let struct_name = &struct_attrs.ident;

    let struct_with_clone = add_clone_derive(struct_attrs);

    // Remove #[inject] from constructor parameters, then strip lifecycle attributes
    let mut impl_def = impl_block.clone();
    for item in impl_def.items.iter_mut() {
        if let syn::ImplItem::Fn(method) = item {
            crate::markers_params::remove_marker_controller_fn::remove_marker_in_controller_fn_args(
                method,
            );
        }
    }
    let impl_def = strip_lifecycle_attrs(&impl_def);

    let mut enhancer_traits = detect_enhancer_traits(struct_attrs, impl_block);
    let lifecycle_hooks = detect_lifecycle_hooks(impl_block);
    enhancer_traits.is_gateway = is_gateway;

    let provider_wrapper = generate_provider_wrapper(
        struct_name,
        dependencies,
        scope,
        &enhancer_traits,
        &lifecycle_hooks,
    );

    let factory = generate_factory(struct_name, dependencies, scope);

    Ok(quote! {
        #[allow(dead_code)]
        #struct_with_clone

        #[allow(dead_code)]
        #impl_def

        #provider_wrapper
        #factory
    })
}

/// Adds Clone and Injectable derives to struct if needed
///
/// # Clone Detection
/// This function checks for `#[derive(Clone)]` attribute on the struct.
///
/// # Limitation: Manual `impl Clone`
/// This macro **cannot detect** manual `impl Clone` blocks that come after the macro invocation:
///
/// ```rust,ignore
/// #[injectable(pub struct Foo { field: String })]
/// impl Foo { /* ... */ }
///
/// // ❌ Macro cannot see this - will add #[derive(Clone)] and cause conflict
/// impl Clone for Foo {
///     fn clone(&self) -> Self { /* custom logic */ }
/// }
/// ```
///
/// This is an acceptable limitation because:
/// - Macros process attributes linearly and cannot look ahead to future impl blocks
/// - Compile errors are clear when conflicts occur
fn add_clone_derive(struct_attrs: &ItemStruct) -> ItemStruct {
    let mut struct_def = struct_attrs.clone();

    let has_clone = struct_def.attrs.iter().any(|attr| {
        if attr.path().is_ident("derive") {
            if let Ok(meta) = attr.parse_args::<syn::Meta>() {
                return meta_contains_clone(&meta);
            }
        }
        false
    });

    if !has_clone {
        // Add both Clone and Injectable derives
        // Injectable registers #[inject] and #[default] as valid attributes
        let derives: syn::Attribute = syn::parse_quote! {
            #[derive(Clone, ::toni::Injectable)]
        };
        struct_def.attrs.push(derives);
    } else {
        // Just add Injectable
        let injectable_derive: syn::Attribute = syn::parse_quote! {
            #[derive(::toni::Injectable)]
        };
        struct_def.attrs.push(injectable_derive);
    }

    struct_def
}

/// Recursively check if a derive meta contains Clone
fn meta_contains_clone(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(path) => path.is_ident("Clone"),
        syn::Meta::List(list) => {
            for nested in list
                .parse_args_with(
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                )
                .ok()
                .iter()
                .flatten()
            {
                if meta_contains_clone(nested) {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

fn generate_provider_wrapper(
    struct_name: &Ident,
    dependencies: &DependencyInfo,
    scope: ProviderScope,
    enhancer_traits: &EnhancerTraits,
    lifecycle_hooks: &LifecycleHooks,
) -> TokenStream {
    match scope {
        ProviderScope::Singleton => {
            generate_singleton_provider(struct_name, enhancer_traits, lifecycle_hooks)
        }
        ProviderScope::Request => {
            generate_request_provider(struct_name, dependencies, enhancer_traits)
        }
        ProviderScope::Transient => {
            generate_transient_provider(struct_name, dependencies, enhancer_traits)
        }
    }
}

fn generate_enhancer_methods(traits: &EnhancerTraits) -> TokenStream {
    let mut methods = Vec::new();

    if traits.is_guard {
        methods.push(quote! {
            fn as_guard(&self) -> Option<::std::sync::Arc<dyn ::toni::traits_helpers::Guard>> {
                Some(::std::sync::Arc::new((*self.instance).clone()))
            }
        });
    }
    if traits.is_interceptor {
        methods.push(quote! {
            fn as_interceptor(&self) -> Option<::std::sync::Arc<dyn ::toni::traits_helpers::Interceptor>> {
                Some(::std::sync::Arc::new((*self.instance).clone()))
            }
        });
    }
    if traits.is_pipe {
        methods.push(quote! {
            fn as_pipe(&self) -> Option<::std::sync::Arc<dyn ::toni::traits_helpers::Pipe>> {
                Some(::std::sync::Arc::new((*self.instance).clone()))
            }
        });
    }
    if traits.is_middleware {
        methods.push(quote! {
            fn as_middleware(&self) -> Option<::std::sync::Arc<dyn ::toni::traits_helpers::middleware::Middleware>> {
                Some(::std::sync::Arc::new((*self.instance).clone()))
            }
        });
    }
    if traits.is_error_handler {
        methods.push(quote! {
            fn as_error_handler(&self) -> Option<::std::sync::Arc<dyn ::toni::traits_helpers::ErrorHandler>> {
                Some(::std::sync::Arc::new((*self.instance).clone()))
            }
        });
    }

    if traits.is_gateway {
        methods.push(quote! {
            fn as_gateway(&self) -> Option<::std::sync::Arc<Box<dyn ::toni::websocket::GatewayTrait>>> {
                Some(::std::sync::Arc::new(Box::new((*self.instance).clone()) as Box<dyn ::toni::websocket::GatewayTrait>))
            }
        });
    }

    quote! { #(#methods)* }
}

/// Generate direct lifecycle method overrides on `Provider` for singleton providers.
///
/// Each override delegates to the user's annotated method on `self.instance`.
/// Signal-bearing hooks receive the signal as the second argument.
fn generate_lifecycle_direct_methods(hooks: &LifecycleHooks) -> TokenStream {
    let mut methods = Vec::new();

    if let Some(method) = &hooks.on_module_init {
        methods.push(quote! {
            async fn on_module_init(&self) {
                self.instance.#method().await;
            }
        });
    }
    if let Some(method) = &hooks.on_application_bootstrap {
        methods.push(quote! {
            async fn on_application_bootstrap(&self) {
                self.instance.#method().await;
            }
        });
    }
    if let Some(method) = &hooks.on_module_destroy {
        methods.push(quote! {
            async fn on_module_destroy(&self) {
                self.instance.#method().await;
            }
        });
    }
    if let Some(method) = &hooks.before_application_shutdown {
        methods.push(quote! {
            async fn before_application_shutdown(&self, signal: Option<String>) {
                self.instance.#method(signal).await;
            }
        });
    }
    if let Some(method) = &hooks.on_application_shutdown {
        methods.push(quote! {
            async fn on_application_shutdown(&self, signal: Option<String>) {
                self.instance.#method(signal).await;
            }
        });
    }

    quote! { #(#methods)* }
}

fn generate_singleton_provider(
    struct_name: &Ident,
    enhancer_traits: &EnhancerTraits,
    lifecycle_hooks: &LifecycleHooks,
) -> TokenStream {
    let provider_name = Ident::new(&format!("{}Provider", struct_name), struct_name.span());
    let enhancer_methods = generate_enhancer_methods(enhancer_traits);
    let lifecycle_methods = generate_lifecycle_direct_methods(lifecycle_hooks);

    quote! {
        struct #provider_name {
            instance: ::std::sync::Arc<#struct_name>,
        }

        #[::toni::async_trait]
        impl ::toni::traits_helpers::Provider for #provider_name {
            async fn execute(
                &self,
                _params: Vec<Box<dyn ::std::any::Any + Send>>,
                _req: Option<&::toni::http_helpers::HttpRequest>,
            ) -> Box<dyn ::std::any::Any + Send> {
                Box::new((*self.instance).clone())
            }

            fn get_token(&self) -> String {
                ::std::any::type_name::<#struct_name>().to_string()
            }

            fn get_token_factory(&self) -> String {
                ::std::any::type_name::<#struct_name>().to_string()
            }

            fn get_scope(&self) -> ::toni::ProviderScope {
                ::toni::ProviderScope::Singleton
            }

            #enhancer_methods
            #lifecycle_methods
        }
    }
}

fn generate_request_provider(
    struct_name: &Ident,
    dependencies: &DependencyInfo,
    enhancer_traits: &EnhancerTraits,
) -> TokenStream {
    let provider_name = Ident::new(&format!("{}Provider", struct_name), struct_name.span());
    let enhancer_methods = generate_enhancer_methods(enhancer_traits);

    let (field_resolutions, field_names) = generate_field_resolutions(dependencies);

    // Check if this uses from_request pattern
    let is_from_request = dependencies
        .init_method
        .as_ref()
        .map(|m| m == "from_request")
        .unwrap_or(false);

    // Generate struct instantiation code (either custom init or struct literal)
    let struct_instantiation = if let Some(init_fn) = &dependencies.init_method {
        let init_ident = syn::Ident::new(init_fn, struct_name.span());

        if is_from_request {
            // Special case: from_request gets HttpRequest as first parameter
            if field_names.is_empty() {
                // No dependencies, just HttpRequest
                quote! {
                    #struct_name::#init_ident(
                        _req.expect("from_request requires HttpRequest")
                    )
                }
            } else {
                // Has dependencies + HttpRequest
                quote! {
                    #struct_name::#init_ident(
                        _req.expect("from_request requires HttpRequest"),
                        #(#field_names),*
                    )
                }
            }
        } else {
            // Normal custom init
            quote! {
                #struct_name::#init_ident(#(#field_names),*)
            }
        }
    } else {
        let owned_field_inits: Vec<_> = dependencies
            .owned_fields
            .iter()
            .map(|(field_name, field_type, default_expr)| {
                if let Some(expr) = default_expr {
                    quote! { #field_name: #expr }
                } else {
                    quote! { #field_name: <#field_type>::default() }
                }
            })
            .collect();

        quote! {
            #struct_name {
                #(#field_names,)*
                #(#owned_field_inits),*
            }
        }
    };

    quote! {
        struct #provider_name {
            dependencies: ::toni::FxHashMap<
                String,
                ::std::sync::Arc<Box<dyn ::toni::traits_helpers::Provider>>
            >,
        }

        #[::toni::async_trait]
        impl ::toni::traits_helpers::Provider for #provider_name {
            async fn execute(
                &self,
                _params: Vec<Box<dyn ::std::any::Any + Send>>,
                _req: Option<&::toni::http_helpers::HttpRequest>,
            ) -> Box<dyn ::std::any::Any + Send> {
                // Resolve dependencies per request
                #(#field_resolutions)*

                // Create new instance per request
                let instance = #struct_instantiation;

                Box::new(instance)
            }

            fn get_token(&self) -> String {
                ::std::any::type_name::<#struct_name>().to_string()
            }

            fn get_token_factory(&self) -> String {
                ::std::any::type_name::<#struct_name>().to_string()
            }

            fn get_scope(&self) -> ::toni::ProviderScope {
                ::toni::ProviderScope::Request
            }

            #enhancer_methods
        }
    }
}

fn generate_transient_provider(
    struct_name: &Ident,
    dependencies: &DependencyInfo,
    enhancer_traits: &EnhancerTraits,
) -> TokenStream {
    let provider_name = Ident::new(&format!("{}Provider", struct_name), struct_name.span());
    let enhancer_methods = generate_enhancer_methods(enhancer_traits);

    let (field_resolutions, field_names) = generate_field_resolutions(dependencies);

    // Generate struct instantiation code (either custom init or struct literal)
    let struct_instantiation = if let Some(init_fn) = &dependencies.init_method {
        let init_ident = syn::Ident::new(init_fn, struct_name.span());
        quote! {
            #struct_name::#init_ident(#(#field_names),*)
        }
    } else {
        let owned_field_inits: Vec<_> = dependencies
            .owned_fields
            .iter()
            .map(|(field_name, field_type, default_expr)| {
                if let Some(expr) = default_expr {
                    quote! { #field_name: #expr }
                } else {
                    quote! { #field_name: <#field_type>::default() }
                }
            })
            .collect();

        quote! {
            #struct_name {
                #(#field_names,)*
                #(#owned_field_inits),*
            }
        }
    };

    quote! {
        struct #provider_name {
            dependencies: ::toni::FxHashMap<
                String,
                ::std::sync::Arc<Box<dyn ::toni::traits_helpers::Provider>>
            >,
        }

        #[::toni::async_trait]
        impl ::toni::traits_helpers::Provider for #provider_name {
            async fn execute(
                &self,
                _params: Vec<Box<dyn ::std::any::Any + Send>>,
                _req: Option<&::toni::http_helpers::HttpRequest>,
            ) -> Box<dyn ::std::any::Any + Send> {
                // Resolve dependencies every time
                #(#field_resolutions)*

                // Create new instance every time
                let instance = #struct_instantiation;

                Box::new(instance)
            }

            fn get_token(&self) -> String {
                ::std::any::type_name::<#struct_name>().to_string()
            }

            fn get_token_factory(&self) -> String {
                ::std::any::type_name::<#struct_name>().to_string()
            }

            fn get_scope(&self) -> ::toni::ProviderScope {
                ::toni::ProviderScope::Transient
            }

            #enhancer_methods
        }
    }
}

/// Generate field resolutions for Request/Transient providers (uses self.dependencies)
fn generate_field_resolutions(dependencies: &DependencyInfo) -> (Vec<TokenStream>, Vec<Ident>) {
    let mut resolutions = Vec::new();
    let mut field_names = Vec::new();

    // When a constructor is specified, resolve its parameters instead of struct fields
    let deps_to_resolve = if !dependencies.constructor_params.is_empty() {
        &dependencies.constructor_params
    } else {
        &dependencies.fields
    };

    // Group by token for deduplication while preserving declaration order
    use indexmap::IndexMap;
    let mut type_groups: IndexMap<String, Vec<(Ident, Type, TokenStream)>> = IndexMap::new();

    for (field_name, full_type, lookup_token_expr) in deps_to_resolve {
        // Group by token, not type - same type can map to different providers
        let type_key = quote!(#lookup_token_expr).to_string();
        type_groups.entry(type_key).or_insert_with(Vec::new).push((
            field_name.clone(),
            full_type.clone(),
            lookup_token_expr.clone(),
        ));
    }
    for (_type_key, fields_of_type) in type_groups {
        let (first_field_name, full_type, lookup_token_expr) = &fields_of_type[0];
        let field_name_str = first_field_name.to_string();

        if fields_of_type.len() == 1 {
            // Only one field of this type
            let field_name = first_field_name;
            let resolution = quote! {
                let #field_name: #full_type = {
                    let __lookup_token = #lookup_token_expr;
                    let provider = self.dependencies
                        .get(&__lookup_token)
                        .unwrap_or_else(|| panic!(
                            "Missing dependency '{}' for field '{}'",
                            __lookup_token, #field_name_str
                        ));

                    let any_box = provider.execute(vec![], _req).await;

                    *any_box.downcast::<#full_type>()
                        .unwrap_or_else(|_| panic!(
                            "Failed to downcast '{}' to {}",
                            __lookup_token,
                            stringify!(#full_type)
                        ))
                };
            };

            resolutions.push(resolution);
            field_names.push(field_name.clone());
        } else {
            // Multiple fields of the same type - deduplicate based on scope
            let temp_var = syn::Ident::new(
                &format!("__temp_instance_{}", first_field_name),
                first_field_name.span(),
            );
            let field_idents: Vec<_> = fields_of_type.iter().map(|(name, _, _)| name).collect();

            let field_declarations: Vec<TokenStream> = field_idents
                .iter()
                .map(|field_ident| {
                    quote! {
                        let #field_ident: #full_type;
                    }
                })
                .collect();

            let resolution = quote! {
                #(#field_declarations)*

                let __lookup_token = #lookup_token_expr;
                let provider = self.dependencies
                    .get(&__lookup_token)
                    .unwrap_or_else(|| panic!(
                        "Missing dependency '{}' for field '{}'",
                        __lookup_token, #field_name_str
                    ));

                if matches!(provider.get_scope(), ::toni::ProviderScope::Transient) {
                    #(
                        #field_idents = {
                            let any_box = provider.execute(vec![], _req).await;
                            *any_box.downcast::<#full_type>()
                                .unwrap_or_else(|_| panic!(
                                    "Failed to downcast '{}' to {}",
                                    __lookup_token,
                                    stringify!(#full_type)
                                ))
                        };
                    )*
                } else {
                    let #temp_var: #full_type = {
                        let any_box = provider.execute(vec![], _req).await;
                        *any_box.downcast::<#full_type>()
                            .unwrap_or_else(|_| panic!(
                                "Failed to downcast '{}' to {}",
                                __lookup_token,
                                stringify!(#full_type)
                            ))
                    };

                    #(
                        #field_idents = #temp_var.clone();
                    )*
                }
            };

            resolutions.push(resolution);
            for (field_name, _, _) in &fields_of_type {
                field_names.push(field_name.clone());
            }
        }
    }

    (resolutions, field_names)
}

/// Generate field resolutions for singleton factory (uses dependencies parameter)
fn generate_factory_field_resolutions(
    dependencies: &DependencyInfo,
) -> (Vec<TokenStream>, Vec<Ident>) {
    let mut resolutions = Vec::new();
    let mut field_names = Vec::new();

    // When a constructor is specified, resolve its parameters instead of struct fields
    let deps_to_resolve = if !dependencies.constructor_params.is_empty() {
        &dependencies.constructor_params
    } else {
        &dependencies.fields
    };

    // Group by token for deduplication while preserving declaration order
    use indexmap::IndexMap;
    let mut type_groups: IndexMap<String, Vec<(Ident, Type, TokenStream)>> = IndexMap::new();

    for (field_name, full_type, lookup_token_expr) in deps_to_resolve {
        // Group by token, not type - same type can map to different providers
        let type_key = quote!(#lookup_token_expr).to_string();
        type_groups.entry(type_key).or_insert_with(Vec::new).push((
            field_name.clone(),
            full_type.clone(),
            lookup_token_expr.clone(),
        ));
    }
    for (_type_key, fields_of_type) in type_groups {
        let (first_field_name, full_type, lookup_token_expr) = &fields_of_type[0];
        let field_name_str = first_field_name.to_string();

        if fields_of_type.len() == 1 {
            // Only one field of this type
            let field_name = first_field_name;
            let resolution = quote! {
                let #field_name: #full_type = {
                    let __lookup_token = #lookup_token_expr;
                    let provider = dependencies
                        .get(&__lookup_token)
                        .unwrap_or_else(|| panic!(
                            "Missing dependency '{}' for field '{}'",
                            __lookup_token, #field_name_str
                        ));

                    let any_box = provider.execute(vec![], None).await;

                    *any_box.downcast::<#full_type>()
                        .unwrap_or_else(|_| panic!(
                            "Failed to downcast '{}' to {}",
                            __lookup_token,
                            stringify!(#full_type)
                        ))
                };
            };

            resolutions.push(resolution);
            field_names.push(field_name.clone());
        } else {
            // Multiple fields of the same type - deduplicate based on scope
            let temp_var = syn::Ident::new(
                &format!("__temp_instance_{}", first_field_name),
                first_field_name.span(),
            );
            let field_idents: Vec<_> = fields_of_type.iter().map(|(name, _, _)| name).collect();

            let field_declarations: Vec<TokenStream> = field_idents
                .iter()
                .map(|field_ident| {
                    quote! {
                        let #field_ident: #full_type;
                    }
                })
                .collect();

            let resolution = quote! {
                #(#field_declarations)*

                let __lookup_token = #lookup_token_expr;
                let provider = dependencies
                    .get(&__lookup_token)
                    .unwrap_or_else(|| panic!(
                        "Missing dependency '{}' for field '{}'",
                        __lookup_token, #field_name_str
                    ));

                if matches!(provider.get_scope(), ::toni::ProviderScope::Transient) {
                    #(
                        #field_idents = {
                            let any_box = provider.execute(vec![], None).await;
                            *any_box.downcast::<#full_type>()
                                .unwrap_or_else(|_| panic!(
                                    "Failed to downcast '{}' to {}",
                                    __lookup_token,
                                    stringify!(#full_type)
                                ))
                        };
                    )*
                } else {
                    let #temp_var: #full_type = {
                        let any_box = provider.execute(vec![], None).await;
                        *any_box.downcast::<#full_type>()
                            .unwrap_or_else(|_| panic!(
                                "Failed to downcast '{}' to {}",
                                __lookup_token,
                                stringify!(#full_type)
                            ))
                    };

                    #(
                        #field_idents = #temp_var.clone();
                    )*
                }
            };

            resolutions.push(resolution);
            for (field_name, _, _) in &fields_of_type {
                field_names.push(field_name.clone());
            }
        }
    }

    (resolutions, field_names)
}

fn generate_factory(
    struct_name: &Ident,
    dependencies: &DependencyInfo,
    scope: ProviderScope,
) -> TokenStream {
    match scope {
        ProviderScope::Singleton => generate_singleton_factory(struct_name, dependencies),
        ProviderScope::Request => generate_request_factory(struct_name, dependencies),
        ProviderScope::Transient => generate_transient_factory(struct_name, dependencies),
    }
}

fn generate_singleton_factory(struct_name: &Ident, dependencies: &DependencyInfo) -> TokenStream {
    let factory_name = Ident::new(
        &format!("{}ProviderFactory", struct_name),
        struct_name.span(),
    );
    let provider_name = Ident::new(&format!("{}Provider", struct_name), struct_name.span());

    let (field_resolutions, field_names) = generate_factory_field_resolutions(dependencies);

    // Generate struct instantiation code (either custom init or struct literal)
    let struct_instantiation = if let Some(init_fn) = &dependencies.init_method {
        // Custom init method: MyService::new(dep1, dep2, ...)
        let init_ident = syn::Ident::new(init_fn, struct_name.span());
        quote! {
            #struct_name::#init_ident(#(#field_names),*)
        }
    } else {
        // Standard struct literal: MyService { dep1, dep2, field3: default, ... }
        let owned_field_inits: Vec<_> = dependencies
            .owned_fields
            .iter()
            .map(|(field_name, field_type, default_expr)| {
                if let Some(expr) = default_expr {
                    // User provided #[default(...)]
                    quote! { #field_name: #expr }
                } else {
                    // Fall back to Default trait
                    quote! { #field_name: <#field_type>::default() }
                }
            })
            .collect();

        quote! {
            #struct_name {
                #(#field_names,)*
                #(#owned_field_inits),*
            }
        }
    };

    // Collect dependency tokens from both constructor params (if using constructor injection)
    // and from #[inject] fields (if using field injection)
    let dependency_tokens: Vec<_> = dependencies
        .constructor_params
        .iter()
        .map(|(_, _, lookup_token_expr)| lookup_token_expr)
        .chain(
            dependencies
                .fields
                .iter()
                .map(|(_, _, lookup_token_expr)| lookup_token_expr),
        )
        .collect();

    // Generate scope validation code (Singleton cannot inject Request)
    // Check both constructor params and #[inject] fields
    let has_dependencies =
        !dependencies.constructor_params.is_empty() || !dependencies.fields.is_empty();
    let scope_validation = if has_dependencies {
        // Combine constructor params and fields for validation
        let constructor_dep_checks = dependencies.constructor_params.iter().map(
            |(param_name, _param_type, lookup_token_expr)| {
                let param_str = param_name.to_string();
                (
                    param_str,
                    quote! { "constructor parameter" },
                    lookup_token_expr,
                )
            },
        );

        let field_dep_checks =
            dependencies
                .fields
                .iter()
                .map(|(field_name, _full_type, lookup_token_expr)| {
                    let field_str = field_name.to_string();
                    (field_str, quote! { "field" }, lookup_token_expr)
                });

        let dep_checks: Vec<_> = constructor_dep_checks
            .chain(field_dep_checks)
            .map(|(dep_name, _dep_kind, lookup_token_expr)| {
                quote! {
                    {
                        let __lookup_token = #lookup_token_expr;
                        if let Some(provider) = dependencies.get(&__lookup_token) {
                            let dep_scope = provider.get_scope();
                            if matches!(dep_scope, ::toni::ProviderScope::Request) {
                                panic!(
                                    "\n❌ Scope validation error in provider '{}':\n\
                                     \n\
                                     Singleton-scoped providers cannot inject Request-scoped providers.\n\
                                     Dependency '{}' depends on '{}' which has Request scope.\n\
                                     \n\
                                     This restriction prevents data leakage across requests. Singleton providers\n\
                                     live for the entire application lifetime and would capture stale request data.\n\
                                     \n\
                                     Solutions:\n\
                                     1. Change '{}' to Request scope: #[injectable(scope = \"request\")]\n\
                                     2. Change '{}' to Singleton scope (if appropriate for your use case)\n\
                                     3. Pass request-specific data as method parameters instead of injecting\n\
                                     4. Extract data in controller (which has HttpRequest access) and pass it down\n\
                                     \n",
                                    ::std::any::type_name::<#struct_name>(),
                                    #dep_name,
                                    __lookup_token,
                                    ::std::any::type_name::<#struct_name>(),
                                    __lookup_token
                                );
                            }
                        }
                    }
                }
            })
            .collect();

        quote! {
            // Validate scope compatibility (runtime check at startup)
            #(#dep_checks)*
        }
    } else {
        quote! {}
    };

    quote! {
        pub struct #factory_name;

        #[::toni::async_trait]
        impl ::toni::traits_helpers::ProviderFactory for #factory_name {
            fn get_token(&self) -> String {
                ::std::any::type_name::<#struct_name>().to_string()
            }

            fn get_dependencies(&self) -> Vec<String> {
                vec![#(#dependency_tokens),*]
            }

            async fn build(
                &self,
                dependencies: ::toni::FxHashMap<
                    String,
                    ::std::sync::Arc<Box<dyn ::toni::traits_helpers::Provider>>
                >,
            ) -> ::std::sync::Arc<Box<dyn ::toni::traits_helpers::Provider>> {
                #scope_validation

                // Resolve all dependencies at startup
                #(#field_resolutions)*

                // Create the instance ONCE at startup
                let instance = ::std::sync::Arc::new({
                    #struct_instantiation
                });

                ::std::sync::Arc::new(Box::new(#provider_name { instance }) as Box<dyn ::toni::traits_helpers::Provider>)
            }
        }
    }
}

fn generate_request_factory(struct_name: &Ident, dependencies: &DependencyInfo) -> TokenStream {
    let factory_name = Ident::new(
        &format!("{}ProviderFactory", struct_name),
        struct_name.span(),
    );
    let provider_name = Ident::new(&format!("{}Provider", struct_name), struct_name.span());

    // Collect dependency tokens from both constructor params and #[inject] fields
    let dependency_tokens: Vec<_> = dependencies
        .constructor_params
        .iter()
        .map(|(_, _, lookup_token_expr)| lookup_token_expr)
        .chain(
            dependencies
                .fields
                .iter()
                .map(|(_, _, lookup_token_expr)| lookup_token_expr),
        )
        .collect();

    quote! {
        pub struct #factory_name;

        #[::toni::async_trait]
        impl ::toni::traits_helpers::ProviderFactory for #factory_name {
            fn get_token(&self) -> String {
                ::std::any::type_name::<#struct_name>().to_string()
            }

            fn get_dependencies(&self) -> Vec<String> {
                vec![#(#dependency_tokens),*]
            }

            async fn build(
                &self,
                dependencies: ::toni::FxHashMap<
                    String,
                    ::std::sync::Arc<Box<dyn ::toni::traits_helpers::Provider>>
                >,
            ) -> ::std::sync::Arc<Box<dyn ::toni::traits_helpers::Provider>> {
                ::std::sync::Arc::new(Box::new(#provider_name {
                    dependencies,
                }) as Box<dyn ::toni::traits_helpers::Provider>)
            }
        }
    }
}

fn generate_transient_factory(struct_name: &Ident, dependencies: &DependencyInfo) -> TokenStream {
    let factory_name = Ident::new(
        &format!("{}ProviderFactory", struct_name),
        struct_name.span(),
    );
    let provider_name = Ident::new(&format!("{}Provider", struct_name), struct_name.span());

    // Collect dependency tokens from both constructor params and #[inject] fields
    let dependency_tokens: Vec<_> = dependencies
        .constructor_params
        .iter()
        .map(|(_, _, lookup_token_expr)| lookup_token_expr)
        .chain(
            dependencies
                .fields
                .iter()
                .map(|(_, _, lookup_token_expr)| lookup_token_expr),
        )
        .collect();

    quote! {
        pub struct #factory_name;

        #[::toni::async_trait]
        impl ::toni::traits_helpers::ProviderFactory for #factory_name {
            fn get_token(&self) -> String {
                ::std::any::type_name::<#struct_name>().to_string()
            }

            fn get_dependencies(&self) -> Vec<String> {
                vec![#(#dependency_tokens),*]
            }

            async fn build(
                &self,
                dependencies: ::toni::FxHashMap<
                    String,
                    ::std::sync::Arc<Box<dyn ::toni::traits_helpers::Provider>>
                >,
            ) -> ::std::sync::Arc<Box<dyn ::toni::traits_helpers::Provider>> {
                ::std::sync::Arc::new(Box::new(#provider_name {
                    dependencies,
                }) as Box<dyn ::toni::traits_helpers::Provider>)
            }
        }
    }
}
