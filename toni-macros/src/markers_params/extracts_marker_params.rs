use proc_macro2::TokenStream;
use quote::quote;
use syn::{Result, Type};

use super::get_marker_params::MarkerParam;

/// Generate extractor-based extraction code for a #[body] marker
///
/// Uses the `Body<T>` extractor which auto-detects content type:
/// - application/json → parses as JSON
/// - application/x-www-form-urlencoded → parses as form data
/// - no content-type → tries both (JSON first, then form)
pub fn extract_body_from_param(marker_param: &MarkerParam) -> Result<TokenStream> {
    let param_name = &marker_param.param_name;
    let param_type = &marker_param.param_type;

    // Generate: Body<T> extractor call
    let extract_token_stream = quote! {
        let #param_name = match <::toni::extractors::Body<#param_type> as ::toni::FromRequest>::from_request(&req) {
            Ok(::toni::extractors::Body(value)) => value,
            Err(e) => {
                let error_body = ::serde_json::json!({
                    "error": "Failed to extract request body",
                    "details": e.to_string()
                });
                return Box::new(::toni::http_helpers::HttpResponse {
                    body: Some(::toni::http_helpers::Body::Json(error_body)),
                    status: 400,
                    headers: vec![],
                });
            }
        };
    };
    Ok(extract_token_stream)
}

/// Generate extractor-based extraction code for a #[query] marker
///
/// Supports two modes:
/// 1. `#[query("param_name")]` - Extract individual query parameter (for scalars)
/// 2. `#[query]` - Extract all query params into a struct (uses Query<T> extractor)
pub fn extract_query_from_param(marker_param: &MarkerParam) -> Result<TokenStream> {
    let param_name = &marker_param.param_name;
    let param_type = &marker_param.param_type;

    // If no argument provided, extract as struct using Query<T>
    let Some(marker_arg) = &marker_param.marker_arg else {
        let extract_token_stream = quote! {
            let #param_name = match <::toni::extractors::Query<#param_type> as ::toni::FromRequest>::from_request(&req) {
                Ok(::toni::extractors::Query(value)) => value,
                Err(e) => {
                    let error_body = ::serde_json::json!({
                        "error": "Failed to extract query parameters",
                        "details": e.to_string()
                    });
                    return Box::new(::toni::http_helpers::HttpResponse {
                        body: Some(::toni::http_helpers::Body::Json(error_body)),
                        status: 400,
                        headers: vec![],
                    });
                }
            };
        };
        return Ok(extract_token_stream);
    };

    // If argument provided, extract individual scalar param
    // For scalar types like String, i32, Option<T>, we extract individual query params
    // For struct types with an argument name, we'd use Query<T> extractor
    let extract_token_stream = if is_scalar_type(param_type) {
        let is_option = is_option_type(param_type);

        // Check if there's a default value
        if let Some(default_val) = &marker_param.default_value {
            // With default value - never errors, uses default if missing or parse fails
            quote! {
                let #param_name: #param_type = req.query_params.get(#marker_arg)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| #default_val.parse().expect("Invalid default value"));
            }
        } else if is_option {
            quote! {
                let #param_name: #param_type = match req.query_params.get(#marker_arg) {
                    Some(value) => match value.parse() {
                        Ok(parsed) => Some(parsed),
                        Err(e) => {
                            let error_body = ::serde_json::json!({
                                "error": "Failed to parse query parameter",
                                "param": #marker_arg,
                                "details": format!("Parse error: {}", e)
                            });
                            return Box::new(::toni::http_helpers::HttpResponse {
                                body: Some(::toni::http_helpers::Body::Json(error_body)),
                                status: 400,
                                headers: vec![],
                            });
                        }
                    },
                    None => None,
                };
            }
        } else {
            quote! {
                let #param_name: #param_type = match req.query_params.get(#marker_arg) {
                    Some(value) => match value.parse() {
                        Ok(parsed) => parsed,
                        Err(e) => {
                            let error_body = ::serde_json::json!({
                                "error": "Failed to parse query parameter",
                                "param": #marker_arg,
                                "details": format!("Parse error: {}", e)
                            });
                            return Box::new(::toni::http_helpers::HttpResponse {
                                body: Some(::toni::http_helpers::Body::Json(error_body)),
                                status: 400,
                                headers: vec![],
                            });
                        }
                    },
                    None => {
                        let error_body = ::serde_json::json!({
                            "error": "Missing required query parameter",
                            "param": #marker_arg
                        });
                        return Box::new(::toni::http_helpers::HttpResponse {
                            body: Some(::toni::http_helpers::Body::Json(error_body)),
                            status: 400,
                            headers: vec![],
                        });
                    }
                };
            }
        }
    } else {
        // For complex types, use Query<T> extractor
        quote! {
            let #param_name = match <::toni::Query<#param_type> as ::toni::FromRequest>::from_request(&req) {
                Ok(::toni::Query(value)) => value,
                Err(e) => {
                    let error_body = ::serde_json::json!({
                        "error": "Failed to extract query parameters",
                        "details": e.to_string()
                    });
                    return Box::new(::toni::http_helpers::HttpResponse {
                        body: Some(::toni::http_helpers::Body::Json(error_body)),
                        status: 400,
                        headers: vec![],
                    });
                }
            };
        }
    };

    Ok(extract_token_stream)
}

/// Generate extractor-based extraction code for a #[param] marker
pub fn extract_path_param_from_param(marker_param: &MarkerParam) -> Result<TokenStream> {
    let param_name = &marker_param.param_name;
    let param_type = &marker_param.param_type;
    let marker_arg = marker_param.marker_arg.as_ref().ok_or_else(|| {
        syn::Error::new_spanned(&param_name, "#[param] requires a parameter name argument")
    })?;

    let extract_token_stream = quote! {
        let #param_name: #param_type = match req.path_params.get(#marker_arg) {
            Some(value) => match value.parse() {
                Ok(parsed) => parsed,
                Err(e) => {
                    let error_body = ::serde_json::json!({
                        "error": "Failed to parse path parameter",
                        "param": #marker_arg,
                        "details": format!("Parse error: {}", e)
                    });
                    return Box::new(::toni::http_helpers::HttpResponse {
                        body: Some(::toni::http_helpers::Body::Json(error_body)),
                        status: 400,
                        headers: vec![],
                    });
                }
            },
            None => {
                let error_body = ::serde_json::json!({
                    "error": "Missing required path parameter",
                    "param": #marker_arg
                });
                return Box::new(::toni::http_helpers::HttpResponse {
                    body: Some(::toni::http_helpers::Body::Json(error_body)),
                    status: 400,
                    headers: vec![],
                });
            }
        };
    };

    Ok(extract_token_stream)
}

/// Check if a type is a scalar type (String, i32, bool, etc. or Option<T>)
fn is_scalar_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            let type_name = segment.ident.to_string();
            // Scalar types and Option
            matches!(
                type_name.as_str(),
                "String"
                    | "str"
                    | "i8"
                    | "i16"
                    | "i32"
                    | "i64"
                    | "i128"
                    | "u8"
                    | "u16"
                    | "u32"
                    | "u64"
                    | "u128"
                    | "isize"
                    | "usize"
                    | "f32"
                    | "f64"
                    | "bool"
                    | "char"
                    | "Option"
            )
        } else {
            false
        }
    } else {
        false
    }
}

/// Check if a type is Option<T>
fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            segment.ident == "Option"
        } else {
            false
        }
    } else {
        false
    }
}
