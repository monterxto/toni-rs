use crate::http_helpers::RequestPart;

/// Describes the execution context under which a provider's `execute` is called.
///
/// Passed to [`Provider::execute`] so request-scoped providers can inspect the
/// active protocol without an `Option<&HttpRequest>` that leaks HTTP details into
/// every provider signature.
#[derive(Clone, Copy)]
pub enum ProviderContext<'a> {
    /// An HTTP request is being handled. Request-scoped providers use this to
    /// access live request metadata (headers, URI, path params, extensions).
    Http(&'a RequestPart),
    /// A WebSocket message is being handled.
    WebSocket,
    /// An RPC message is being handled.
    Rpc,
    /// No active request (module initialisation, `ApplicationContext::get`, etc.).
    None,
}
