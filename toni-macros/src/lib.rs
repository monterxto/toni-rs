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

/// Applies error handlers to route handlers or controllers for custom error processing.
///
/// Error handlers catch errors from route handlers and return custom HTTP responses.
/// They follow a chain-of-responsibility pattern where specialized handlers can pass
/// errors to more generic handlers by returning None.
///
/// # Syntax
///
/// - **Type name only** - Requires the error handler to be registered in DI container:
///   ```rust,ignore
///   #[use_error_handlers(CustomErrorHandler)]
///   ```
///
/// - **Struct literal** - Directly instantiates the error handler:
///   ```rust,ignore
///   #[use_error_handlers(ValidationErrorHandler{})]
///   #[use_error_handlers(DatabaseErrorHandler { log_queries: true })]
///   ```
///
/// - **Constructor call** - Directly calls the constructor:
///   ```rust,ignore
///   #[use_error_handlers(LoggingErrorHandler::new(log_level))]
///   ```
///
/// # Examples
///
/// **Method-level error handlers:**
/// ```rust,ignore
/// #[use_error_handlers(ValidationErrorHandler{}, DatabaseErrorHandler{})]
/// #[post("/users")]
/// fn create_user(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
///     // Validation and database errors are handled by specialized handlers
/// }
/// ```
///
/// **Controller-level error handlers (applies to all methods):**
/// ```rust,ignore
/// #[use_error_handlers(CustomErrorHandler{})]
/// #[controller("/api")]
/// impl MyController {
///     // All methods use custom error handling
/// }
/// ```
///
/// # Execution Order
///
/// Error handlers execute in reverse hierarchical order (most specific first):
/// 1. Method-level error handlers (in order specified)
/// 2. Controller-level error handlers (in order specified)
/// 3. Global error handlers (registered via `ToniFactory`)
///
/// Each handler can return Some(response) to handle the error, or None to pass
/// to the next handler. If all handlers return None, a default 500 error is returned.
#[proc_macro_attribute]
pub fn use_error_handlers(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

/// Attaches metadata to a route handler for use by guards, interceptors, or other enhancers.
///
/// Route metadata is stored once at startup and shared across all requests to the route.
/// Guards and interceptors can read this metadata via `context.route_metadata().get::<T>()`.
///
/// # Usage
///
/// ```rust,ignore
/// // Define a metadata type
/// #[derive(Clone)]
/// pub struct Roles(pub Vec<&'static str>);
///
/// // Attach to route
/// #[set_metadata(Roles(vec!["admin", "moderator"]))]
/// #[get("/admin")]
/// fn admin_panel(&self) -> ToniBody { ... }
///
/// // Read in guard
/// impl Guard for RolesGuard {
///     fn can_activate(&self, context: &Context) -> bool {
///         if let Some(Roles(required)) = context.route_metadata().get::<Roles>() {
///             // Check user has required roles
///         }
///         true
///     }
/// }
/// ```
///
/// # Multiple Metadata
///
/// Multiple `#[set_metadata(...)]` attributes can be applied to the same route:
///
/// ```rust,ignore
/// #[set_metadata(Roles(vec!["user"]))]
/// #[set_metadata(RateLimit { max: 100, window: 60 })]
/// #[get("/api/data")]
/// fn get_data(&self) -> ToniBody { ... }
/// ```
#[proc_macro_attribute]
pub fn set_metadata(_attr: TokenStream, item: TokenStream) -> TokenStream {
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
// Usage: #[injectable(pub struct Foo {})] #[guard] impl Foo { ... }

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

#[proc_macro_attribute]
pub fn error_handler(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
