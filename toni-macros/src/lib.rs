extern crate proc_macro2;

use controller_macro::controller_struct::handle_controller_struct;
use proc_macro::TokenStream;
use proc_macro2::Span;
use provider_macro::provider_struct::handle_provider_struct;
use syn::Ident;

mod config_macro;
mod controller_macro;
mod enhancer;
mod markers_params;
mod middleware_macro;
mod module_macro;
mod provider_macro;
mod provider_variants;
mod shared;
mod utils;

#[proc_macro_attribute]
pub fn module(attr: TokenStream, item: TokenStream) -> TokenStream {
    module_macro::module_struct::module(attr, item)
}

#[proc_macro_attribute]
pub fn controller_struct(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = proc_macro2::TokenStream::from(attr);
    let item = proc_macro2::TokenStream::from(item);
    let trait_name = Ident::new("ControllerTrait", Span::call_site());
    let output = handle_controller_struct(attr, item, trait_name);
    proc_macro::TokenStream::from(output.unwrap_or_else(|e| e.to_compile_error()))
}

#[proc_macro_attribute]
pub fn injectable(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = proc_macro2::TokenStream::from(attr);
    let item = proc_macro2::TokenStream::from(item);
    let trait_name = Ident::new("ProviderTrait", Span::call_site());
    let output = handle_provider_struct(attr, item, trait_name);
    proc_macro::TokenStream::from(output.unwrap_or_else(|e| e.to_compile_error()))
}

#[proc_macro_attribute]
#[deprecated(since = "0.2.0", note = "Use #[injectable] instead")]
pub fn provider_struct(attr: TokenStream, item: TokenStream) -> TokenStream {
    injectable(attr, item)
}

#[proc_macro_attribute]
pub fn controller(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn get(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
#[proc_macro_attribute]
pub fn post(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
#[proc_macro_attribute]
pub fn put(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
#[proc_macro_attribute]
pub fn delete(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Attribute macro for applying guards to a route handler method or controller impl block
///
/// # Example - Method level
/// ```rust
/// #[use_guards(AuthGuard, RoleGuard)]
/// #[get("/admin")]
/// fn admin_panel(&self, req: HttpRequest) -> HttpResponse {
///     // ...
/// }
/// ```
///
/// # Example - Controller level
/// ```rust
/// #[use_guards(AuthGuard)]  // Applies to ALL methods
/// #[controller("/api")]
/// impl MyController {
///     // All methods get AuthGuard
/// }
/// ```
#[proc_macro_attribute]
pub fn use_guards(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Attribute macro for applying interceptors to a route handler method or controller impl block
///
/// # Example - Method level
/// ```rust
/// #[use_interceptors(TimingInterceptor, LoggingInterceptor)]
/// #[get("/users")]
/// fn find_all(&self, req: HttpRequest) -> HttpResponse {
///     // ...
/// }
/// ```
///
/// # Example - Controller level
/// ```rust
/// #[use_interceptors(LoggingInterceptor)]  // Applies to ALL methods
/// #[controller("/api")]
/// impl MyController {
///     // All methods get LoggingInterceptor
/// }
/// ```
#[proc_macro_attribute]
pub fn use_interceptors(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Attribute macro for applying pipes to a route handler method or controller impl block
///
/// # Example - Method level
/// ```rust
/// #[use_pipes(ValidationPipe, TransformPipe)]
/// #[post("/users")]
/// fn create_user(&self, req: HttpRequest) -> HttpResponse {
///     // ...
/// }
/// ```
///
/// # Example - Controller level
/// ```rust
/// #[use_pipes(ValidationPipe)]  // Applies to ALL methods
/// #[controller("/api")]
/// impl MyController {
///     // All methods get ValidationPipe
/// }
/// ```
#[proc_macro_attribute]
pub fn use_pipes(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

// Helper derive to register #[inject] and #[default] as valid attributes
// This allows them to be used on struct fields in injectable/controller_struct
#[proc_macro_derive(Injectable, attributes(inject, default))]
pub fn derive_injectable(_input: TokenStream) -> TokenStream {
    // This derive does nothing - it just registers the attributes
    TokenStream::new()
}

#[proc_macro_derive(Config, attributes(env, default, nested))]
pub fn derive_config(input: TokenStream) -> TokenStream {
    config_macro::derive_config(input)
}

#[proc_macro]
pub fn provider_value(input: TokenStream) -> TokenStream {
    let input = proc_macro2::TokenStream::from(input);
    let output = provider_variants::handle_provider_value(input);
    proc_macro::TokenStream::from(output.unwrap_or_else(|e| e.to_compile_error()))
}

#[proc_macro]
pub fn provider_factory(input: TokenStream) -> TokenStream {
    let input = proc_macro2::TokenStream::from(input);
    let output = provider_variants::handle_provider_factory(input);
    proc_macro::TokenStream::from(output.unwrap_or_else(|e| e.to_compile_error()))
}

#[proc_macro]
pub fn provider_alias(input: TokenStream) -> TokenStream {
    let input = proc_macro2::TokenStream::from(input);
    let output = provider_variants::handle_provider_alias(input);
    proc_macro::TokenStream::from(output.unwrap_or_else(|e| e.to_compile_error()))
}

#[proc_macro]
pub fn provider_token(input: TokenStream) -> TokenStream {
    let input = proc_macro2::TokenStream::from(input);
    let output = provider_variants::handle_provider_token(input);
    proc_macro::TokenStream::from(output.unwrap_or_else(|e| e.to_compile_error()))
}

#[proc_macro]
pub fn provide(input: TokenStream) -> TokenStream {
    let input = proc_macro2::TokenStream::from(input);
    let output = provider_variants::handle_provide(input);
    proc_macro::TokenStream::from(output.unwrap_or_else(|e| e.to_compile_error()))
}

// ============================================================================
// ENHANCER MARKER ATTRIBUTES
// ============================================================================
// These attributes mark structs as specific enhancer types (Guard, Interceptor, etc.)
// They are pass-through attributes that #[injectable] can detect

#[proc_macro_attribute]
pub fn guard(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn interceptor(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn middleware(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn pipe(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
