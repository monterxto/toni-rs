use crate::http_helpers::RequestPart;

use super::request_cache::RequestCache;

/// Carries the active HTTP request and the per-request instance cache.
///
/// `parts` is public for use in guards, interceptors, and custom providers.
/// `cache` is framework-internal — used by request-scoped provider wrappers
/// to share a single instance across all injection points within one request.
#[derive(Clone, Copy)]
pub struct HttpContext<'a> {
    pub parts: &'a RequestPart,
    #[doc(hidden)]
    pub cache: &'a RequestCache,
}

/// Describes the execution context under which a provider's `execute` is called.
///
/// Passed to [`Provider::execute`] so request-scoped providers can inspect the
/// active protocol without an `Option<&HttpRequest>` that leaks HTTP details into
/// every provider signature.
#[derive(Clone, Copy)]
pub enum ProviderContext<'a> {
    /// An HTTP request is being handled. Request-scoped providers use this to
    /// access live request metadata (headers, URI, path params, extensions).
    Http(HttpContext<'a>),
    /// A WebSocket message is being handled.
    WebSocket,
    /// An RPC message is being handled.
    Rpc,
    /// No active request (module initialisation, `ApplicationContext::get`, etc.).
    None,
}
