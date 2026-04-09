use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Error, Ident, ItemImpl, ItemStruct, LitInt, LitStr, Result, parse2};

use crate::controller_macro::controller_struct::{extract_constructor_params, has_new_method};
use crate::provider_macro::instance_injection::generate_instance_provider_system;
use crate::shared::attr_is;
use crate::shared::dependency_info::DependencySource;
use crate::shared::scope_parser::ProviderScope;
use crate::utils::extracts::extract_struct_dependencies;

/// Parse WebSocket gateway arguments.
/// Supports:
/// - `#[websocket_gateway] impl Foo { ... }` — struct defined separately (preferred)
/// - `#[websocket_gateway("/path")] impl Foo { ... }` — with path
/// - `#[websocket_gateway("/path", pub struct Foo { ... })]` — inline struct (legacy)
struct GatewayArgs {
    path: String,
    namespace: Option<String>,
    port: Option<u16>,
    /// `None` when the struct is defined above the impl.
    struct_def: Option<ItemStruct>,
}

impl syn::parse::Parse for GatewayArgs {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let mut path = None;
        let mut namespace = None;
        let mut port = None;
        let mut struct_def = None;

        while !input.is_empty() {
            if input.peek(syn::Token![pub]) || input.peek(syn::Token![struct]) {
                struct_def = Some(input.parse::<ItemStruct>()?);
                break;
            }

            if input.peek(syn::LitStr) && path.is_none() {
                path = Some(input.parse::<LitStr>()?.value());

                if !input.is_empty() && input.peek(syn::Token![,]) {
                    input.parse::<syn::Token![,]>()?;
                }
                continue;
            }

            if input.peek(syn::Ident) {
                let ident: Ident = input.parse()?;

                if ident == "namespace" {
                    input.parse::<syn::Token![=]>()?;
                    namespace = Some(input.parse::<LitStr>()?.value());

                    if !input.is_empty() && input.peek(syn::Token![,]) {
                        input.parse::<syn::Token![,]>()?;
                    }
                    continue;
                }

                if ident == "port" {
                    input.parse::<syn::Token![=]>()?;
                    let lit = input.parse::<LitInt>()?;
                    port = Some(lit.base10_parse::<u16>().map_err(|e| {
                        Error::new(lit.span(), format!("port must be a valid u16: {}", e))
                    })?);

                    if !input.is_empty() && input.peek(syn::Token![,]) {
                        input.parse::<syn::Token![,]>()?;
                    }
                    continue;
                }

                return Err(Error::new(ident.span(), "Unknown argument"));
            }

            return Err(input.error("Expected path string, namespace, port, or struct definition"));
        }

        Ok(GatewayArgs {
            path: path.unwrap_or_else(|| "/".to_string()),
            namespace,
            port,
            struct_def,
        })
    }
}

pub fn handle_websocket_gateway(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let args = parse2::<GatewayArgs>(attr)?;
    let impl_block = parse2::<ItemImpl>(item)?;

    let struct_def = args.struct_def;
    let path = args.path;
    let namespace = args.namespace;
    let port = args.port;

    let mut dependencies = match &struct_def {
        Some(s) => extract_struct_dependencies(s)?,
        None => crate::shared::dependency_info::DependencyInfo {
            fields: vec![],
            owned_fields: vec![],
            init_method: None,
            constructor_params: vec![],
            unique_types: std::collections::HashSet::new(),
            source: DependencySource::None,
        },
    };

    if has_new_method(&impl_block) {
        let params = extract_constructor_params(&impl_block, "new")?;
        dependencies.init_method = Some("new".to_string());
        dependencies.constructor_params = params;
        dependencies.source = DependencySource::Constructor("new".to_string());
    } else if struct_def.is_none() {
        return Err(syn::Error::new_spanned(
            &impl_block.self_ty,
            "add a `fn new(...) -> Self` constructor to declare this gateway's dependencies, \
             or move the struct definition into the macro attribute",
        ));
    }

    generate_gateway_impl(
        struct_def.as_ref(),
        &impl_block,
        &dependencies,
        &path,
        namespace.as_deref(),
        port,
    )
}

fn generate_gateway_impl(
    struct_def: Option<&ItemStruct>,
    impl_block: &ItemImpl,
    dependencies: &crate::shared::dependency_info::DependencyInfo,
    path: &str,
    namespace: Option<&str>,
    port: Option<u16>,
) -> Result<TokenStream> {
    let struct_name = match struct_def {
        Some(s) => s.ident.clone(),
        None => crate::utils::extracts::extract_impl_self_ident(impl_block)?,
    };
    let struct_name = &struct_name;
    let struct_token = struct_name.to_string();

    let mut message_handlers = Vec::new();
    let mut on_connect_method = None;
    let mut on_disconnect_method = None;
    let mut after_init_method = None;

    for item in &impl_block.items {
        if let syn::ImplItem::Fn(method) = item {
            if let Some(event_name) = extract_subscribe_message_event(&method.attrs) {
                message_handlers.push((event_name, method.clone()));
            } else if has_attribute(&method.attrs, "on_connect") {
                on_connect_method = Some(method.clone());
            } else if has_attribute(&method.attrs, "on_disconnect") {
                on_disconnect_method = Some(method.clone());
            } else if has_attribute(&method.attrs, "after_init") {
                after_init_method = Some(method.clone());
            }
        }
    }

    // Generate handle_event implementation
    let match_arms: Vec<_> = message_handlers
        .iter()
        .map(|(event, method)| {
            let method_name = &method.sig.ident;

            quote! {
                #event => {
                    self.#method_name(client, message).await
                }
            }
        })
        .collect();

    let namespace_impl = namespace.map(|ns| {
        quote! {
            fn get_namespace(&self) -> Option<String> {
                Some(#ns.to_string())
            }
        }
    });

    let port_impl = port.map(|p| {
        quote! {
            fn get_port(&self) -> Option<u16> {
                Some(#p)
            }
        }
    });

    // Note: Clone derive is handled by generate_instance_provider_system()
    let on_connect_impl = on_connect_method.as_ref().map(|method| {
        let method_name = &method.sig.ident;
        quote! {
            async fn on_connect(
                &self,
                client: &toni::WsClient,
                _context: &toni::Context,
            ) -> Result<(), toni::WsError> {
                self.#method_name(client).await
            }
        }
    });

    let on_disconnect_impl = on_disconnect_method.as_ref().map(|method| {
        let method_name = &method.sig.ident;
        quote! {
            async fn on_disconnect(
                &self,
                client: &toni::WsClient,
                _reason: toni::DisconnectReason,
            ) {
                self.#method_name(client).await;
            }
        }
    });

    let after_init_impl = after_init_method.as_ref().map(|method| {
        let method_name = &method.sig.ident;
        quote! {
            async fn after_init(&self) {
                self.#method_name().await;
            }
        }
    });

    // Clean impl block (remove marker attributes)
    let mut impl_def = impl_block.clone();
    for item in impl_def.items.iter_mut() {
        if let syn::ImplItem::Fn(method) = item {
            method.attrs.retain(|attr| {
                !attr_is(attr, "subscribe_message")
                    && !attr_is(attr, "on_connect")
                    && !attr_is(attr, "on_disconnect")
                    && !attr_is(attr, "after_init")
            });
        }
    }

    let provider_system = generate_instance_provider_system(
        struct_def,
        &impl_def,
        dependencies,
        ProviderScope::Singleton,
        true,
        false,
    )?;

    let gateway_trait_impl = quote! {
        #[toni::async_trait]
        impl toni::GatewayTrait for #struct_name {
            fn get_token(&self) -> String {
                #struct_token.to_string()
            }

            fn get_path(&self) -> String {
                #path.to_string()
            }

            #namespace_impl

            #port_impl

            #after_init_impl

            #on_connect_impl

            #on_disconnect_impl

            async fn handle_event(
                &self,
                client: toni::WsClient,
                message: toni::WsMessage,
                event: &str,
            ) -> Result<Option<toni::WsMessage>, toni::WsError> {
                match event {
                    #(#match_arms)*
                    _ => Err(toni::WsError::EventNotFound(format!("Unknown event: {}", event)))
                }
            }
        }
    };

    Ok(quote! {
        #provider_system

        #gateway_trait_impl
    })
}

fn extract_subscribe_message_event(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs {
        if attr_is(attr, "subscribe_message") {
            if let Ok(lit) = attr.parse_args::<LitStr>() {
                return Some(lit.value());
            }
        }
    }
    None
}

fn has_attribute(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| attr_is(attr, name))
}
