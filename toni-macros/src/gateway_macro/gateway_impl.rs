use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Error, Ident, ItemImpl, ItemStruct, LitStr, Result, parse2};

use crate::utils::extracts::extract_struct_dependencies;

/// Parse WebSocket gateway arguments
/// Syntax:
/// - #[websocket_gateway(pub struct Foo { ... })]
/// - #[websocket_gateway("/path", pub struct Foo { ... })]
/// - #[websocket_gateway("/path", namespace = "chat", pub struct Foo { ... })]
struct GatewayArgs {
    path: String,
    namespace: Option<String>,
    struct_def: ItemStruct,
}

impl syn::parse::Parse for GatewayArgs {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        let mut path = None;
        let mut namespace = None;
        let mut struct_def = None;

        while !input.is_empty() {
            // Try to parse struct definition
            if input.peek(syn::Token![pub]) || input.peek(syn::Token![struct]) {
                struct_def = Some(input.parse::<ItemStruct>()?);
                break;
            }

            // Try to parse path string
            if input.peek(syn::LitStr) && path.is_none() {
                path = Some(input.parse::<LitStr>()?.value());

                // Check for comma
                if !input.is_empty() && input.peek(syn::Token![,]) {
                    input.parse::<syn::Token![,]>()?;
                }
                continue;
            }

            // Try to parse named arguments (namespace = "...")
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

                return Err(Error::new(ident.span(), "Unknown argument"));
            }

            return Err(
                input.error("Expected path string, namespace argument, or struct definition")
            );
        }

        let struct_def = struct_def.ok_or_else(|| {
            input.error("Missing struct definition (expected `pub struct Name { ... }`)")
        })?;

        Ok(GatewayArgs {
            path: path.unwrap_or_else(|| "/".to_string()),
            namespace,
            struct_def,
        })
    }
}

/// Handle websocket_gateway macro
pub fn handle_websocket_gateway(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let args = parse2::<GatewayArgs>(attr)?;
    let impl_block = parse2::<ItemImpl>(item)?;

    let struct_def = args.struct_def;
    let path = args.path;
    let namespace = args.namespace;

    let dependencies = extract_struct_dependencies(&struct_def)?;

    generate_gateway_impl(
        &struct_def,
        &impl_block,
        &dependencies,
        &path,
        namespace.as_deref(),
    )
}

fn generate_gateway_impl(
    struct_def: &ItemStruct,
    impl_block: &ItemImpl,
    _dependencies: &crate::shared::dependency_info::DependencyInfo,
    path: &str,
    namespace: Option<&str>,
) -> Result<TokenStream> {
    let struct_name = &struct_def.ident;
    let struct_token = struct_name.to_string();

    // Extract message handlers from impl block
    let mut message_handlers = Vec::new();

    for item in &impl_block.items {
        if let syn::ImplItem::Fn(method) = item {
            if let Some(event_name) = extract_subscribe_message_event(&method.attrs) {
                message_handlers.push((event_name, method.clone()));
            }
        }
    }

    // Generate message handler structs
    let handler_structs: Vec<_> = message_handlers.iter().map(|(event, method)| {
        let method_name = &method.sig.ident;
        let method_pascal = to_pascal_case(&method_name.to_string());
        let handler_name = Ident::new(
            &format!("{}{}Handler", struct_name, method_pascal),
            struct_name.span(),
        );

        quote! {
            struct #handler_name;

            #[toni::async_trait]
            impl toni::MessageHandlerTrait for #handler_name {
                async fn handle(&self, context: &mut toni::Context) -> Result<Option<toni::WsMessage>, toni::WsError> {
                    // TODO: Extract gateway instance and call method
                    Ok(None)
                }

                fn event_name(&self) -> &str {
                    #event
                }
            }
        }
    }).collect();

    // Generate handle_event implementation
    let match_arms: Vec<_> = message_handlers
        .iter()
        .map(|(event, method)| {
            let method_name = &method.sig.ident;

            quote! {
                #event => {
                    let gateway_instance = self; // TODO: Resolve dependencies
                    gateway_instance.#method_name(client, message).await
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

    // Add Clone derive to struct
    let struct_with_clone = add_clone_derive(struct_def);

    // Clean impl block (remove marker attributes)
    let mut impl_def = impl_block.clone();
    for item in impl_def.items.iter_mut() {
        if let syn::ImplItem::Fn(method) = item {
            method
                .attrs
                .retain(|attr| !attr.path().is_ident("subscribe_message"));
        }
    }

    Ok(quote! {
        #struct_with_clone

        #impl_def

        #[toni::async_trait]
        impl toni::GatewayTrait for #struct_name {
            fn get_token(&self) -> String {
                #struct_token.to_string()
            }

            fn get_path(&self) -> String {
                #path.to_string()
            }

            #namespace_impl

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

        #(#handler_structs)*
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

fn add_clone_derive(struct_def: &ItemStruct) -> ItemStruct {
    let mut new_struct = struct_def.clone();

    let has_clone = struct_def.attrs.iter().any(|attr| {
        if attr.path().is_ident("derive") {
            if let Ok(meta) = attr.parse_args::<syn::Meta>() {
                return meta_contains_clone(&meta);
            }
        }
        false
    });

    if !has_clone {
        let clone_derive: Attribute = syn::parse_quote! { #[derive(Clone)] };
        new_struct.attrs.push(clone_derive);
    }

    new_struct
}

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

fn to_pascal_case(snake: &str) -> String {
    snake
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}
