//! Extractor parameter detection and code generation
//!
//! Detects extractor types like Path<T>, Query<T>, Json<T>, Validated<T>
//! and generates FromRequest extraction code.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{FnArg, Ident, ImplItemFn, Result, Type};

/// Check if a method has a `self` receiver (i.e., is an instance method)
pub fn has_self_receiver(method: &ImplItemFn) -> bool {
    method
        .sig
        .inputs
        .iter()
        .any(|arg| matches!(arg, FnArg::Receiver(_)))
}

/// Information about an extractor parameter
#[derive(Clone)]
pub struct ExtractorParam {
    /// The parameter name (e.g., `id` from `Path(id): Path<i32>`)
    pub param_name: Ident,
    /// The full type (e.g., `Path<i32>`)
    pub param_type: Type,
    /// The extractor kind
    pub kind: ExtractorKind,
}

/// The kind of extractor
#[derive(Debug, Clone, PartialEq)]
pub enum ExtractorKind {
    /// Path<T> extractor
    Path,
    /// Query<T> extractor
    Query,
    /// Json<T> extractor
    Json,
    /// Body<T> extractor (auto-detects content type)
    Body,
    /// Bytes extractor (raw binary data)
    Bytes,
    /// Validated<T> extractor
    Validated,
    /// HttpRequest (not an extractor, just passed through)
    HttpRequest,
    /// Request extractor (optional parameter)
    Request,
    /// Option<T> wrapped extractor (optional extraction)
    Optional {
        /// The inner extractor kind
        inner_kind: Box<ExtractorKind>,
        /// The inner type T from Option<T>
        inner_type: Type,
    },
    /// Unknown type - will be passed as-is
    Unknown,
}

/// Recursively extract parameter name from potentially nested patterns
/// Handles: `dto`, `Json(dto)`, `Validated(Json(dto))`, etc.
fn extract_param_name(pat: &syn::Pat) -> Option<Ident> {
    match pat {
        syn::Pat::Ident(pat_ident) => Some(pat_ident.ident.clone()),
        syn::Pat::TupleStruct(tuple_struct) => {
            if let Some(inner) = tuple_struct.elems.first() {
                extract_param_name(inner)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Extract extractor parameters from a method signature
pub fn get_extractor_params(method: &ImplItemFn) -> Result<Vec<ExtractorParam>> {
    let mut params = Vec::new();

    for input in method.sig.inputs.iter() {
        if let FnArg::Typed(pat_type) = input {
            // Skip parameters with marker attributes (#[body], #[query], #[param])
            if !pat_type.attrs.is_empty() {
                if let Some(attr_ident) = pat_type.attrs[0].path().get_ident() {
                    if matches!(
                        attr_ident.to_string().as_str(),
                        "body" | "param" | "query" | "inject"
                    ) {
                        continue;
                    }
                }
            }

            let param_name = extract_param_name(&pat_type.pat);
            let param_name = match param_name {
                Some(name) => name,
                None => continue,
            };

            if param_name == "self" {
                continue;
            }

            let param_type = (*pat_type.ty).clone();
            let kind = detect_extractor_kind(&param_type);

            params.push(ExtractorParam {
                param_name,
                param_type,
                kind,
            });
        }
    }

    Ok(params)
}

/// Detect what kind of extractor a type is
fn detect_extractor_kind(ty: &Type) -> ExtractorKind {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            let type_name = segment.ident.to_string();

            if type_name == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_type)) = args.args.first() {
                        // Recursively detect the inner extractor kind
                        let inner_kind = detect_extractor_kind(inner_type);
                        return ExtractorKind::Optional {
                            inner_kind: Box::new(inner_kind),
                            inner_type: inner_type.clone(),
                        };
                    }
                }
                // If we can't extract the inner type, treat as Unknown
                return ExtractorKind::Unknown;
            }

            return match type_name.as_str() {
                "Path" => ExtractorKind::Path,
                "Query" => ExtractorKind::Query,
                "Json" => ExtractorKind::Json,
                "Body" => ExtractorKind::Body,
                "Bytes" => ExtractorKind::Bytes,
                "Validated" => ExtractorKind::Validated,
                "HttpRequest" => ExtractorKind::HttpRequest,
                "Request" => ExtractorKind::Request,
                _ => ExtractorKind::Unknown,
            };
        }
    }
    ExtractorKind::Unknown
}

/// Generate extraction code for extractor parameters
pub fn generate_extractor_extractions(
    params: &[ExtractorParam],
) -> Result<(Vec<TokenStream>, Vec<TokenStream>)> {
    let mut extractions = Vec::new();
    let mut call_args = Vec::new();

    for param in params {
        let param_name = &param.param_name;
        let param_type = &param.param_type;

        match &param.kind {
            ExtractorKind::HttpRequest => {
                call_args.push(quote! { req.clone() });
            }
            ExtractorKind::Optional { inner_type, .. } => {
                // Returns None on extraction failure instead of 400 error
                let extraction = quote! {
                    let #param_name = <#inner_type as ::toni::FromRequest>::from_request(&req).ok();
                };
                extractions.push(extraction);
                call_args.push(quote! { #param_name });
            }
            ExtractorKind::Path
            | ExtractorKind::Query
            | ExtractorKind::Json
            | ExtractorKind::Body
            | ExtractorKind::Bytes
            | ExtractorKind::Validated
            | ExtractorKind::Request => {
                let extraction = quote! {
                    let #param_name = match <#param_type as ::toni::FromRequest>::from_request(&req) {
                        Ok(value) => value,
                        Err(e) => {
                            let error_body = ::serde_json::json!({
                                "error": "Extraction failed",
                                "details": e.to_string()
                            });
                            return ::toni::http_helpers::HttpResponse {
                                body: Some(::toni::http_helpers::Body::json(error_body)),
                                status: 400,
                                headers: vec![],
                            };
                        }
                    };
                };
                extractions.push(extraction);
                call_args.push(quote! { #param_name });
            }
            ExtractorKind::Unknown => {
                // Unknown type - try to extract it anyway
                let extraction = quote! {
                    let #param_name = match <#param_type as ::toni::FromRequest>::from_request(&req) {
                        Ok(value) => value,
                        Err(e) => {
                            let error_body = ::serde_json::json!({
                                "error": "Extraction failed",
                                "details": e.to_string()
                            });
                            return ::toni::http_helpers::HttpResponse {
                                body: Some(::toni::http_helpers::Body::json(error_body)),
                                status: 400,
                                headers: vec![],
                            };
                        }
                    };
                };
                extractions.push(extraction);
                call_args.push(quote! { #param_name });
            }
        }
    }

    Ok((extractions, call_args))
}

/// Generate the method call with extracted parameters
pub fn generate_extractor_method_call(
    method: &ImplItemFn,
    call_args: &[TokenStream],
) -> Result<TokenStream> {
    let method_name = &method.sig.ident;
    let is_async = method.sig.asyncness.is_some();

    let call = quote! { controller.#method_name(#(#call_args),*) };

    Ok(if is_async {
        quote! { #call.await }
    } else {
        call
    })
}

/// Generate the method call for static methods (no self receiver)
pub fn generate_extractor_static_method_call(
    method: &ImplItemFn,
    struct_name: &Ident,
    call_args: &[TokenStream],
) -> Result<TokenStream> {
    let method_name = &method.sig.ident;
    let is_async = method.sig.asyncness.is_some();

    let call = quote! { #struct_name::#method_name(#(#call_args),*) };

    Ok(if is_async {
        quote! { #call.await }
    } else {
        call
    })
}
