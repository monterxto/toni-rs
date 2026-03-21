use std::collections::HashSet;

use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    Error, Expr, FnArg, Ident, ImplItemFn, ItemImpl, ItemStruct, LitStr, Pat, Result, Type,
    TypePath, TypeReference, spanned::Spanned,
};

use crate::shared::{attr_is, TokenType};
use crate::shared::dependency_info::{DependencyInfo, DependencySource};

pub fn extract_controller_prefix(impl_block: &ItemImpl) -> Result<String> {
    impl_block
        .attrs
        .iter()
        .find(|attr| attr_is(attr, "controller"))
        .map(|attr| attr.parse_args::<LitStr>().map(|lit| lit.value()))
        .transpose()
        .map(|opt| opt.unwrap_or_default())
}

pub fn extract_struct_dependencies(struct_attrs: &ItemStruct) -> Result<DependencyInfo> {
    let unique_types = HashSet::new();
    let mut fields = Vec::new();
    let mut owned_fields = Vec::new();

    // Check if struct is empty
    if struct_attrs.fields.is_empty() {
        return Ok(DependencyInfo {
            fields,
            owned_fields,
            init_method: None,
            constructor_params: Vec::new(),
            unique_types,
            source: DependencySource::None,
        });
    }

    // Check if ANY field has DI annotations (#[inject] or #[default])
    let has_di_annotations = struct_attrs.fields.iter().any(|field| {
        field
            .attrs
            .iter()
            .any(|attr| attr_is(attr, "inject") || attr_is(attr, "default"))
    });

    for field in &struct_attrs.fields {
        let field_ident = field
            .ident
            .as_ref()
            .ok_or_else(|| syn::Error::new_spanned(field, "Unnamed struct fields not supported"))?;

        // Check for #[inject] or #[inject("TOKEN")] attribute
        let inject_attr = extract_inject_attr(field)?;
        let has_inject = inject_attr.is_some();

        // Check for #[default] attribute
        let default_expr = extract_default_attr(field)?;

        // Validate: can't have both #[inject] and #[default]
        if has_inject && default_expr.is_some() {
            return Err(syn::Error::new_spanned(
                field,
                "Field cannot have both #[inject] and #[default] attributes. \
                 Use #[inject] for DI dependencies or #[default(...)] for owned fields, not both.",
            ));
        }

        if has_di_annotations {
            // Explicit annotation mode: #[inject] means DI, no annotation or #[default] means owned
            if has_inject {
                // This is a DI dependency
                let full_type = field.ty.clone();

                // Determine the lookup token
                let lookup_token_expr = if let Some(custom_token_expr) = inject_attr.unwrap() {
                    // #[inject("TOKEN")] or #[inject(Type)] - use custom token
                    custom_token_expr
                } else {
                    // #[inject] - use type-based token
                    extract_type_token(&field.ty)?
                };

                fields.push((field_ident.clone(), full_type, lookup_token_expr));
            } else {
                // This is an owned field - will use Default::default() if no #[default(...)]
                owned_fields.push((field_ident.clone(), field.ty.clone(), default_expr));
            }
        } else {
            // No annotations - DefaultFallback mode: all fields are owned and use Default trait
            owned_fields.push((field_ident.clone(), field.ty.clone(), None));
        }
    }

    // Determine source
    let source = if has_di_annotations {
        DependencySource::Annotations
    } else {
        DependencySource::DefaultFallback
    };

    Ok(DependencyInfo {
        fields,
        owned_fields,
        init_method: None, // Will be set by caller if provided in attributes
        constructor_params: Vec::new(), // Will be populated by caller if constructor detected
        unique_types,
        source,
    })
}

/// Extract the #[default(expr)] attribute from a field
fn extract_default_attr(field: &syn::Field) -> Result<Option<Expr>> {
    for attr in &field.attrs {
        if attr_is(attr, "default") {
            let expr: Expr = attr.parse_args()?;
            return Ok(Some(expr));
        }
    }
    Ok(None)
}

/// Extract the #[inject] or #[inject(token)] attribute from a field
/// Returns:
/// - None: no #[inject] attribute
/// - Some(None): #[inject] without token (use type-based token)
/// - Some(Some(token_expr)): #[inject("TOKEN")] or #[inject(Type)] with custom token
fn extract_inject_attr(field: &syn::Field) -> Result<Option<Option<TokenStream>>> {
    for attr in &field.attrs {
        if attr_is(attr, "inject") {
            // Check if there's an argument
            if attr.meta.require_path_only().is_ok() {
                // #[inject] without arguments - use type-based token
                return Ok(Some(None));
            } else {
                // #[inject("TOKEN")] or #[inject(Type)] or #[inject(CONST)]
                // Parse as TokenType to support all token formats
                let token_type: TokenType = attr.parse_args()?;
                let token_expr = token_type.to_token_expr();
                return Ok(Some(Some(token_expr)));
            }
        }
    }
    Ok(None)
}

pub fn extract_ident_from_type(ty: &Type) -> Result<&Ident> {
    if let Type::Reference(TypeReference { elem, .. }) = ty {
        if let Type::Path(TypePath { path, .. }) = &**elem {
            if let Some(segment) = path.segments.last() {
                return Ok(&segment.ident);
            }
        }
    }
    if let Type::Path(TypePath { path, .. }) = ty {
        if let Some(segment) = path.segments.last() {
            return Ok(&segment.ident);
        }
    }
    Err(Error::new(ty.span(), "Invalid type"))
}

/// Extracts a type token expression that preserves generic parameters at runtime.
/// For non-generic types, returns a simple string literal.
/// For generic types, generates a runtime type_name call.
///
/// Examples:
/// - `MyService` → `"MyService".to_string()`
/// - `ConfigService<T>` → `format!("ConfigService<{}>", std::any::type_name::<T>())`
/// - `HashMap<K, V>` → `format!("HashMap<{}, {}>", std::any::type_name::<K>(), std::any::type_name::<V>())`
pub fn extract_type_token(ty: &Type) -> Result<TokenStream> {
    // Handle references by unwrapping to inner type
    let actual_type = if let Type::Reference(TypeReference { elem, .. }) = ty {
        &**elem
    } else {
        ty
    };

    if let Type::Path(type_path) = actual_type {
        if let Some(segment) = type_path.path.segments.last() {
            let base_ident = &segment.ident;
            let base_name = base_ident.to_string();

            // Check if this type has generic arguments
            if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                if !args.args.is_empty() {
                    // Has generics - generate runtime type_name call
                    let generic_params: Vec<_> = args
                        .args
                        .iter()
                        .filter_map(|arg| {
                            if let syn::GenericArgument::Type(inner_ty) = arg {
                                Some(inner_ty)
                            } else {
                                None
                            }
                        })
                        .collect();

                    if generic_params.len() == 1 {
                        // Single generic parameter
                        let param_ty = generic_params[0];
                        return Ok(quote! {
                            format!("{}<{}>", #base_name, std::any::type_name::<#param_ty>())
                        });
                    } else {
                        // Multiple generic parameters - build comma-separated list
                        let type_name_calls: Vec<TokenStream> = generic_params
                            .iter()
                            .map(|param_ty| {
                                quote! { std::any::type_name::<#param_ty>() }
                            })
                            .collect();

                        return Ok(quote! {
                            format!("{}<{}>", #base_name, [#(#type_name_calls),*].join(", "))
                        });
                    }
                }
            }

            // No generics - use std::any::type_name for full path
            return Ok(quote! { ::std::any::type_name::<#base_ident>().to_string() });
        }
    }

    Err(syn::Error::new_spanned(
        ty,
        "Expected a type path (e.g., MyType or MyType<T>)",
    ))
}

pub fn extract_params_from_impl_fn(func: &ImplItemFn) -> Vec<(Ident, Type)> {
    let mut params = Vec::new();

    for input in &func.sig.inputs {
        if let FnArg::Typed(pat_type) = input {
            let param_name = match &*pat_type.pat {
                Pat::Ident(pat_ident) => pat_ident.ident.clone(),
                _ => continue,
            };

            let param_type = (*pat_type.ty).clone();

            params.push((param_name, param_type));
        }
    }

    params
}
