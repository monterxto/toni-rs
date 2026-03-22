use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, ItemImpl, ItemStruct, LitStr, Result, parse2};

use crate::controller_macro::controller_struct::{extract_constructor_params, has_new_method};
use crate::provider_macro::instance_injection::generate_instance_provider_system;
use crate::shared::dependency_info::DependencySource;
use crate::shared::scope_parser::ProviderScope;
use crate::shared::attr_is;
use crate::utils::extracts::extract_struct_dependencies;

/// Parse `#[rpc_controller(pub struct Foo { ... })]`
struct RpcControllerArgs {
    struct_def: ItemStruct,
}

impl syn::parse::Parse for RpcControllerArgs {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let struct_def = input.parse::<ItemStruct>()?;
        Ok(RpcControllerArgs { struct_def })
    }
}

pub fn handle_rpc_controller(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let args = parse2::<RpcControllerArgs>(attr)?;
    let impl_block = parse2::<ItemImpl>(item)?;

    let struct_def = args.struct_def;

    let mut dependencies = extract_struct_dependencies(&struct_def)?;

    if has_new_method(&impl_block) {
        let params = extract_constructor_params(&impl_block, "new")?;
        dependencies.init_method = Some("new".to_string());
        dependencies.constructor_params = params;
        dependencies.source = DependencySource::Constructor("new".to_string());
    }

    generate_rpc_controller_impl(&struct_def, &impl_block, &dependencies)
}

fn generate_rpc_controller_impl(
    struct_def: &ItemStruct,
    impl_block: &ItemImpl,
    dependencies: &crate::shared::dependency_info::DependencyInfo,
) -> Result<TokenStream> {
    let struct_name = &struct_def.ident;
    let struct_token = struct_name.to_string();

    let mut message_handlers: Vec<(String, syn::ImplItemFn)> = Vec::new();
    let mut event_handlers: Vec<(String, syn::ImplItemFn)> = Vec::new();

    for item in &impl_block.items {
        if let syn::ImplItem::Fn(method) = item {
            if let Some(pattern) = extract_pattern_attr(&method.attrs, "message_pattern") {
                message_handlers.push((pattern, method.clone()));
            } else if let Some(pattern) = extract_pattern_attr(&method.attrs, "event_pattern") {
                check_event_return_type(method)?;
                event_handlers.push((pattern, method.clone()));
            }
        }
    }

    let all_patterns: Vec<&str> = message_handlers
        .iter()
        .map(|(p, _)| p.as_str())
        .chain(event_handlers.iter().map(|(p, _)| p.as_str()))
        .collect();

    let message_arms: Vec<_> = message_handlers
        .iter()
        .map(|(pattern, method)| {
            let method_name = &method.sig.ident;
            let payload_expr = typed_payload_expr(method);
            if returns_rpc_data(method) {
                quote! {
                    #pattern => Ok(Some(self.#method_name(#payload_expr, context).await?)),
                }
            } else {
                quote! {
                    #pattern => {
                        let __result = self.#method_name(#payload_expr, context).await?;
                        let __data = toni::rpc::RpcData::from_serialize(&__result)
                            .map_err(|e| toni::rpc::RpcError::Internal(e.to_string()))?;
                        Ok(Some(__data))
                    }
                }
            }
        })
        .collect();

    let event_arms: Vec<_> = event_handlers
        .iter()
        .map(|(pattern, method)| {
            let method_name = &method.sig.ident;
            let payload_expr = typed_payload_expr(method);
            quote! {
                #pattern => {
                    self.#method_name(#payload_expr, context).await?;
                    Ok(None)
                }
            }
        })
        .collect();

    // Strip marker attributes from the impl block before emitting it
    let mut impl_def = impl_block.clone();
    for item in impl_def.items.iter_mut() {
        if let syn::ImplItem::Fn(method) = item {
            method.attrs.retain(|attr| {
                !attr_is(attr, "message_pattern") && !attr_is(attr, "event_pattern")
            });
        }
    }

    let provider_system = generate_instance_provider_system(
        struct_def,
        &impl_def,
        dependencies,
        ProviderScope::Singleton,
        false,
        true,
    )?;

    let rpc_trait_impl = quote! {
        #[toni::async_trait]
        impl toni::rpc::RpcControllerTrait for #struct_name {
            fn get_token(&self) -> String {
                #struct_token.to_string()
            }

            fn get_patterns(&self) -> Vec<String> {
                vec![#(#all_patterns.to_string()),*]
            }

            async fn handle_message(
                &self,
                data: toni::rpc::RpcData,
                context: toni::rpc::RpcContext,
            ) -> Result<Option<toni::rpc::RpcData>, toni::rpc::RpcError> {
                match context.pattern.as_str() {
                    #(#message_arms)*
                    #(#event_arms)*
                    _ => Err(toni::rpc::RpcError::PatternNotFound(
                        format!("Unknown pattern: {}", context.pattern),
                    )),
                }
            }
        }
    };

    Ok(quote! {
        #provider_system

        #rpc_trait_impl
    })
}

/// Enforces that `#[event_pattern]` handlers return `Result<(), RpcError>`.
///
/// Without this check the Ok value is silently discarded by the generated match arm,
/// which would be a confusing silent bug if the user accidentally used `Result<RpcData, RpcError>`.
fn check_event_return_type(method: &syn::ImplItemFn) -> Result<()> {
    let syn::ReturnType::Type(_, ty) = &method.sig.output else {
        return Err(syn::Error::new_spanned(
            &method.sig,
            "#[event_pattern] handler must return `Result<(), RpcError>`",
        ));
    };

    if let syn::Type::Path(type_path) = ty.as_ref() {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Result" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(first)) = args.args.first() {
                        if let syn::Type::Tuple(tuple) = first {
                            if tuple.elems.is_empty() {
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
    }

    Err(syn::Error::new_spanned(
        &method.sig.output,
        "#[event_pattern] handler must return `Result<(), RpcError>` — use `#[message_pattern]` to return data",
    ))
}

fn extract_pattern_attr(attrs: &[Attribute], name: &str) -> Option<String> {
    for attr in attrs {
        if attr_is(attr, name) {
            if let Ok(lit) = attr.parse_args::<LitStr>() {
                return Some(lit.value());
            }
        }
    }
    None
}

/// Returns true if the type path ends in `RpcData`.
fn is_rpc_data(ty: &syn::Type) -> bool {
    if let syn::Type::Path(tp) = ty {
        if let Some(seg) = tp.path.segments.last() {
            return seg.ident == "RpcData";
        }
    }
    false
}

/// Generates the expression passed as the first non-self argument to the handler.
///
/// If the declared parameter type is `RpcData` (or a path ending in it), the
/// raw `data` value is forwarded directly.  Otherwise, the payload is
/// deserialized into the declared type so handlers can use concrete structs.
fn typed_payload_expr(method: &syn::ImplItemFn) -> proc_macro2::TokenStream {
    let payload_ty = method.sig.inputs.iter().find_map(|arg| {
        if let syn::FnArg::Typed(pt) = arg {
            Some(pt.ty.as_ref())
        } else {
            None
        }
    });

    match payload_ty {
        Some(ty) if !is_rpc_data(ty) => quote! {
            data.parse::<#ty>().map_err(|e| toni::rpc::RpcError::Internal(e.to_string()))?
        },
        _ => quote! { data },
    }
}

/// Returns true when a `#[message_pattern]` handler's `Ok` arm contains `RpcData`.
///
/// Handlers that return `Result<RpcData, RpcError>` are forwarded as-is.
/// Handlers that return `Result<T, RpcError>` for any other T are serialized
/// via `RpcData::from_serialize`.
fn returns_rpc_data(method: &syn::ImplItemFn) -> bool {
    let syn::ReturnType::Type(_, ty) = &method.sig.output else {
        return true;
    };
    let syn::Type::Path(tp) = ty.as_ref() else {
        return true;
    };
    let Some(seg) = tp.path.segments.last() else {
        return true;
    };
    if seg.ident != "Result" {
        return true;
    }
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return true;
    };
    let Some(syn::GenericArgument::Type(inner)) = args.args.first() else {
        return true;
    };
    is_rpc_data(inner)
}
