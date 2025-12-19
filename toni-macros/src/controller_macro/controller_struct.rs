use proc_macro2::TokenStream;
use syn::{FnArg, Ident, ImplItem, ItemImpl, Pat, Result, Type, parse2};

use crate::{
    shared::{dependency_info::DependencySource, scope_parser::ControllerStructArgs},
    utils::extracts::{extract_controller_prefix, extract_struct_dependencies, extract_type_token},
};

use super::instance_injection::generate_instance_controller_system;

/// Check if the impl block has a `new()` method
pub fn has_new_method(impl_block: &ItemImpl) -> bool {
    impl_block.items.iter().any(|item| {
        if let ImplItem::Fn(method) = item {
            method.sig.ident == "new"
        } else {
            false
        }
    })
}

/// Extract parameters from a constructor method (init or new())
pub fn extract_constructor_params(
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

                // Generate lookup token for DI resolution
                let lookup_token_expr = extract_type_token(&param_type)?;

                params.push((param_name, param_type, lookup_token_expr));
            }
        }
    }

    Ok(params)
}

pub fn handle_controller_struct(
    attr: TokenStream,
    item: TokenStream,
    _trait_name: Ident,
) -> Result<TokenStream> {
    // Parse: #[controller_struct(scope = "request", pub struct Foo { ... })]
    let args = parse2::<ControllerStructArgs>(attr)?;
    let scope = args.scope;
    let was_explicit = args.was_explicit;
    let init_method = args.init;
    let struct_attrs = args.struct_def;

    let impl_block = parse2::<ItemImpl>(item)?;

    let prefix_path = extract_controller_prefix(&impl_block)?;
    let mut dependencies = extract_struct_dependencies(&struct_attrs)?;

    // DI Priority Order: init override → new() → #[inject] → Default fallback
    // Same as providers for consistency
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

    // Use new instance injection pattern with scope and explicitness
    let expanded = generate_instance_controller_system(
        &struct_attrs,
        &impl_block,
        &dependencies,
        &prefix_path,
        scope,
        was_explicit,
    )?;

    Ok(expanded)
}
