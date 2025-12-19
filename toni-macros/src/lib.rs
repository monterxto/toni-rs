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
pub fn controller(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = proc_macro2::TokenStream::from(attr);
    let item = proc_macro2::TokenStream::from(item);
    let output =
        controller_macro::controller_consolidated::handle_controller_consolidated(attr, item);
    proc_macro::TokenStream::from(output.unwrap_or_else(|e| e.to_compile_error()))
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

/// Applies guards to route handlers or controllers for request authorization.
///
/// Guards execute before the route handler and can block requests based on custom logic.
/// Multiple guards can be specified and execute in the order listed.
///
/// # Syntax
///
/// - **Type name only** - Requires the guard to be registered in DI container:
///   ```rust,ignore
///   #[use_guards(AuthGuard)]
///   ```
///
/// - **Struct literal** - Directly instantiates the guard:
///   ```rust,ignore
///   #[use_guards(SimpleGuard{})]
///   #[use_guards(AdminGuard { role: "admin" })]
///   ```
///
/// - **Constructor call** - Directly calls the constructor:
///   ```rust,ignore
///   #[use_guards(RoleGuard::new("admin"))]
///   ```
///
/// # Examples
///
/// **Method-level guards:**
/// ```rust,ignore
/// #[use_guards(AuthGuard{}, RoleGuard::new("admin"))]
/// #[get("/admin")]
/// fn admin_panel(&self, req: HttpRequest) -> HttpResponse {
///     // Only accessible to authenticated admin users
/// }
/// ```
///
/// **Controller-level guards (applies to all methods):**
/// ```rust,ignore
/// #[controller("/api", pub struct MyController{})]
/// #[use_guards(AuthGuard{})]
/// impl MyController {
///     // All methods require authentication
/// }
/// ```
///
/// # Execution Order
///
/// Guards execute in hierarchical order:
/// 1. Global guards (registered via `ToniFactory`)
/// 2. Controller-level guards
/// 3. Method-level guards
///
/// Within each level, guards execute in the order specified.
#[proc_macro_attribute]
pub fn use_guards(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Applies interceptors to route handlers or controllers for cross-cutting concerns.
///
/// Interceptors wrap request/response handling, allowing you to execute logic before and after
/// the route handler. Common uses include logging, timing, transformation, and caching.
///
/// # Syntax
///
/// - **Type name only** - Requires the interceptor to be registered in DI container:
///   ```rust,ignore
///   #[use_interceptors(LoggingInterceptor)]
///   ```
///
/// - **Struct literal** - Directly instantiates the interceptor:
///   ```rust,ignore
///   #[use_interceptors(TimingInterceptor{})]
///   #[use_interceptors(CacheInterceptor { ttl: Duration::from_secs(60) })]
///   ```
///
/// - **Constructor call** - Directly calls the constructor:
///   ```rust,ignore
///   #[use_interceptors(CacheInterceptor::new(Duration::from_secs(60)))]
///   ```
///
/// # Examples
///
/// **Method-level interceptors:**
/// ```rust,ignore
/// #[use_interceptors(TimingInterceptor{}, LoggingInterceptor{})]
/// #[get("/users")]
/// fn find_all(&self, req: HttpRequest) -> HttpResponse {
///     // Request is logged and timed
/// }
/// ```
///
/// **Controller-level interceptors (applies to all methods):**
/// ```rust,ignore
/// #[use_interceptors(LoggingInterceptor{})]
/// #[controller("/api")]
/// impl MyController {
///     // All methods are logged
/// }
/// ```
///
/// # Execution Order
///
/// Interceptors execute in hierarchical order with nested "before" and "after" phases:
/// 1. Global interceptors (registered via `ToniFactory`)
/// 2. Controller-level interceptors
/// 3. Method-level interceptors
/// 4. Route handler executes
/// 5. Method-level interceptors (after phase, reverse order)
/// 6. Controller-level interceptors (after phase, reverse order)
/// 7. Global interceptors (after phase, reverse order)
#[proc_macro_attribute]
pub fn use_interceptors(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Applies pipes to route handlers or controllers for data transformation and validation.
///
/// Pipes process request data before it reaches the route handler. Common uses include
/// validation, transformation, sanitization, and parsing.
///
/// # Syntax
///
/// - **Type name only** - Requires the pipe to be registered in DI container:
///   ```rust,ignore
///   #[use_pipes(ValidationPipe)]
///   ```
///
/// - **Struct literal** - Directly instantiates the pipe:
///   ```rust,ignore
///   #[use_pipes(TransformPipe{})]
///   #[use_pipes(ValidationPipe { strict: true })]
///   ```
///
/// - **Constructor call** - Directly calls the constructor:
///   ```rust,ignore
///   #[use_pipes(ValidationPipe::new(strict_mode))]
///   ```
///
/// # Examples
///
/// **Method-level pipes:**
/// ```rust,ignore
/// #[use_pipes(ValidationPipe{}, TransformPipe{})]
/// #[post("/users")]
/// fn create_user(&self, req: HttpRequest) -> HttpResponse {
///     // Request data is validated and transformed
/// }
/// ```
///
/// **Controller-level pipes (applies to all methods):**
/// ```rust,ignore
/// #[use_pipes(ValidationPipe{})]
/// #[controller("/api")]
/// impl MyController {
///     // All methods validate request data
/// }
/// ```
///
/// # Execution Order
///
/// Pipes execute in hierarchical order:
/// 1. Global pipes (registered via `ToniFactory`)
/// 2. Controller-level pipes
/// 3. Method-level pipes
///
/// Within each level, pipes execute in the order specified.
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
