use proc_macro2::TokenStream;
use syn::{ItemImpl, Result, parse2};

use super::instance_injection::generate_instance_controller_system;
use crate::shared::{dependency_info::DependencySource, scope_parser::ControllerArgs};
use crate::utils::extracts::extract_struct_dependencies;

pub fn handle_controller_consolidated(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    let args = parse2::<ControllerArgs>(attr)?;
    let impl_block = parse2::<ItemImpl>(item)?;

    let struct_def = args.struct_def;
    let path = args.path;
    let scope = args.scope;
    let was_explicit = args.was_explicit;
    let init_method = args.init;

    let mut dependencies = match &struct_def {
        Some(s) => extract_struct_dependencies(s)?,
        None => crate::shared::dependency_info::DependencyInfo {
            fields: vec![],
            owned_fields: vec![],
            init_method: None,
            constructor_params: vec![],
            unique_types: std::collections::HashSet::new(),
            source: DependencySource::None,
        },
    };

    if let Some(method_name) = init_method {
        let params =
            super::controller_struct::extract_constructor_params(&impl_block, &method_name)?;
        dependencies.init_method = Some(method_name.clone());
        dependencies.constructor_params = params;
        dependencies.source = DependencySource::Constructor(method_name);
    } else if super::controller_struct::has_new_method(&impl_block) {
        let params = super::controller_struct::extract_constructor_params(&impl_block, "new")?;
        dependencies.init_method = Some("new".to_string());
        dependencies.constructor_params = params;
        dependencies.source = DependencySource::Constructor("new".to_string());
    } else if struct_def.is_none() {
        return Err(syn::Error::new_spanned(
            &impl_block.self_ty,
            "add a `fn new(...) -> Self` constructor to declare this controller's dependencies, \
             or move the struct definition into the macro attribute",
        ));
    }

    generate_instance_controller_system(
        struct_def.as_ref(),
        &impl_block,
        &dependencies,
        &path,
        scope,
        was_explicit,
    )
}
