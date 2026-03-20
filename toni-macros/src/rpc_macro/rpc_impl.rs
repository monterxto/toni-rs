use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, ItemImpl, ItemStruct, LitStr, Result, parse2};

use crate::controller_macro::controller_struct::{extract_constructor_params, has_new_method};
use crate::provider_macro::instance_injection::generate_instance_provider_system;
use crate::shared::dependency_info::DependencySource;
use crate::shared::scope_parser::ProviderScope;
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
            quote! {
                #pattern => Ok(Some(self.#method_name(data, context).await?)),
            }
        })
        .collect();

    let event_arms: Vec<_> = event_handlers
        .iter()
        .map(|(pattern, method)| {
            let method_name = &method.sig.ident;
            quote! {
                #pattern => {
                    self.#method_name(data, context).await?;
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
                !attr.path().is_ident("message_pattern") && !attr.path().is_ident("event_pattern")
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

fn extract_pattern_attr(attrs: &[Attribute], name: &str) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident(name) {
            if let Ok(lit) = attr.parse_args::<LitStr>() {
                return Some(lit.value());
            }
        }
    }
    None
}
