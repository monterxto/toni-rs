//! Error handler trait for customizing error responses
//!
//! Error handlers allow you to customize how errors are converted to HTTP responses.
//! Implement the ErrorHandler trait to add custom error formatting, logging, or transformations.
//!
//! # Examples
//!
//! ## Custom Error Handler
//!
//! ```ignore
//! use toni::{async_trait, traits_helpers::ErrorHandler, HttpRequest, HttpResponse, Body};
//! use serde_json::json;
//! use std::error::Error;
//!
//! pub struct CustomErrorHandler;
//!
//! #[async_trait]
//! impl ErrorHandler for CustomErrorHandler {
//!     async fn handle_error(
//!         &self,
//!         error: Box<dyn Error + Send>,
//!         request: &HttpRequest,
//!     ) -> HttpResponse {
//!         // Custom error handling logic
//!         HttpResponse {
//!             status: 500,
//!             body: Some(Body::Json(json!({
//!                 "error": "Something went wrong",
//!                 "path": request.uri.clone(),
//!                 "timestamp": chrono::Utc::now().to_rfc3339(),
//!             }))),
//!             headers: vec![],
//!         }
//!     }
//! }
//! ```
//!
//! ## Usage in Application
//!
//! ```ignore
//! // Apply globally
//! app.use_global_error_handler(CustomErrorHandler);
//!
//! // Or apply to specific controller
//! #[use_error_handler(CustomErrorHandler)]
//! #[controller("/api")]
//! pub struct ApiController {}
//! ```

use crate::async_trait;
use crate::errors::HttpError;
use crate::http_helpers::{Body, HttpRequest, HttpResponse};
use serde_json::json;
use std::error::Error;

/// Trait for handling errors and converting them to HTTP responses
///
/// Implement this trait to customize how errors are converted to responses.
/// Handlers are called in order (method > controller > global) until one returns Some.
#[async_trait]
pub trait ErrorHandler: Send + Sync {
    /// Try to handle an error and return an HTTP response
    ///
    /// Return Some(response) to handle the error, or None to pass to the next handler.
    /// If all handlers return None, a default 500 error is returned.
    async fn handle_error(
        &self,
        error: Box<dyn Error + Send>,
        request: &HttpRequest,
    ) -> Option<HttpResponse>;
}

/// Default error handler implementation
///
/// This handler:
/// - Converts HttpError variants to appropriate HTTP responses
/// - Returns 500 Internal Server Error for unknown errors
/// - Includes error message, status code, and error type in response
pub struct DefaultErrorHandler;

#[async_trait]
impl ErrorHandler for DefaultErrorHandler {
    async fn handle_error(
        &self,
        error: Box<dyn Error + Send>,
        _request: &HttpRequest,
    ) -> Option<HttpResponse> {
        if let Some(http_error) = error.downcast_ref::<HttpError>() {
            return Some(http_error.to_response());
        }

        Some(HttpResponse {
            status: 500,
            body: Some(Body::Json(json!({
                "statusCode": 500,
                "message": "Internal Server Error",
                "error": "Internal Server Error",
            }))),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
        })
    }
}

/// Error handler that logs errors before converting to responses
///
/// This handler wraps another handler and logs all errors.
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
        request: &HttpRequest,
    ) -> Option<HttpResponse> {
        eprintln!("[ERROR] {} {} - {}", request.method, request.uri, error);
        self.inner.handle_error(error, request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_helpers::Extensions;
    use std::collections::HashMap;

    fn create_test_request() -> HttpRequest {
        HttpRequest {
            body: Body::Text(String::new()),
            headers: vec![],
            method: "GET".to_string(),
            uri: "/test".to_string(),
            query_params: HashMap::new(),
            path_params: HashMap::new(),
            extensions: Extensions::new(),
        }
    }

    #[tokio::test]
    async fn test_default_handler_with_http_error() {
        let handler = DefaultErrorHandler;
        let error = HttpError::not_found("Resource not found");
        let request = create_test_request();

        let response = handler
            .handle_error(Box::new(error), &request)
            .await
            .unwrap();

        assert_eq!(response.status, 404);
    }

    #[tokio::test]
    async fn test_default_handler_with_unknown_error() {
        let handler = DefaultErrorHandler;
        let error = std::io::Error::new(std::io::ErrorKind::Other, "Unknown error");
        let request = create_test_request();

        let response = handler
            .handle_error(Box::new(error), &request)
            .await
            .unwrap();

        assert_eq!(response.status, 500);
    }
}
