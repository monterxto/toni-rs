pub mod dependency_info;
pub mod enhancer_markers;
pub mod lifecycle_hooks;
pub mod metadata_info;
pub mod scope_parser;
pub mod token_parser;

pub use token_parser::TokenType;

/// Returns `true` if the attribute's path ends with `name`.
///
/// Unlike [`syn::Path::is_ident`], this matches both the bare form (`#[foo]`)
/// and any path-qualified form (`#[crate::foo]`, `#[toni_macros::foo]`).
pub fn attr_is(attr: &syn::Attribute, name: &str) -> bool {
    attr.path()
        .segments
        .last()
        .map_or(false, |seg| seg.ident == name)
}
