use proc_macro2::TokenStream;
use quote::quote;
use syn::{Error, ItemImpl, ItemStruct, Result, parse2};

/// Handle #[middleware_struct] attribute macro
///
/// This macro transforms a struct and its impl block into a proper Middleware implementation
///
/// Example input:
/// ```ignore
/// #[middleware_struct(
///     pub struct MyMiddleware {
///         config: String,
///     }
/// )]
/// impl MyMiddleware {
///     pub fn new(config: String) -> Self {
///         Self { config }
///     }
///
///     async fn handle(&self, next: NextHandle) -> MiddlewareResult {
///         // middleware logic
///         next.run().await
///     }
/// }
/// ```
pub fn handle_middleware_struct(attr: TokenStream, item: TokenStream) -> Result<TokenStream> {
    // Parse the struct definition from the attribute
    let struct_attrs = parse2::<ItemStruct>(attr)?;

    // Parse the impl block from the item
    let impl_block = parse2::<ItemImpl>(item)?;

    let struct_name = &struct_attrs.ident;
    let struct_fields = &struct_attrs.fields;
    let struct_vis = &struct_attrs.vis;

    // Find the handle method in the impl block
    let handle_method = impl_block.items.iter()
        .find_map(|item| {
            if let syn::ImplItem::Fn(method) = item {
                if method.sig.ident == "handle" {
                    Some(method)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .ok_or_else(|| {
            Error::new_spanned(
                &impl_block,
                "Middleware must have a 'handle' method with signature: async fn handle(&self, next: NextHandle) -> MiddlewareResult"
            )
        })?;

    // Extract the method body
    let handle_body = &handle_method.block;

    // Validate the method signature (basic check)
    if handle_method.sig.inputs.len() != 2 {
        return Err(Error::new_spanned(
            &handle_method.sig,
            "handle method must have exactly 2 parameters: &self, next: NextHandle",
        ));
    }

    // Generate the complete code
    let expanded = quote! {
        // Keep the original struct definition
        #[allow(dead_code)]
        #struct_vis struct #struct_name #struct_fields

        // Keep the original impl block (with all other methods)
        #[allow(dead_code)]
        #impl_block

        // Generate the Middleware trait implementation
        #[::toni::async_trait]
        impl ::toni::traits_helpers::middleware::Middleware for #struct_name {
            async fn handle(
                &self,
                next: ::toni::traits_helpers::middleware::NextHandle,
            ) -> ::toni::traits_helpers::middleware::MiddlewareResult {
                #handle_body
            }
        }
    };

    Ok(expanded)
}
