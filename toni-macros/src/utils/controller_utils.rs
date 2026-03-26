use quote::quote;
use syn::{Attribute, Ident};

fn is_http_method(segment: &Ident) -> bool {
    matches!(
        segment.to_string().as_str(),
        "get" | "post" | "put" | "delete" | "patch" | "options" | "head"
    )
}

fn has_http_method_attribute(attr: &Attribute) -> bool {
    attr.path()
        .segments
        .iter()
        .any(|segment| is_http_method(&segment.ident))
}

pub fn find_macro_attribute(attrs: &[Attribute], type_macro: String) -> Option<&Attribute> {
    if type_macro == "http_method" {
        return attrs.iter().find(|attr| has_http_method_attribute(attr));
    }
    None
}

pub fn attr_to_string(attr: &Attribute) -> Result<String, ()> {
    let atribute_string = attr
        .path()
        .segments
        .first()
        .map(|segment| segment.ident.to_string());
    match atribute_string {
        Some(s) => Ok(s),
        None => Err(()),
    }
}

pub fn create_extract_body_dto_token_stream(
    body_dto: &Ident,
) -> syn::Result<proc_macro2::TokenStream> {
    let get_body_dto_block = quote! {
        let body: ::serde_json::Value = ::serde_json::from_slice(&req.body)
            .unwrap_or_else(|_| ::serde_json::json!({}));
        let body_json: #body_dto = ::serde_json::from_value(body).unwrap();
        Some(Box::new(body_json))
    };
    Ok(get_body_dto_block)
}
