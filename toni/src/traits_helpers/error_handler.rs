use crate::async_trait;
use crate::errors::HttpError;
use crate::http_helpers::{Body, HttpResponse};
use crate::injector::Context;
use crate::rpc::RpcData;
use crate::websocket::WsMessage;
use serde_json::json;
use std::error::Error;

/// The response an error handler sends back to the client.
///
/// Return the variant that matches the active protocol. The framework
/// routes each variant to the correct send path and ignores variants
/// that don't match the current context (e.g. `Http` in a WS context).
pub enum ErrorResponse {
    /// Send an HTTP response. Use in HTTP contexts.
    Http(HttpResponse),
    /// Send a WebSocket message back to the client. Use in WS contexts.
    Ws(WsMessage),
    /// Send an RPC reply. Use in RPC contexts.
    Rpc(RpcData),
}

impl From<HttpResponse> for ErrorResponse {
    fn from(r: HttpResponse) -> Self {
        ErrorResponse::Http(r)
    }
}

impl From<WsMessage> for ErrorResponse {
    fn from(m: WsMessage) -> Self {
        ErrorResponse::Ws(m)
    }
}

impl From<RpcData> for ErrorResponse {
    fn from(d: RpcData) -> Self {
        ErrorResponse::Rpc(d)
    }
}

/// Customize how errors are turned into responses.
///
/// Handlers are tried in order (method > controller > global) until one returns `Some`.
/// Return `None` to pass to the next handler; if all return `None`, a default is sent.
///
/// Call `ctx.switch_to_http()` / `ctx.switch_to_ws()` / `ctx.switch_to_rpc()` to access
/// protocol-specific data, and return the matching `ErrorResponse` variant.
/// Return `None` for protocols you don't handle.
///
/// # Example
///
/// ```ignore
/// use toni::{async_trait, traits_helpers::{ErrorHandler, ErrorResponse}, HttpResponse};
/// use toni::injector::Context;
/// use std::error::Error;
///
/// pub struct CustomErrorHandler;
///
/// #[async_trait]
/// impl ErrorHandler for CustomErrorHandler {
///     async fn handle_error(
///         &self,
///         error: Box<dyn Error + Send>,
///         ctx: &Context,
///     ) -> Option<ErrorResponse> {
///         let http = ctx.switch_to_http()?;
///         Some(ErrorResponse::Http(HttpResponse {
///             status: 500,
///             body: Some(toni::Body::json(serde_json::json!({
///                 "error": error.to_string(),
///                 "path": http.request().uri.to_string(),
///             }))),
///             headers: vec![],
///         }))
///     }
/// }
/// ```
#[async_trait]
pub trait ErrorHandler: Send + Sync {
    async fn handle_error(
        &self,
        error: Box<dyn Error + Send>,
        ctx: &Context,
    ) -> Option<ErrorResponse>;
}

pub struct DefaultErrorHandler;

#[async_trait]
impl ErrorHandler for DefaultErrorHandler {
    async fn handle_error(
        &self,
        error: Box<dyn Error + Send>,
        ctx: &Context,
    ) -> Option<ErrorResponse> {
        use crate::injector::ProtocolType;

        match ctx.protocol_type() {
            ProtocolType::Http => {
                if let Some(http_error) = error.downcast_ref::<HttpError>() {
                    return Some(ErrorResponse::Http(http_error.to_response()));
                }
                Some(ErrorResponse::Http(HttpResponse {
                    status: 500,
                    body: Some(Body::json(json!({
                        "statusCode": 500,
                        "message": "Internal Server Error",
                        "error": "Internal Server Error",
                    }))),
                    headers: vec![],
                }))
            }
            ProtocolType::WebSocket => {
                let message = if let Some(http_error) = error.downcast_ref::<HttpError>() {
                    http_error.message().to_string()
                } else {
                    error.to_string()
                };
                Some(ErrorResponse::Ws(WsMessage::text(
                    json!({ "status": "error", "message": message }).to_string(),
                )))
            }
            ProtocolType::Rpc => {
                let message = if let Some(http_error) = error.downcast_ref::<HttpError>() {
                    http_error.message().to_string()
                } else {
                    error.to_string()
                };
                Some(ErrorResponse::Rpc(RpcData::json(json!({ "status": "error", "message": message }))))
            }
        }
    }
}

/// Wraps another [`ErrorHandler`] and logs each error before delegating.
pub struct LoggingErrorHandler<H: ErrorHandler> {
    inner: H,
}

impl<H: ErrorHandler> LoggingErrorHandler<H> {
    pub fn new(inner: H) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl<H: ErrorHandler> ErrorHandler for LoggingErrorHandler<H> {
    async fn handle_error(
        &self,
        error: Box<dyn Error + Send>,
        ctx: &Context,
    ) -> Option<ErrorResponse> {
        if let Some(http) = ctx.switch_to_http() {
            let req = http.request();
            eprintln!("[ERROR] {} {} - {}", req.method, req.uri, error);
        } else {
            eprintln!("[ERROR] {:?} - {}", ctx.protocol_type(), error);
        }
        self.inner.handle_error(error, ctx).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_helpers::RequestPart;

    fn create_test_context() -> Context {
        let parts: RequestPart = http::Request::builder().body(()).unwrap().into_parts().0;
        Context::from_parts(parts)
    }

    #[tokio::test]
    async fn test_default_handler_with_http_error() {
        let handler = DefaultErrorHandler;
        let error = HttpError::not_found("Resource not found");
        let ctx = create_test_context();

        let response = handler.handle_error(Box::new(error), &ctx).await.unwrap();
        let ErrorResponse::Http(r) = response else { panic!("expected Http") };
        assert_eq!(r.status, 404);
    }

    #[tokio::test]
    async fn test_default_handler_with_unknown_error() {
        let handler = DefaultErrorHandler;
        let error = std::io::Error::new(std::io::ErrorKind::Other, "Unknown error");
        let ctx = create_test_context();

        let response = handler.handle_error(Box::new(error), &ctx).await.unwrap();
        let ErrorResponse::Http(r) = response else { panic!("expected Http") };
        assert_eq!(r.status, 500);
    }
}
