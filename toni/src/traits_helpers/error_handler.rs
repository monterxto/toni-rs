use crate::async_trait;
use crate::errors::HttpError;
use crate::http_helpers::{Body, HttpResponse};
use crate::injector::Context;
use serde_json::json;
use std::error::Error;

/// Customize how errors are turned into HTTP responses.
///
/// Handlers are tried in order (method > controller > global) until one returns `Some`.
/// Return `None` to pass to the next handler; if all return `None`, a 500 is sent.
///
/// The `ctx` parameter carries the full execution context. Call `ctx.switch_to_http()` to
/// access HTTP request metadata; `ctx.switch_to_ws()` / `ctx.switch_to_rpc()` for other
/// protocols. Return `None` for protocols you don't handle.
///
/// # Example
///
/// ```ignore
/// use toni::{async_trait, traits_helpers::ErrorHandler, HttpResponse, Body};
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
///     ) -> Option<HttpResponse> {
///         let (parts, _) = ctx.switch_to_http()?;
///         Some(HttpResponse {
///             status: 500,
///             body: Some(Body::json(serde_json::json!({
///                 "error": error.to_string(),
///                 "path": parts.uri().to_string(),
///             }))),
///             headers: vec![],
///         })
///     }
/// }
/// ```
#[async_trait]
pub trait ErrorHandler: Send + Sync {
    async fn handle_error(
        &self,
        error: Box<dyn Error + Send>,
        ctx: &Context,
    ) -> Option<HttpResponse>;
}

pub struct DefaultErrorHandler;

#[async_trait]
impl ErrorHandler for DefaultErrorHandler {
    async fn handle_error(
        &self,
        error: Box<dyn Error + Send>,
        _ctx: &Context,
    ) -> Option<HttpResponse> {
        if let Some(http_error) = error.downcast_ref::<HttpError>() {
            return Some(http_error.to_response());
        }

        Some(HttpResponse {
            status: 500,
            body: Some(Body::json(json!({
                "statusCode": 500,
                "message": "Internal Server Error",
                "error": "Internal Server Error",
            }))),
            headers: vec![],
        })
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
    ) -> Option<HttpResponse> {
        if let Some((parts, _)) = ctx.switch_to_http() {
            eprintln!("[ERROR] {} {} - {}", parts.method, parts.uri, error);
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

        let response = handler
            .handle_error(Box::new(error), &ctx)
            .await
            .unwrap();

        assert_eq!(response.status, 404);
    }

    #[tokio::test]
    async fn test_default_handler_with_unknown_error() {
        let handler = DefaultErrorHandler;
        let error = std::io::Error::new(std::io::ErrorKind::Other, "Unknown error");
        let ctx = create_test_context();

        let response = handler
            .handle_error(Box::new(error), &ctx)
            .await
            .unwrap();

        assert_eq!(response.status, 500);
    }
}
