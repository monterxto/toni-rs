use proc_macro2::TokenStream;
use syn::{ItemImpl, Result, parse2};

use super::instance_injection::generate_instance_controller_system;
use crate::shared::scope_parser::ControllerArgs;
use crate::utils::extracts::extract_struct_dependencies;

/// Handle new consolidated controller macro
/// Syntax:
/// - #[controller(pub struct Foo { ... })]
/// - #[controller("/path", pub struct Foo { ... })]
/// - #[controller("/path", scope = "request", pub struct Foo { ... })]
pub fn handle_controller_consolidated(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    // Parse controller arguments (includes struct definition)
    let args = parse2::<ControllerArgs>(attr)?;

    // Parse the impl block
    let impl_block = parse2::<ItemImpl>(item)?;

    // Extract struct and controller config
    let struct_def = args.struct_def;
    let path = args.path;
    let scope = args.scope;
    let was_explicit = args.was_explicit;
    let init_method = args.init;

    // Extract dependencies from struct
    let mut dependencies = extract_struct_dependencies(&struct_def)?;

    // Handle init method if specified
    if let Some(method_name) = init_method {
        let params =
            super::controller_struct::extract_constructor_params(&impl_block, &method_name)?;
        dependencies.init_method = Some(method_name.clone());
        dependencies.constructor_params = params;
        dependencies.source =
            crate::shared::dependency_info::DependencySource::Constructor(method_name);
    } else if super::controller_struct::has_new_method(&impl_block) {
        let params = super::controller_struct::extract_constructor_params(&impl_block, "new")?;
        dependencies.init_method = Some("new".to_string());
        dependencies.constructor_params = params;
        dependencies.source =
            crate::shared::dependency_info::DependencySource::Constructor("new".to_string());
    }

    // Reuse existing code generation
    let expanded = generate_instance_controller_system(
        &struct_def,
        &impl_block,
        &dependencies,
        &path,
        scope,
        was_explicit,
    )?;

    Ok(expanded)
}
