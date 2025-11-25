use std::collections::HashMap;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Error, Ident, Result, Token, punctuated::Punctuated, spanned::Spanned};

fn is_enhancer(segment: &Ident) -> bool {
    matches!(
        segment.to_string().as_str(),
        "toni_guards" | "toni_interceptors" | "toni_pipes" |
        "use_guards" | "use_interceptors" | "use_pipes"
    )
}

pub fn has_enhancer_attribute(attr: &Attribute) -> bool {
    attr.path()
        .segments
        .iter()
        .any(|segment| is_enhancer(&segment.ident))
}

/// Represents an enhancer that can be resolved from DI
#[derive(Clone)]
pub struct EnhancerInfo {
    /// The type identifier of the enhancer
    pub type_ident: Ident,
    /// The token used for DI resolution
    pub token_expr: TokenStream,
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
        let arg_idents = attr
            .parse_args_with(Punctuated::<Ident, Token![,]>::parse_terminated)
            .map_err(|_| Error::new(attr.span(), "Invalid attribute format"))?;

        // Normalize attribute names: strip "toni_" prefix and "use_" prefix
        let key = ident.to_string()
            .replace("toni_", "")
            .replace("use_", "");

        for arg_ident in arg_idents {
            let token_expr = quote! { std::any::type_name::<#arg_ident>().to_string() };
            let info = EnhancerInfo {
                type_ident: arg_ident,
                token_expr,
            };

            enhancers.entry(key.clone()).or_default().push(info);
        }
    }

    // Then process method-level enhancers (ADDS to controller-level, doesn't replace)
    for (ident, attr) in method_enhancers_attr {
        let arg_idents = attr
            .parse_args_with(Punctuated::<Ident, Token![,]>::parse_terminated)
            .map_err(|_| Error::new(attr.span(), "Invalid attribute format"))?;

        // Normalize attribute names: strip "toni_" prefix and "use_" prefix
        let key = ident.to_string()
            .replace("toni_", "")
            .replace("use_", "");

        for arg_ident in arg_idents {
            let token_expr = quote! { std::any::type_name::<#arg_ident>().to_string() };
            let info = EnhancerInfo {
                type_ident: arg_ident,
                token_expr,
            };

            enhancers.entry(key.clone()).or_default().push(info);
        }
    }

    Ok(enhancers)
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
