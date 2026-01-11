use std::collections::HashMap;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Error, Ident, Result, Token, punctuated::Punctuated, spanned::Spanned};

fn is_enhancer(segment: &Ident) -> bool {
    matches!(
        segment.to_string().as_str(),
        "use_guards" | "use_interceptors" | "use_pipes" | "use_error_handlers"
    )
}

pub fn has_enhancer_attribute(attr: &Attribute) -> bool {
    attr.path()
        .segments
        .iter()
        .any(|segment| is_enhancer(&segment.ident))
}

/// Represents an enhancer that can be resolved from DI or directly instantiated
#[derive(Clone)]
pub struct EnhancerInfo {
    /// The type identifier of the enhancer (for token-based DI resolution)
    pub type_ident: Ident,
    /// The token used for DI resolution
    pub token_expr: TokenStream,
    /// The full instantiation expression (for direct instantiation fallback)
    /// E.g., `MyGuard` or `MyGuard::new()` or `MyGuard::new("admin")`
    pub instance_expr: TokenStream,
}

/// Create enhancer infos from attributes for DI resolution
/// Returns a map of enhancer type -> list of EnhancerInfo
pub fn create_enhancer_infos(
    controller_enhancers_attr: HashMap<&Ident, &Attribute>,
    method_enhancers_attr: HashMap<&Ident, &Attribute>,
) -> Result<HashMap<String, Vec<EnhancerInfo>>> {
    let mut enhancers: HashMap<String, Vec<EnhancerInfo>> = HashMap::new();

    // Process controller-level enhancers FIRST
    for (ident, attr) in controller_enhancers_attr {
        // Parse as expressions to support both `MyGuard` and `MyGuard::new()`
        let arg_exprs = attr
            .parse_args_with(Punctuated::<syn::Expr, Token![,]>::parse_terminated)
            .map_err(|_| Error::new(attr.span(), "Invalid attribute format"))?;

        // Normalize attribute names: strip "use_" prefix
        let key = ident.to_string().replace("use_", "");

        for arg_expr in arg_exprs {
            // Extract the type identifier and optionally the instance expression
            let (type_ident, instance_expr_opt) = extract_enhancer_info(&arg_expr)?;

            // Generate token based on the type of enhancer
            let (token_expr, instance_expr) = if let Some(expr) = instance_expr_opt {
                // Check if this is a string token (dummy ident __StringToken)
                if type_ident == "__StringToken" {
                    // String literal: use the expression as token
                    (expr, quote! {})
                } else {
                    // Direct instantiation: no token, use expression for instance
                    (quote! {}, expr)
                }
            } else {
                // Type-name syntax: generate type token
                (
                    quote! { std::any::type_name::<#type_ident>().to_string() },
                    quote! {},
                )
            };

            let info = EnhancerInfo {
                type_ident,
                token_expr,
                instance_expr,
            };

            enhancers.entry(key.clone()).or_default().push(info);
        }
    }

    // Then process method-level enhancers (ADDS to controller-level, doesn't replace)
    for (ident, attr) in method_enhancers_attr {
        // Parse as expressions to support both `MyGuard` and `MyGuard::new()`
        let arg_exprs = attr
            .parse_args_with(Punctuated::<syn::Expr, Token![,]>::parse_terminated)
            .map_err(|_| Error::new(attr.span(), "Invalid attribute format"))?;

        // Normalize attribute names: strip "use_" prefix
        let key = ident.to_string().replace("use_", "");

        for arg_expr in arg_exprs {
            // Extract the type identifier and optionally the instance expression
            let (type_ident, instance_expr_opt) = extract_enhancer_info(&arg_expr)?;

            // Generate token based on the type of enhancer
            let (token_expr, instance_expr) = if let Some(expr) = instance_expr_opt {
                // Check if this is a string token (dummy ident __StringToken)
                if type_ident == "__StringToken" {
                    // String literal: use the expression as token
                    (expr, quote! {})
                } else {
                    // Direct instantiation: no token, use expression for instance
                    (quote! {}, expr)
                }
            } else {
                // Type-name syntax: generate type token
                (
                    quote! { std::any::type_name::<#type_ident>().to_string() },
                    quote! {},
                )
            };

            let info = EnhancerInfo {
                type_ident,
                token_expr,
                instance_expr,
            };

            enhancers.entry(key.clone()).or_default().push(info);
        }
    }

    Ok(enhancers)
}

/// Extract enhancer information from an expression
/// Returns: (type_ident, optional_instance_expr)
///
/// Supports:
/// - `MyGuard` → (`MyGuard`, None) - DI resolution only (generates type token)
/// - `"AUTH_GUARD"` → (`__StringToken`, None) - DI resolution with string token
/// - `APP_GUARD` → (`__ConstToken`, None) - DI resolution with const token
/// - `MyGuard{}` → (`MyGuard`, Some(`MyGuard`)) - Direct instantiation (generates instance)
/// - `MyGuard::new()` → (`MyGuard`, Some(`MyGuard::new()`)) - Direct instantiation via constructor (generates instance)
/// - `MyGuard::new("admin")` → (`MyGuard`, Some(`MyGuard::new("admin")`)) - Direct instantiation with args (generates instance)
fn extract_enhancer_info(expr: &syn::Expr) -> Result<(Ident, Option<TokenStream>)> {
    match expr {
        // String literal: "AUTH_GUARD"
        // Generates: token from string → DI resolution
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(lit_str),
            ..
        }) => {
            let token_string = lit_str.clone();
            // Create a dummy ident for tracking
            let type_ident = Ident::new("__StringToken", lit_str.span());
            // Generate token expression that returns the string
            let token_expr = quote! { #token_string.to_string() };
            // Return as "instance_expr" to override token generation
            Ok((type_ident, Some(token_expr)))
        }
        // Simple path (just type name): MyGuard or APP_GUARD (const)
        // Generates: token only → DI resolution required
        syn::Expr::Path(expr_path) if expr_path.path.segments.len() == 1 => {
            let type_ident = expr_path.path.segments[0].ident.clone();
            Ok((type_ident, None))
        }
        // Struct instantiation: MyGuard{} or MyGuard { field: value }
        // Generates: instance expression → direct instantiation
        syn::Expr::Struct(expr_struct) => {
            if let Some(first_segment) = expr_struct.path.segments.first() {
                let type_ident = first_segment.ident.clone();
                let instance_expr = quote! { #expr };
                return Ok((type_ident, Some(instance_expr)));
            }
            Err(Error::new(
                expr.span(),
                "Expected valid struct path in struct expression",
            ))
        }
        // Constructor call: MyGuard::new() or MyGuard::new("args")
        // Generates: instance expression → direct instantiation
        syn::Expr::Call(expr_call) => {
            if let syn::Expr::Path(path_expr) = &*expr_call.func {
                // Get the first segment (the type name before ::)
                if let Some(first_segment) = path_expr.path.segments.first() {
                    let type_ident = first_segment.ident.clone();
                    let instance_expr = quote! { #expr };
                    return Ok((type_ident, Some(instance_expr)));
                }
            }
            Err(Error::new(
                expr.span(),
                "Expected type identifier or Type::new() expression",
            ))
        }
        _ => Err(Error::new(
            expr.span(),
            "Expected type identifier (MyGuard), string literal (\"AUTH_GUARD\"), struct literal (MyGuard{}), or constructor call (MyGuard::new())",
        )),
    }
}

pub fn create_enchancers_token_stream(
    enhancers_attr: HashMap<&Ident, &Attribute>,
) -> Result<HashMap<String, Vec<TokenStream>>> {
    if enhancers_attr.is_empty() {
        return Ok(HashMap::new());
    }
    let mut enhancers: HashMap<String, Vec<TokenStream>> = HashMap::new();
    for (ident, attr) in enhancers_attr {
        // Parse comma-separated list of identifiers
        let arg_idents = attr
            .parse_args_with(Punctuated::<Ident, Token![,]>::parse_terminated)
            .map_err(|_| Error::new(attr.span(), "Invalid attribute format"))?;

        // Normalize the attribute name (remove toni_ prefix)
        let key = ident.to_string().replace("toni_", "");

        // Add each identifier to the enhancers map
        for arg_ident in arg_idents {
            match enhancers.get_mut(key.as_str()) {
                Some(enhancer_mut) => {
                    enhancer_mut.push(quote! {::std::sync::Arc::new(#arg_ident)});
                }
                None => {
                    enhancers.insert(
                        key.clone(),
                        vec![quote! {::std::sync::Arc::new(#arg_ident)}],
                    );
                }
            };
        }
    }
    Ok(enhancers)
}

/// Create enhancers token stream from TWO hashmaps (controller-level and method-level)
/// This properly accumulates enhancers: controller-level first, then method-level
pub fn create_enhancers_token_stream(
    controller_enhancers_attr: HashMap<&Ident, &Attribute>,
    method_enhancers_attr: HashMap<&Ident, &Attribute>,
) -> Result<HashMap<String, Vec<TokenStream>>> {
    let mut enhancers: HashMap<String, Vec<TokenStream>> = HashMap::new();

    // Process controller-level enhancers FIRST
    for (ident, attr) in controller_enhancers_attr {
        let arg_idents = attr
            .parse_args_with(Punctuated::<Ident, Token![,]>::parse_terminated)
            .map_err(|_| Error::new(attr.span(), "Invalid attribute format"))?;

        let key = ident.to_string().replace("toni_", "");

        for arg_ident in arg_idents {
            match enhancers.get_mut(key.as_str()) {
                Some(enhancer_mut) => {
                    enhancer_mut.push(quote! {::std::sync::Arc::new(#arg_ident)});
                }
                None => {
                    enhancers.insert(
                        key.clone(),
                        vec![quote! {::std::sync::Arc::new(#arg_ident)}],
                    );
                }
            };
        }
    }

    // Then process method-level enhancers (ADDS to controller-level, doesn't replace)
    for (ident, attr) in method_enhancers_attr {
        let arg_idents = attr
            .parse_args_with(Punctuated::<Ident, Token![,]>::parse_terminated)
            .map_err(|_| Error::new(attr.span(), "Invalid attribute format"))?;

        let key = ident.to_string().replace("toni_", "");

        for arg_ident in arg_idents {
            match enhancers.get_mut(key.as_str()) {
                Some(enhancer_mut) => {
                    // This APPENDS instead of replacing!
                    enhancer_mut.push(quote! {::std::sync::Arc::new(#arg_ident)});
                }
                None => {
                    enhancers.insert(
                        key.clone(),
                        vec![quote! {::std::sync::Arc::new(#arg_ident)}],
                    );
                }
            };
        }
    }

    Ok(enhancers)
}

pub fn get_enhancers_attr(attrs: &[Attribute]) -> Result<HashMap<&Ident, &Attribute>> {
    let mut enhancers_attr = HashMap::new();
    attrs.iter().for_each(|attr| {
        if has_enhancer_attribute(attr) {
            let ident = match attr.meta.path().get_ident() {
                Some(ident) => ident,
                None => return,
            };
            enhancers_attr.insert(ident, attr);
        }
    });
    Ok(enhancers_attr)
}
