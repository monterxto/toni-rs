use proc_macro2::TokenStream;
use syn::{FnArg, Ident, ImplItem, ItemImpl, ItemStruct, Pat, Result, Type, parse2};

use crate::{
    shared::{attr_is, dependency_info::DependencySource, scope_parser::ProviderStructArgs},
    utils::extracts::{extract_struct_dependencies, extract_type_token},
};

use super::instance_injection::generate_instance_provider_system;

/// Check if the impl block has a `new()` method
fn has_new_method(impl_block: &ItemImpl) -> bool {
    impl_block.items.iter().any(|item| {
        if let ImplItem::Fn(method) = item {
            method.sig.ident == "new"
        } else {
            false
        }
    })
}

/// Extract the #[inject] or #[inject(token)] attribute from a parameter
/// Returns:
/// - None: no #[inject] attribute
/// - Some(None): #[inject] without token (use type-based token)
/// - Some(Some(token_expr)): #[inject("TOKEN")] or #[inject(Type)] with custom token
fn extract_param_inject_attr(pat_type: &syn::PatType) -> Result<Option<Option<TokenStream>>> {
    for attr in &pat_type.attrs {
        if attr_is(attr, "inject") {
            // Check if there's an argument
            if attr.meta.require_path_only().is_ok() {
                // #[inject] without arguments - use type-based token
                return Ok(Some(None));
            } else {
                // #[inject("TOKEN")] or #[inject(Type)] or #[inject(CONST)]
                // Parse as TokenType to support all token formats
                let token_type: crate::shared::TokenType = attr.parse_args()?;
                let token_expr = token_type.to_token_expr();
                return Ok(Some(Some(token_expr)));
            }
        }
    }
    Ok(None)
}

/// Extract parameters from a constructor method (init or new())
/// Supports #[inject] attribute on parameters to specify custom DI tokens
fn extract_constructor_params(
    impl_block: &ItemImpl,
    method_name: &str,
) -> Result<Vec<(Ident, Type, TokenStream)>> {
    // Find the method
    let method = impl_block.items.iter().find_map(|item| {
        if let ImplItem::Fn(method) = item {
            if method.sig.ident == method_name {
                Some(method)
            } else {
                None
            }
        } else {
            None
        }
    });

    let method = match method {
        Some(m) => m,
        None => return Ok(Vec::new()), // Method not found, return empty
    };

    let mut params = Vec::new();

    // Extract parameters (skip &self, &mut self, self)
    for input in &method.sig.inputs {
        match input {
            FnArg::Receiver(_) => continue, // Skip self parameters
            FnArg::Typed(pat_type) => {
                // Extract parameter name
                let param_name = match &*pat_type.pat {
                    Pat::Ident(pat_ident) => pat_ident.ident.clone(),
                    _ => continue, // Skip complex patterns
                };

                // Extract parameter type
                let param_type = (*pat_type.ty).clone();

                // Check for #[inject] attribute on parameter
                let inject_attr = extract_param_inject_attr(pat_type)?;

                // Determine the lookup token
                let lookup_token_expr = if let Some(custom_token) = inject_attr {
                    if let Some(token_expr) = custom_token {
                        // #[inject("TOKEN")] or #[inject(Type)] - use custom token
                        token_expr
                    } else {
                        // #[inject] - use type-based token
                        extract_type_token(&param_type)?
                    }
                } else {
                    // No #[inject] attribute - use type-based token (default behavior)
                    extract_type_token(&param_type)?
                };

                params.push((param_name, param_type, lookup_token_expr));
            }
        }
    }

    Ok(params)
}

pub fn handle_provider_struct(
    attr: TokenStream,
    item: TokenStream,
    _trait_name: Ident,
) -> Result<TokenStream> {
    // Parse attributes: scope = "request", init = "new", etc.
    let args = parse2::<ProviderStructArgs>(attr)?;

    let scope = args.scope;
    let init_method = args.init;

    // Get struct definition from either args (inline syntax) or item (attribute syntax)
    let (struct_attrs, impl_block) = if let Some(struct_def) = args.struct_def {
        // Inline syntax: #[injectable(pub struct Foo { ... })]
        // In this case, item should be impl block
        let impl_block = parse2::<ItemImpl>(item)?;
        (struct_def, impl_block)
    } else {
        // Attribute syntax: #[injectable] pub struct Foo { ... }
        // Parse struct from item, create empty impl block
        let struct_attrs = parse2::<ItemStruct>(item)?;
        let struct_name = &struct_attrs.ident;
        let empty_impl: ItemImpl = syn::parse_quote! {
            impl #struct_name {}
        };
        (struct_attrs, empty_impl)
    };

    let mut dependencies = extract_struct_dependencies(&struct_attrs)?;

    // DI Priority Order: init override → new() → #[inject] → Default fallback
    // 1. If init is explicitly specified in attributes, use it (highest priority)
    // 2. Otherwise, if new() method exists in impl block, use it automatically
    // 3. Otherwise, use the detected source (Annotations or DefaultFallback)

    if let Some(method_name) = init_method {
        // Explicit init attribute - highest priority
        let params = extract_constructor_params(&impl_block, &method_name)?;
        dependencies.init_method = Some(method_name.clone());
        dependencies.constructor_params = params;
        dependencies.source = DependencySource::Constructor(method_name);
    } else if has_new_method(&impl_block) {
        // Auto-detect new() method - second priority
        let params = extract_constructor_params(&impl_block, "new")?;
        dependencies.init_method = Some("new".to_string());
        dependencies.constructor_params = params;
        dependencies.source = DependencySource::Constructor("new".to_string());
    }
    // Otherwise keep the source determined by extract_struct_dependencies
    // (Annotations, DefaultFallback, or None)

    let expanded = generate_instance_provider_system(
        &struct_attrs,
        &impl_block,
        &dependencies,
        scope,
        false,
        false,
    )?;

    Ok(expanded)
}
