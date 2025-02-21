use proc_macro2::TokenStream;
use syn::{Ident, ImplItem, ItemImpl, Result};

use crate::shared::dependency_info::DependencyInfo;
use crate::shared::metadata_info::MetadataInfo;

use crate::controller_macro::controller::generate_controller_and_metadata;
use crate::utils::controller_utils::find_http_method_attribute;

pub fn process_impl_functions(
    impl_block: &ItemImpl,
    dependencies: &mut DependencyInfo,
    struct_name: &syn::Ident,
    trait_name: &Ident,
    prefix_path: &str,
) -> Result<(Vec<TokenStream>, Vec<MetadataInfo>)> {
    let mut controllers = Vec::new();
    let mut metadata = Vec::new();
    for item in &impl_block.items {
        if let ImplItem::Fn(method) = item {
            if let Some(attr) = find_http_method_attribute(&method.attrs) {
                let (controller, meta) = generate_controller_and_metadata(
                    method,
                    struct_name,
                    dependencies,
                    trait_name,
                    &prefix_path.to_string(),
                    attr,
                )?;

                controllers.push(controller);
                metadata.push(meta);
            }
        }
    }

    Ok((controllers, metadata))
}
