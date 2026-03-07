use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Error, Ident, ItemImpl, ItemStruct, LitInt, LitStr, Result, parse2};

use crate::controller_macro::controller_struct::{extract_constructor_params, has_new_method};
use crate::provider_macro::instance_injection::generate_instance_provider_system;
use crate::shared::dependency_info::DependencySource;
use crate::shared::scope_parser::ProviderScope;
use crate::utils::extracts::extract_struct_dependencies;

/// Parse WebSocket gateway arguments
/// Syntax:
/// - #[websocket_gateway(pub struct Foo { ... })]
/// - #[websocket_gateway("/path", pub struct Foo { ... })]
/// - #[websocket_gateway("/path", namespace = "chat", pub struct Foo { ... })]
/// - #[websocket_gateway("/path", port = 3001, pub struct Foo { ... })]
struct GatewayArgs {
    path: String,
    namespace: Option<String>,
    port: Option<u16>,
    struct_def: ItemStruct,
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

            return Err(input.error(
                "Expected path string, namespace argument, port argument, or struct definition",
            ));
        }

        let struct_def = struct_def.ok_or_else(|| {
            input.error("Missing struct definition (expected `pub struct Name { ... }`)")
        })?;

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

    let mut dependencies = extract_struct_dependencies(&struct_def)?;

    // DI Priority Order (same as #[injectable]):
    // 1. new() method (auto-detected) → constructor injection
    // 2. #[inject] fields → field injection
    // 3. Default fallback (all fields use Default::default())
    if has_new_method(&impl_block) {
        // Auto-detect new() method and use constructor injection
        let params = extract_constructor_params(&impl_block, "new")?;
        dependencies.init_method = Some("new".to_string());
        dependencies.constructor_params = params;
        dependencies.source = DependencySource::Constructor("new".to_string());
    }
    // else: source stays as determined by extract_struct_dependencies (#[inject] fields or Default)

    generate_gateway_impl(
        &struct_def,
        &impl_block,
        &dependencies,
        &path,
        namespace.as_deref(),
        port,
    )
}

fn generate_gateway_impl(
    struct_def: &ItemStruct,
    impl_block: &ItemImpl,
    dependencies: &crate::shared::dependency_info::DependencyInfo,
    path: &str,
    namespace: Option<&str>,
    port: Option<u16>,
) -> Result<TokenStream> {
    let struct_name = &struct_def.ident;
    let struct_token = struct_name.to_string();

    let mut message_handlers = Vec::new();
    let mut on_connect_method = None;
    let mut on_disconnect_method = None;

    for item in &impl_block.items {
        if let syn::ImplItem::Fn(method) = item {
            if let Some(event_name) = extract_subscribe_message_event(&method.attrs) {
                message_handlers.push((event_name, method.clone()));
            } else if has_attribute(&method.attrs, "on_connect") {
                on_connect_method = Some(method.clone());
            } else if has_attribute(&method.attrs, "on_disconnect") {
                on_disconnect_method = Some(method.clone());
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

    // Clean impl block (remove marker attributes)
    let mut impl_def = impl_block.clone();
    for item in impl_def.items.iter_mut() {
        if let syn::ImplItem::Fn(method) = item {
            method.attrs.retain(|attr| {
                !attr.path().is_ident("subscribe_message")
                    && !attr.path().is_ident("on_connect")
                    && !attr.path().is_ident("on_disconnect")
            });
        }
    }

    // Adds Clone derive and DI wiring, same as #[injectable]; is_gateway=true ensures as_gateway() is included in the Provider impl
    let provider_system = generate_instance_provider_system(
        struct_def,
        &impl_def,
        &dependencies,
        ProviderScope::Singleton,
        true,
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
        if attr.path().is_ident("subscribe_message") {
            if let Ok(lit) = attr.parse_args::<LitStr>() {
                return Some(lit.value());
            }
        }
    }
    None
}

fn has_attribute(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident(name))
}
