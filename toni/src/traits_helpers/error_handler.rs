use crate::async_trait;
use crate::errors::HttpError;
use crate::http_helpers::{Body, HttpResponse, RequestPart};
use serde_json::json;
use std::error::Error;

/// Customize how errors are turned into HTTP responses.
///
/// Handlers are tried in order (method > controller > global) until one returns `Some`.
/// Return `None` to pass to the next handler; if all return `None`, a 500 is sent.
///
/// The `request` parameter carries only the request metadata — method, URI, headers,
/// path params. The body has already been consumed before any error handler is called.
///
/// # Example
///
/// ```ignore
/// use toni::{async_trait, traits_helpers::ErrorHandler, HttpResponse, Body};
/// use toni::http_helpers::RequestPart;
/// use std::error::Error;
///
/// pub struct CustomErrorHandler;
///
/// #[async_trait]
/// impl ErrorHandler for CustomErrorHandler {
///     async fn handle_error(
///         &self,
///         error: Box<dyn Error + Send>,
///         request: &RequestPart,
///     ) -> Option<HttpResponse> {
///         Some(HttpResponse {
///             status: 500,
///             body: Some(Body::json(serde_json::json!({
///                 "error": error.to_string(),
///                 "path": request.uri().to_string(),
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
        request: &RequestPart,
    ) -> Option<HttpResponse>;
}

pub struct DefaultErrorHandler;

#[async_trait]
impl ErrorHandler for DefaultErrorHandler {
    async fn handle_error(
        &self,
        error: Box<dyn Error + Send>,
        _request: &RequestPart,
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
        request: &RequestPart,
    ) -> Option<HttpResponse> {
        eprintln!("[ERROR] {} {} - {}", request.method, request.uri, error);
        self.inner.handle_error(error, request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_helpers::RequestPart;

    fn create_test_parts() -> RequestPart {
        http::Request::builder().body(()).unwrap().into_parts().0
    }

    #[tokio::test]
    async fn test_default_handler_with_http_error() {
        let handler = DefaultErrorHandler;
        let error = HttpError::not_found("Resource not found");
        let parts = create_test_parts();

        let response = handler
            .handle_error(Box::new(error), &parts)
            .await
            .unwrap();

        assert_eq!(response.status, 404);
    }

    #[tokio::test]
    async fn test_default_handler_with_unknown_error() {
        let handler = DefaultErrorHandler;
        let error = std::io::Error::new(std::io::ErrorKind::Other, "Unknown error");
        let parts = create_test_parts();

        let response = handler
            .handle_error(Box::new(error), &parts)
            .await
            .unwrap();

        assert_eq!(response.status, 500);
    }
}
