//! Controller Instance Injection Implementation
//!
//! Architecture:
//! 1. User struct with REAL fields (unchanged)
//! 2. Controller wrapper per handler method that lazily creates instances with dependency resolution
//! 3. Manager that returns controller wrappers
//!
//! Example transformation:
//! ```rust,ignore
//! // User code:
//! #[controller("/api", pub struct AppController { service: AppService })]
//! impl AppController {
//!     #[get("/info")]
//!     fn get_info(&self, req: HttpRequest) -> ToniBody {
//!         self.service.get_app_info()
//!     }
//! }
//!
//! // Generated:
//! #[derive(Clone)]
//! pub struct AppController {
//!     service: AppService
//! }
//!
//! impl AppController {
//!     fn get_info(&self, req: HttpRequest) -> ToniBody {
//!         self.service.get_app_info()
//!     }
//! }
//!
//! struct GetInfoController {
//!     dependencies: FxHashMap<String, Arc<Box<dyn ProviderTrait>>>
//! }
//!
//! impl ControllerTrait for GetInfoController {
//!     async fn handle(&self, req: HttpRequest) -> HttpResponse {
//!         // Resolve dependencies
//!         let service: AppService = *self.dependencies.get("AppService")
//!             .unwrap().execute(vec![]).await.downcast().unwrap();
//!
//!         // Create controller instance with real fields
//!         let controller = AppController { service };
//!
//!         // Call user's method on real struct
//!         controller.get_info(req)
//!     }
//! }
//! ```

use proc_macro2::TokenStream;
use quote::quote;
use std::collections::HashMap;
use syn::{
    Attribute, Error, Ident, ImplItemFn, ItemImpl, ItemStruct, LitStr, Result, spanned::Spanned,
};

use crate::{
    controller_macro::extractor_params::{
        ExtractorKind, generate_extractor_extractions, generate_extractor_method_call,
        generate_extractor_static_method_call, get_extractor_params, has_self_receiver,
    },
    enhancer::enhancer::{EnhancerInfo, create_enhancer_infos},
    markers_params::{
        extracts_marker_params::{
            extract_body_from_param, extract_path_param_from_param, extract_query_from_param,
        },
        get_marker_params::MarkerParam,
    },
    shared::{dependency_info::DependencyInfo, metadata_info::MetadataInfo},
    utils::controller_utils::attr_to_string,
};

pub fn generate_instance_controller_system(
    struct_attrs: &ItemStruct,
    impl_block: &ItemImpl,
    dependencies: &DependencyInfo,
    route_prefix: &str,
    scope: crate::shared::scope_parser::ControllerScope,
    was_explicit: bool,
) -> Result<TokenStream> {
    let struct_name = &struct_attrs.ident;

    // Add Clone derive to struct (required for creating instances)
    let struct_with_clone = add_clone_derive(struct_attrs);

    // Clone impl block and remove marker attributes from all methods
    let mut impl_def = impl_block.clone();
    for item in impl_def.items.iter_mut() {
        if let syn::ImplItem::Fn(method) = item {
            crate::markers_params::remove_marker_controller_fn::remove_marker_in_controller_fn_args(
                method,
            );
        }
    }

    // OPTIMIZATION: Conditionally generate wrappers based on scope and dependencies
    // Goal: Only generate wrappers that could actually be used

    let (singleton_wrappers, singleton_metadata, request_wrappers, request_metadata) =
        match (scope, was_explicit) {
            // Case 1: Explicit Request scope - only generate Request wrappers
            // No auto-elevation possible (already Request), so Singleton wrappers are dead code
            (crate::shared::scope_parser::ControllerScope::Request, true) => {
                let (req_wrappers, req_meta) = generate_controller_wrappers(
                    impl_block,
                    struct_name,
                    dependencies,
                    route_prefix,
                    crate::shared::scope_parser::ControllerScope::Request,
                )?;
                (vec![], vec![], req_wrappers, req_meta) // Skip Singleton wrappers!
            }

            // Case 2: Explicit or default Singleton - might need elevation
            _ => {
                let (sing_wrappers, sing_meta) = generate_controller_wrappers(
                    impl_block,
                    struct_name,
                    dependencies,
                    route_prefix,
                    crate::shared::scope_parser::ControllerScope::Singleton,
                )?;

                // Sub-optimization: Skip Request wrappers if no dependencies
                let (req_wrappers, req_meta) = if dependencies.fields.is_empty() {
                    (vec![], vec![]) // No deps = no elevation possible
                } else {
                    generate_controller_wrappers(
                        impl_block,
                        struct_name,
                        dependencies,
                        route_prefix,
                        crate::shared::scope_parser::ControllerScope::Request,
                    )?
                };

                (sing_wrappers, sing_meta, req_wrappers, req_meta)
            }
        };

    let manager = generate_manager(
        struct_name,
        singleton_metadata,
        request_metadata,
        dependencies,
        scope,
        was_explicit,
    );

    Ok(quote! {
        #[allow(dead_code)]
        #struct_with_clone

        #[allow(dead_code)]
        #impl_def

        // Generate Singleton wrappers (always)
        #(#singleton_wrappers)*

        // Generate Request wrappers (only if controller has dependencies)
        #(#request_wrappers)*

        #manager
    })
}

fn add_clone_derive(struct_attrs: &ItemStruct) -> ItemStruct {
    let mut struct_def = struct_attrs.clone();

    let has_clone = struct_def.attrs.iter().any(|attr| {
        if attr.path().is_ident("derive") {
            // Would need to parse derive contents properly
            false
        } else {
            false
        }
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

fn generate_controller_wrappers(
    impl_block: &ItemImpl,
    struct_name: &Ident,
    dependencies: &DependencyInfo,
    route_prefix: &str,
    scope: crate::shared::scope_parser::ControllerScope,
) -> Result<(Vec<TokenStream>, Vec<MetadataInfo>)> {
    let mut wrappers = Vec::new();
    let mut metadata_list = Vec::new();

    // Extract controller-level enhancers from impl block attributes
    let controller_enhancers_attr = get_enhancers_attr(&impl_block.attrs)?;

    for item in &impl_block.items {
        if let syn::ImplItem::Fn(method) = item {
            if let Some(http_method_attr) = find_http_method_attr(&method.attrs) {
                let method_enhancers_attr = get_enhancers_attr(&method.attrs)?;

                let marker_params = get_marker_params(method)?;

                let (wrapper, metadata) = generate_controller_wrapper(
                    method,
                    struct_name,
                    dependencies,
                    route_prefix,
                    http_method_attr,
                    controller_enhancers_attr.clone(),
                    method_enhancers_attr,
                    marker_params,
                    scope,
                )?;

                wrappers.push(wrapper);
                metadata_list.push(metadata);
            }
        }
    }

    Ok((wrappers, metadata_list))
}

fn find_http_method_attr(attrs: &[Attribute]) -> Option<&Attribute> {
    attrs.iter().find(|attr| {
        attr.path().is_ident("get")
            || attr.path().is_ident("post")
            || attr.path().is_ident("put")
            || attr.path().is_ident("delete")
            || attr.path().is_ident("patch")
            || attr.path().is_ident("head")
            || attr.path().is_ident("options")
    })
}

fn get_enhancers_attr(attrs: &[syn::Attribute]) -> Result<HashMap<&Ident, &Attribute>> {
    use crate::enhancer::enhancer::get_enhancers_attr as get_enhancers;
    get_enhancers(attrs)
}

fn get_marker_params(method: &ImplItemFn) -> Result<Vec<MarkerParam>> {
    use crate::markers_params::get_marker_params::get_marker_params as get_params;
    get_params(method)
}

fn generate_controller_wrapper(
    method: &ImplItemFn,
    struct_name: &Ident,
    dependencies: &DependencyInfo,
    route_prefix: &str,
    http_method_attr: &Attribute,
    controller_enhancers_attr: HashMap<&Ident, &Attribute>,
    method_enhancers_attr: HashMap<&Ident, &Attribute>,
    marker_params: Vec<MarkerParam>,
    scope: crate::shared::scope_parser::ControllerScope,
) -> Result<(TokenStream, MetadataInfo)> {
    let http_method = attr_to_string(http_method_attr)
        .map_err(|_| Error::new(http_method_attr.span(), "Invalid attribute format"))?;

    let route_path = http_method_attr
        .parse_args::<LitStr>()
        .map_err(|_| Error::new(http_method_attr.span(), "Invalid attribute format"))?
        .value();

    let full_route_path = join_paths(route_prefix, &route_path);

    let method_name = &method.sig.ident;
    // Include struct name to avoid collisions between controllers with same method names
    // Also include scope suffix to allow both Singleton and Request wrappers
    let scope_suffix = match scope {
        crate::shared::scope_parser::ControllerScope::Singleton => "",
        crate::shared::scope_parser::ControllerScope::Request => "Request",
    };
    let controller_name = Ident::new(
        &format!(
            "{}{}Controller{}",
            struct_name,
            capitalize_first(method_name.to_string()),
            scope_suffix
        ),
        method_name.span(),
    );
    let controller_token = controller_name.to_string();

    // Check if this is a static method (no self receiver)
    let is_static_method = !has_self_receiver(method);

    // Only generate field resolutions and struct instantiation for instance methods
    let (field_resolutions, _field_names, struct_instantiation) = if is_static_method {
        // Static method - no need for instance creation
        (vec![], vec![], quote! {})
    } else {
        let (resolutions, names) = generate_field_resolutions(dependencies);

        // Generate struct instantiation based on DI source
        let instantiation = if let Some(init_method_name) = &dependencies.init_method {
            // Constructor-based DI: call the constructor with resolved parameters
            let init_method = Ident::new(init_method_name, struct_name.span());
            quote! {
                let controller = #struct_name::#init_method(#(#names),*);
            }
        } else {
            // Field-based DI: use struct literal
            // Generate initializers for owned fields
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
                let controller = #struct_name {
                    #(#names,)*  // Injected dependencies
                    #(#owned_field_inits),*  // Owned fields with defaults
                };
            }
        };

        (resolutions, names, instantiation)
    };

    // Get enhancer infos for DI resolution
    let enhancer_infos = create_enhancer_infos(controller_enhancers_attr, method_enhancers_attr)?;

    // Check if we're using extractors or marker params
    let extractor_params = get_extractor_params(method)?;
    let has_extractors = extractor_params
        .iter()
        .any(|p| p.kind != ExtractorKind::HttpRequest && p.kind != ExtractorKind::Unknown);

    // Use extractor-based approach if:
    // 1. There are any actual extractors (Path, Query, Json, Body, Validated, Request), OR
    // 2. There are NO marker params (meaning user is not using legacy #param, #body, #query)
    let use_extractors = has_extractors || marker_params.is_empty();

    let (method_call, marker_params_extraction, body_dto_token_stream) = if use_extractors {
        // Use extractor-based approach
        let (extractions, call_args) = generate_extractor_extractions(&extractor_params)?;
        let method_call = if is_static_method {
            generate_extractor_static_method_call(method, struct_name, &call_args)?
        } else {
            generate_extractor_method_call(method, &call_args)?
        };
        (method_call, extractions, None)
    } else {
        // Use legacy marker-based approach
        let method_call =
            generate_method_call(method, &marker_params, struct_name, is_static_method)?;
        let (extractions, body_dto) = generate_marker_params_extraction(&marker_params)?;
        (method_call, extractions, body_dto)
    };

    let wrapper = generate_controller_wrapper_code(
        &controller_name,
        &controller_token,
        &full_route_path,
        &http_method,
        &field_resolutions,
        &struct_instantiation,
        &method_call,
        &enhancer_infos,
        &marker_params_extraction,
        &body_dto_token_stream,
        scope,
        struct_name, // Pass struct_name for downcast in singleton wrapper
        is_static_method,
    );

    let controller_dependencies: Vec<(Ident, TokenStream)> = dependencies
        .fields
        .iter()
        .map(|(field_name, _full_type, lookup_token_expr)| {
            let dep_field_name = Ident::new(&format!("{}_dep", field_name), field_name.span());
            (dep_field_name, lookup_token_expr.clone())
        })
        .collect();

    Ok((
        wrapper,
        MetadataInfo {
            struct_name: controller_name,
            dependencies: controller_dependencies,
            is_static: is_static_method,
        },
    ))
}

fn generate_field_resolutions(dependencies: &DependencyInfo) -> (Vec<TokenStream>, Vec<Ident>) {
    let mut resolutions = Vec::new();
    let mut field_names = Vec::new();

    // When a constructor is specified, resolve its parameters instead of struct fields
    let deps_to_resolve = if !dependencies.constructor_params.is_empty() {
        &dependencies.constructor_params
    } else {
        &dependencies.fields
    };

    // Process each dependency individually
    for (field_name, full_type, lookup_token_expr) in deps_to_resolve {
        let resolution = quote! {
            let #field_name: #full_type = {
                let __lookup_token = #lookup_token_expr;
                let provider = self.dependencies
                    .get(&__lookup_token)
                    .unwrap_or_else(|| panic!("Missing dependency '{}'", __lookup_token));

                let any_box = provider.execute(vec![], Some(&req)).await;

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
    }

    (resolutions, field_names)
}

// NOTE: Old deduplication logic was removed since TokenStream can't be easily compared
// If deduplication is needed in the future, we would need to:
// 1. Store type information separately from the runtime token generation
// 2. Group by static type information at compile time
// 3. Generate runtime token expressions for each grouped field

fn generate_method_call(
    method: &ImplItemFn,
    marker_params: &[MarkerParam],
    struct_name: &Ident,
    is_static: bool,
) -> Result<TokenStream> {
    let method_name = &method.sig.ident;
    let is_async = method.sig.asyncness.is_some();

    // Check if method has any non-marker parameters (like HttpRequest)
    let mut call_args = Vec::new();

    // Iterate through the method signature to build call args in order
    for input in method.sig.inputs.iter() {
        if let syn::FnArg::Typed(pat_type) = input {
            if let syn::Pat::Ident(pat_ident) = &*pat_type.pat {
                let param_name = &pat_ident.ident;

                // Check if this is a marker param
                let is_marker = marker_params.iter().any(|mp| mp.param_name == *param_name);

                if is_marker {
                    // Use the extracted marker param value
                    call_args.push(quote! { #param_name });
                } else {
                    // Check if it's HttpRequest type
                    if let syn::Type::Path(type_path) = &*pat_type.ty {
                        if let Some(segment) = type_path.path.segments.last() {
                            if segment.ident == "HttpRequest" {
                                call_args.push(quote! { req });
                            } else {
                                // Unknown non-marker parameter
                                call_args.push(quote! { #param_name });
                            }
                        }
                    }
                }
            }
        }
    }

    let call = if is_static {
        quote! { #struct_name::#method_name(#(#call_args),*) }
    } else {
        quote! { controller.#method_name(#(#call_args),*) }
    };

    Ok(if is_async {
        quote! { #call.await }
    } else {
        call
    })
}

fn generate_marker_params_extraction(
    marker_params: &[MarkerParam],
) -> Result<(Vec<TokenStream>, Option<TokenStream>)> {
    let mut extractions = Vec::new();
    let body_dto_token_stream = None;

    for marker_param in marker_params {
        match marker_param.marker_name.as_str() {
            "body" => {
                // New extractor-based approach handles this internally
                extractions.push(extract_body_from_param(marker_param)?);
            }
            "query" => {
                extractions.push(extract_query_from_param(marker_param)?);
            }
            "param" => {
                extractions.push(extract_path_param_from_param(marker_param)?);
            }
            _ => {}
        }
    }

    Ok((extractions, body_dto_token_stream))
}

fn generate_controller_wrapper_code(
    controller_name: &Ident,
    controller_token: &str,
    full_route_path: &str,
    http_method: &str,
    field_resolutions: &[TokenStream],
    struct_instantiation: &TokenStream,
    method_call: &TokenStream,
    enhancer_infos: &HashMap<String, Vec<EnhancerInfo>>,
    marker_params_extraction: &[TokenStream],
    body_dto_token_stream: &Option<TokenStream>,
    scope: crate::shared::scope_parser::ControllerScope,
    struct_name: &Ident,
    is_static_method: bool,
) -> TokenStream {
    use crate::shared::scope_parser::ControllerScope;

    match scope {
        ControllerScope::Singleton => generate_singleton_controller_wrapper(
            controller_name,
            controller_token,
            full_route_path,
            http_method,
            method_call,
            enhancer_infos,
            marker_params_extraction,
            body_dto_token_stream,
            struct_name, // Pass struct name for downcast
            is_static_method,
        ),
        ControllerScope::Request => generate_request_controller_wrapper(
            controller_name,
            controller_token,
            full_route_path,
            http_method,
            field_resolutions,
            struct_instantiation,
            method_call,
            enhancer_infos,
            marker_params_extraction,
            body_dto_token_stream,
            is_static_method,
        ),
    }
}

// Singleton controller (stores Arc<ControllerInstance> created at startup)
fn generate_singleton_controller_wrapper(
    controller_name: &Ident,
    controller_token: &str,
    full_route_path: &str,
    http_method: &str,
    method_call: &TokenStream,
    enhancer_infos: &HashMap<String, Vec<EnhancerInfo>>,
    marker_params_extraction: &[TokenStream],
    body_dto_token_stream: &Option<TokenStream>,
    struct_name: &Ident, // Need this for downcast type
    is_static_method: bool,
) -> TokenStream {
    let binding = Vec::new();

    // Generate enhancer token expressions for DI resolution
    // Filter out empty tokens (from direct instantiation syntax)
    let guard_tokens: Vec<_> = enhancer_infos
        .get("guards")
        .unwrap_or(&binding)
        .iter()
        .filter(|info| !info.token_expr.is_empty())
        .map(|info| &info.token_expr)
        .collect();

    let interceptor_tokens: Vec<_> = enhancer_infos
        .get("interceptors")
        .unwrap_or(&binding)
        .iter()
        .filter(|info| !info.token_expr.is_empty())
        .map(|info| &info.token_expr)
        .collect();

    let pipe_tokens: Vec<_> = enhancer_infos
        .get("pipes")
        .unwrap_or(&binding)
        .iter()
        .filter(|info| !info.token_expr.is_empty())
        .map(|info| &info.token_expr)
        .collect();

    // Generate direct instantiation expressions for fallback (when not in DI)
    // Only include enhancers with explicit constructor calls (instance_expr is non-empty)
    let guard_instances: Vec<_> = enhancer_infos
        .get("guards")
        .unwrap_or(&binding)
        .iter()
        .filter(|info| !info.instance_expr.is_empty())
        .map(|info| &info.instance_expr)
        .collect();

    let interceptor_instances: Vec<_> = enhancer_infos
        .get("interceptors")
        .unwrap_or(&binding)
        .iter()
        .filter(|info| !info.instance_expr.is_empty())
        .map(|info| &info.instance_expr)
        .collect();

    let pipe_instances: Vec<_> = enhancer_infos
        .get("pipes")
        .unwrap_or(&binding)
        .iter()
        .filter(|info| !info.instance_expr.is_empty())
        .map(|info| &info.instance_expr)
        .collect();

    let body_dto_stream = if let Some(token_stream) = body_dto_token_stream {
        token_stream.clone()
    } else {
        quote! { None }
    };

    // For static methods, we don't need to store or downcast the instance
    let (struct_fields, instance_downcast) = if is_static_method {
        (
            quote! {
                // Static method: no instance needed
            },
            quote! {
                // Static method: call directly on struct type, no instance needed
            },
        )
    } else {
        (
            quote! {
                // Singleton: Store the pre-created controller instance!
                instance: ::std::sync::Arc<dyn ::std::any::Any + Send + Sync>,
            },
            quote! {
                // Downcast the Arc<dyn Any> to the actual controller type
                let controller = self.instance
                    .downcast_ref::<#struct_name>()
                    .expect("Failed to downcast controller instance");
            },
        )
    };

    quote! {
        struct #controller_name {
            #struct_fields
        }

        #[::toni::async_trait]
        impl ::toni::traits_helpers::ControllerTrait for #controller_name {
            async fn execute(
                &self,
                req: ::toni::http_helpers::HttpRequest,
            ) -> Box<dyn ::toni::http_helpers::IntoResponse<Response = ::toni::http_helpers::HttpResponse> + Send> {
                // NO dependency resolution here!
                // NO controller instantiation here!
                // Just extract parameters and call the handler

                #(#marker_params_extraction)*

                #instance_downcast

                let result = #method_call;
                Box::new(result)
            }

            fn get_method(&self) -> ::toni::http_helpers::HttpMethod {
                ::toni::http_helpers::HttpMethod::from_string(#http_method).unwrap()
            }

            fn get_path(&self) -> String {
                #full_route_path.to_string()
            }

            fn get_token(&self) -> String {
                #controller_token.to_string()
            }

            fn get_guards(&self) -> Vec<::std::sync::Arc<dyn ::toni::traits_helpers::Guard>> {
                // Direct instantiation fallback for enhancers not in DI
                vec![#(::std::sync::Arc::new(#guard_instances)),*]
            }

            fn get_interceptors(&self) -> Vec<::std::sync::Arc<dyn ::toni::traits_helpers::Interceptor>> {
                // Direct instantiation fallback for enhancers not in DI
                vec![#(::std::sync::Arc::new(#interceptor_instances)),*]
            }

            fn get_pipes(&self) -> Vec<::std::sync::Arc<dyn ::toni::traits_helpers::Pipe>> {
                // Direct instantiation fallback for enhancers not in DI
                vec![#(::std::sync::Arc::new(#pipe_instances)),*]
            }

            fn get_guard_tokens(&self) -> Vec<String> {
                vec![#(#guard_tokens),*]
            }

            fn get_interceptor_tokens(&self) -> Vec<String> {
                vec![#(#interceptor_tokens),*]
            }

            fn get_pipe_tokens(&self) -> Vec<String> {
                vec![#(#pipe_tokens),*]
            }

            fn get_body_dto(&self, _req: &::toni::http_helpers::HttpRequest) -> Option<Box<dyn ::toni::traits_helpers::validate::Validatable>> {
                #body_dto_stream
            }
        }
    }
}

// Request-scoped controller (creates instance per request)
fn generate_request_controller_wrapper(
    controller_name: &Ident,
    controller_token: &str,
    full_route_path: &str,
    http_method: &str,
    field_resolutions: &[TokenStream],
    struct_instantiation: &TokenStream,
    method_call: &TokenStream,
    enhancer_infos: &HashMap<String, Vec<EnhancerInfo>>,
    marker_params_extraction: &[TokenStream],
    body_dto_token_stream: &Option<TokenStream>,
    is_static_method: bool,
) -> TokenStream {
    let binding = Vec::new();

    // Generate enhancer token expressions for DI resolution
    // Filter out empty tokens (from direct instantiation syntax)
    let guard_tokens: Vec<_> = enhancer_infos
        .get("guards")
        .unwrap_or(&binding)
        .iter()
        .filter(|info| !info.token_expr.is_empty())
        .map(|info| &info.token_expr)
        .collect();

    let interceptor_tokens: Vec<_> = enhancer_infos
        .get("interceptors")
        .unwrap_or(&binding)
        .iter()
        .filter(|info| !info.token_expr.is_empty())
        .map(|info| &info.token_expr)
        .collect();

    let pipe_tokens: Vec<_> = enhancer_infos
        .get("pipes")
        .unwrap_or(&binding)
        .iter()
        .filter(|info| !info.token_expr.is_empty())
        .map(|info| &info.token_expr)
        .collect();

    // Generate direct instantiation expressions for fallback (when not in DI)
    // Only include enhancers with explicit constructor calls (instance_expr is non-empty)
    let guard_instances: Vec<_> = enhancer_infos
        .get("guards")
        .unwrap_or(&binding)
        .iter()
        .filter(|info| !info.instance_expr.is_empty())
        .map(|info| &info.instance_expr)
        .collect();

    let interceptor_instances: Vec<_> = enhancer_infos
        .get("interceptors")
        .unwrap_or(&binding)
        .iter()
        .filter(|info| !info.instance_expr.is_empty())
        .map(|info| &info.instance_expr)
        .collect();

    let pipe_instances: Vec<_> = enhancer_infos
        .get("pipes")
        .unwrap_or(&binding)
        .iter()
        .filter(|info| !info.instance_expr.is_empty())
        .map(|info| &info.instance_expr)
        .collect();

    let body_dto_stream = if let Some(token_stream) = body_dto_token_stream {
        token_stream.clone()
    } else {
        quote! { None }
    };

    // For static methods, we don't need dependencies field
    let struct_fields = if is_static_method {
        quote! {
            // Static method: no dependencies needed
        }
    } else {
        quote! {
            dependencies: ::toni::FxHashMap<
                String,
                ::std::sync::Arc<Box<dyn ::toni::traits_helpers::ProviderTrait>>
            >,
        }
    };

    quote! {
        struct #controller_name {
            #struct_fields
        }

        #[::toni::async_trait]
        impl ::toni::traits_helpers::ControllerTrait for #controller_name {
            async fn execute(
                &self,
                req: ::toni::http_helpers::HttpRequest,
            ) -> Box<dyn ::toni::http_helpers::IntoResponse<Response = ::toni::http_helpers::HttpResponse> + Send> {
                #(#field_resolutions)*
                #(#marker_params_extraction)*
                #struct_instantiation

                let result = #method_call;
                Box::new(result)
            }

            fn get_method(&self) -> ::toni::http_helpers::HttpMethod {
                ::toni::http_helpers::HttpMethod::from_string(#http_method).unwrap()
            }

            fn get_path(&self) -> String {
                #full_route_path.to_string()
            }

            fn get_token(&self) -> String {
                #controller_token.to_string()
            }

            fn get_guards(&self) -> Vec<::std::sync::Arc<dyn ::toni::traits_helpers::Guard>> {
                // Direct instantiation fallback for enhancers not in DI
                vec![#(::std::sync::Arc::new(#guard_instances)),*]
            }

            fn get_interceptors(&self) -> Vec<::std::sync::Arc<dyn ::toni::traits_helpers::Interceptor>> {
                // Direct instantiation fallback for enhancers not in DI
                vec![#(::std::sync::Arc::new(#interceptor_instances)),*]
            }

            fn get_pipes(&self) -> Vec<::std::sync::Arc<dyn ::toni::traits_helpers::Pipe>> {
                // Direct instantiation fallback for enhancers not in DI
                vec![#(::std::sync::Arc::new(#pipe_instances)),*]
            }

            fn get_guard_tokens(&self) -> Vec<String> {
                vec![#(#guard_tokens),*]
            }

            fn get_interceptor_tokens(&self) -> Vec<String> {
                vec![#(#interceptor_tokens),*]
            }

            fn get_pipe_tokens(&self) -> Vec<String> {
                vec![#(#pipe_tokens),*]
            }

            fn get_body_dto(&self, _req: &::toni::http_helpers::HttpRequest) -> Option<Box<dyn ::toni::traits_helpers::validate::Validatable>> {
                #body_dto_stream
            }
        }
    }
}

fn generate_manager(
    struct_name: &Ident,
    singleton_metadata: Vec<MetadataInfo>,
    request_metadata: Vec<MetadataInfo>,
    dependencies: &DependencyInfo,
    scope: crate::shared::scope_parser::ControllerScope,
    was_explicit: bool,
) -> TokenStream {
    use crate::shared::scope_parser::ControllerScope;

    match scope {
        ControllerScope::Singleton => generate_singleton_manager(
            struct_name,
            singleton_metadata,
            request_metadata,
            dependencies,
            was_explicit,
        ),
        ControllerScope::Request => {
            // Request-scoped controllers don't need elevation logic
            generate_request_manager(struct_name, request_metadata, dependencies)
        }
    }
}

// Singleton manager - creates controller instance AT STARTUP
// OR elevates to Request scope if dependencies require it
fn generate_singleton_manager(
    struct_name: &Ident,
    singleton_metadata: Vec<MetadataInfo>,
    request_metadata: Vec<MetadataInfo>,
    dependencies: &DependencyInfo,
    was_explicit: bool,
) -> TokenStream {
    let manager_name = Ident::new(&format!("{}Manager", struct_name), struct_name.span());
    let struct_token = struct_name.to_string();

    // Collect dependency tokens from both constructor params and #[inject] fields
    let constructor_token_exprs: Vec<&TokenStream> = dependencies
        .constructor_params
        .iter()
        .map(|(_, _, lookup_token_expr)| lookup_token_expr)
        .collect();

    let field_token_exprs: Vec<&TokenStream> = dependencies
        .fields
        .iter()
        .map(|(_, _full_type, lookup_token_expr)| lookup_token_expr)
        .collect();

    let unique_tokens: Vec<_> = constructor_token_exprs
        .iter()
        .chain(field_token_exprs.iter())
        .map(|token_expr| token_expr)
        .collect();

    // When a constructor is specified, resolve its parameters instead of struct fields
    let deps_to_resolve = if !dependencies.constructor_params.is_empty() {
        &dependencies.constructor_params
    } else {
        &dependencies.fields
    };

    // Generate field resolutions AT STARTUP (no HttpRequest available)
    let field_resolutions = deps_to_resolve
        .iter()
        .map(|(field_name, full_type, lookup_token_expr)| {
            quote! {
                let #field_name: #full_type = {
                    let __lookup_token = #lookup_token_expr;
                    let provider = dependencies
                        .get(&__lookup_token)
                        .unwrap_or_else(|| panic!("Missing dependency '{}'", __lookup_token));

                    let any_box = provider.execute(vec![], None).await;

                    *any_box.downcast::<#full_type>()
                        .unwrap_or_else(|_| panic!(
                            "Failed to downcast '{}' to {}",
                            __lookup_token,
                            stringify!(#full_type)
                        ))
                };
            }
        })
        .collect::<Vec<_>>();

    let field_names: Vec<_> = deps_to_resolve
        .iter()
        .map(|(field_name, _, _)| field_name.clone())
        .collect();

    // Generate struct instantiation based on DI source
    let struct_instantiation = if let Some(init_method_name) = &dependencies.init_method {
        // Constructor-based DI: call the constructor with resolved parameters
        let init_method = Ident::new(init_method_name, struct_name.span());
        quote! { #struct_name::#init_method(#(#field_names),*) }
    } else {
        // Field-based DI: use struct literal
        // Generate initializers for owned fields
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
                #(#field_names,)*  // Injected dependencies
                #(#owned_field_inits),*  // Owned fields with defaults
            }
        }
    };

    // Create controller wrappers with the shared Arc'd instance (for Singleton mode)
    let controller_wrapper_creations: Vec<_> = singleton_metadata
        .iter()
        .map(|metadata| {
            let controller_name = &metadata.struct_name;
            let controller_token = controller_name.to_string();

            if metadata.is_static {
                // Static method: no instance field needed
                quote! {
                    controllers.insert(
                        #controller_token.to_string(),
                        ::std::sync::Arc::new(
                            Box::new(#controller_name {
                            }) as Box<dyn ::toni::traits_helpers::ControllerTrait>
                        )
                    );
                }
            } else {
                // Instance method: pass the shared instance
                quote! {
                    controllers.insert(
                        #controller_token.to_string(),
                        ::std::sync::Arc::new(
                            Box::new(#controller_name {
                                instance: controller_instance.clone(),
                            }) as Box<dyn ::toni::traits_helpers::ControllerTrait>
                        )
                    );
                }
            }
        })
        .collect();

    // Generate scope checking code to determine if we need to elevate to Request scope
    let scope_check_code = if dependencies.fields.is_empty() {
        // No dependencies - definitely Singleton
        quote! {
            let request_deps: Vec<String> = Vec::new();
            let needs_elevation = false;
        }
    } else {
        let dep_checks: Vec<_> = dependencies
            .fields
            .iter()
            .map(|(_, _, lookup_token_expr)| {
                quote! {
                    {
                        let __lookup_token = #lookup_token_expr;
                        if let Some(provider) = dependencies.get(&__lookup_token) {
                            if matches!(provider.get_scope(), ::toni::ProviderScope::Request) {
                                request_deps.push(__lookup_token);
                            }
                        }
                    }
                }
            })
            .collect();

        quote! {
            // Check if any dependency is Request-scoped
            let mut request_deps: Vec<String> = Vec::new();
            #(#dep_checks)*
            let needs_elevation = !request_deps.is_empty();
        }
    };

    // Generate warning messages based on strategy
    let warning_code = if was_explicit {
        // Case 3: User explicitly set singleton, but we need to elevate
        quote! {
            if needs_elevation {
                eprintln!("⚠️  WARNING: Controller '{}' explicitly declared as 'singleton'", #struct_token);
                eprintln!("    but depends on Request-scoped provider(s): {:?}", request_deps);
                eprintln!("    The controller will be Request-scoped. Change to:");
                eprintln!("    #[controller_struct(scope = \"request\", pub struct {} {{ ... }})]", #struct_token);
            }
        }
    } else {
        // Case 1: Default scope (implicit singleton), elevating to request
        quote! {
            if needs_elevation {
                eprintln!("⚠️  INFO: Controller '{}' automatically elevated to Request scope", #struct_token);
                eprintln!("    due to Request-scoped provider(s): {:?}", request_deps);
                eprintln!("    To silence this message, explicitly set:");
                eprintln!("    #[controller_struct(scope = \"request\", pub struct {} {{ ... }})]", #struct_token);
            }
        }
    };

    // Generate controller instances for Request-scoped (used if elevation happens)
    let request_controller_instances: Vec<_> = request_metadata
        .iter()
        .map(|metadata| {
            let controller_name = &metadata.struct_name;
            let controller_token = controller_name.to_string();

            if metadata.is_static {
                // Static method: no dependencies field needed
                quote! {
                    (
                        #controller_token.to_string(),
                        ::std::sync::Arc::new(
                            Box::new(#controller_name {
                            }) as Box<dyn ::toni::traits_helpers::ControllerTrait>
                        )
                    )
                }
            } else {
                // Instance method: pass dependencies
                quote! {
                    (
                        #controller_token.to_string(),
                        ::std::sync::Arc::new(
                            Box::new(#controller_name {
                                dependencies: dependencies.clone(),
                            }) as Box<dyn ::toni::traits_helpers::ControllerTrait>
                        )
                    )
                }
            }
        })
        .collect();

    quote! {
        pub struct #manager_name;

        #[::toni::async_trait]
        impl ::toni::traits_helpers::Controller for #manager_name {
            async fn get_all_controllers(
                &self,
                dependencies: &::toni::FxHashMap<
                    String,
                    ::std::sync::Arc<Box<dyn ::toni::traits_helpers::ProviderTrait>>
                >,
            ) -> ::toni::FxHashMap<
                String,
                ::std::sync::Arc<Box<dyn ::toni::traits_helpers::ControllerTrait>>
            > {
                let mut controllers = ::toni::FxHashMap::default();

                // CHECK IF ELEVATION TO REQUEST SCOPE IS NEEDED
                #scope_check_code

                // EMIT WARNINGS BASED ON STRATEGY
                #warning_code

                // BRANCH: Use Request-scoped logic if elevation needed, otherwise Singleton
                if needs_elevation {
                    // ELEVATED TO REQUEST SCOPE - use Request-scoped wrappers
                    #(
                        let (key, value): (String, ::std::sync::Arc<Box<dyn ::toni::traits_helpers::ControllerTrait>>) = #request_controller_instances;
                        controllers.insert(key, value);
                    )*
                } else {
                    // TRUE SINGLETON - create instance once at startup
                    // RESOLVE DEPENDENCIES AT STARTUP
                    #(#field_resolutions)*

                    // CREATE CONTROLLER INSTANCE AT STARTUP
                    let controller_instance: ::std::sync::Arc<dyn ::std::any::Any + Send + Sync> = ::std::sync::Arc::new(#struct_instantiation);

                    // CREATE ALL HANDLER WRAPPERS THAT SHARE THE SAME ARC
                    #(#controller_wrapper_creations)*
                }

                controllers
            }

            fn get_name(&self) -> String {
                #struct_token.to_string()
            }

            fn get_token(&self) -> String {
                #struct_token.to_string()
            }

            fn get_dependencies(&self) -> Vec<String> {
                vec![#(#unique_tokens),*]
            }
        }
    }
}

// Request manager - stores dependencies, creates instance per request
fn generate_request_manager(
    struct_name: &Ident,
    metadata_list: Vec<MetadataInfo>,
    dependencies: &DependencyInfo,
) -> TokenStream {
    let manager_name = Ident::new(&format!("{}Manager", struct_name), struct_name.span());
    let struct_token = struct_name.to_string();

    // Collect dependency tokens from both constructor params and #[inject] fields
    let constructor_token_exprs: Vec<&TokenStream> = dependencies
        .constructor_params
        .iter()
        .map(|(_, _, lookup_token_expr)| lookup_token_expr)
        .collect();

    let field_token_exprs: Vec<&TokenStream> = dependencies
        .fields
        .iter()
        .map(|(_, _full_type, lookup_token_expr)| lookup_token_expr)
        .collect();

    let unique_tokens: Vec<_> = constructor_token_exprs
        .iter()
        .chain(field_token_exprs.iter())
        .map(|token_expr| token_expr)
        .collect();

    let controller_instances: Vec<_> = metadata_list
        .iter()
        .map(|metadata| {
            let controller_name = &metadata.struct_name;
            let controller_token = controller_name.to_string();

            if metadata.is_static {
                // Static method: no dependencies field needed
                quote! {
                    (
                        #controller_token.to_string(),
                        ::std::sync::Arc::new(
                            Box::new(#controller_name {
                            }) as Box<dyn ::toni::traits_helpers::ControllerTrait>
                        )
                    )
                }
            } else {
                // Instance method: pass dependencies
                quote! {
                    (
                        #controller_token.to_string(),
                        ::std::sync::Arc::new(
                            Box::new(#controller_name {
                                dependencies: dependencies.clone(),
                            }) as Box<dyn ::toni::traits_helpers::ControllerTrait>
                        )
                    )
                }
            }
        })
        .collect();

    quote! {
        pub struct #manager_name;

        #[::toni::async_trait]
        impl ::toni::traits_helpers::Controller for #manager_name {
            async fn get_all_controllers(
                &self,
                dependencies: &::toni::FxHashMap<
                    String,
                    ::std::sync::Arc<Box<dyn ::toni::traits_helpers::ProviderTrait>>
                >,
            ) -> ::toni::FxHashMap<
                String,
                ::std::sync::Arc<Box<dyn ::toni::traits_helpers::ControllerTrait>>
            > {
                let mut controllers = ::toni::FxHashMap::default();

                #(
                    let (key, value): (String, ::std::sync::Arc<Box<dyn ::toni::traits_helpers::ControllerTrait>>) = #controller_instances;
                    controllers.insert(key, value);
                )*

                controllers
            }

            fn get_name(&self) -> String {
                #struct_token.to_string()
            }

            fn get_token(&self) -> String {
                #struct_token.to_string()
            }

            fn get_dependencies(&self) -> Vec<String> {
                vec![#(#unique_tokens),*]
            }
        }
    }
}

fn capitalize_first(s: String) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect()
}

/// Smart path joining that normalizes slashes
/// Examples:
/// - "/" + "/test" = "/test"
/// - "" + "/test" = "/test"
/// - "/api" + "/users" = "/api/users"
/// - "/api/" + "/users" = "/api/users"
/// - "/api" + "users" = "/api/users"
fn join_paths(prefix: &str, path: &str) -> String {
    let prefix = prefix.trim_end_matches('/');
    let path = path.trim_start_matches('/');

    if prefix.is_empty() {
        format!("/{}", path)
    } else if path.is_empty() {
        prefix.to_string()
    } else {
        format!("{}/{}", prefix, path)
    }
}
