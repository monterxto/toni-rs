use async_trait::async_trait;
use std::sync::Arc;

use crate::http_helpers::{HttpRequest, HttpResponse};
use crate::middleware::RoutePattern;

/// Result type for middleware chain execution
pub type MiddlewareResult = Result<HttpResponse, Box<dyn std::error::Error + Send + Sync>>;

/// Internal continuation passed through the middleware chain.
///
/// Not part of the public API — use [`NextHandle`] in `Middleware::handle`.
#[async_trait]
pub(crate) trait NextInternal: Send + Sync {
    async fn run_internal(self: Box<Self>, req: HttpRequest) -> MiddlewareResult;
}

/// The continuation of the middleware chain, carrying the in-flight request.
///
/// Passed to [`Middleware::handle`]. Call [`run`][NextHandle::run] to pass
/// the request downstream unchanged, or [`run_with`][NextHandle::run_with]
/// to replace it (e.g. after adding extensions or rewriting headers).
/// Read the request without consuming `next` via [`request`][NextHandle::request].
pub struct NextHandle {
    req: HttpRequest,
    inner: Box<dyn NextInternal>,
}

impl NextHandle {
    pub(crate) fn new(req: HttpRequest, inner: Box<dyn NextInternal>) -> Self {
        Self { req, inner }
    }

    pub fn request(&self) -> &HttpRequest {
        &self.req
    }

    pub fn request_mut(&mut self) -> &mut HttpRequest {
        &mut self.req
    }

    pub(crate) fn into_parts(self) -> (HttpRequest, Box<dyn NextInternal>) {
        (self.req, self.inner)
    }

    pub async fn run(self) -> MiddlewareResult {
        self.inner.run_internal(self.req).await
    }

    pub async fn run_with(self, req: HttpRequest) -> MiddlewareResult {
        self.inner.run_internal(req).await
    }
}

/// Core middleware trait
#[async_trait]
pub trait Middleware: Send + Sync {
    async fn handle(&self, next: NextHandle) -> MiddlewareResult;
}

/// Functional middleware - simpler alternative using closures
pub type MiddlewareFn = Arc<
    dyn Fn(
            NextHandle,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = MiddlewareResult> + Send>>
        + Send
        + Sync,
>;

/// Wrapper to convert functional middleware to trait
pub struct FunctionalMiddleware {
    handler: MiddlewareFn,
}

impl FunctionalMiddleware {
    pub fn new(handler: MiddlewareFn) -> Self {
        Self { handler }
    }
}

#[async_trait]
impl Middleware for FunctionalMiddleware {
    async fn handle(&self, next: NextHandle) -> MiddlewareResult {
        (self.handler)(next).await
    }
}

/// Middleware configuration for a module
#[derive(Default)]
pub struct MiddlewareConfiguration {
    /// Direct middleware instances (backwards compatible)
    pub middleware: Vec<Arc<dyn Middleware>>,
    /// Middleware tokens for DI resolution (resolved after DI container is built)
    pub middleware_tokens: Vec<String>,
    pub include_patterns: Vec<RoutePattern>,
    pub exclude_patterns: Vec<RoutePattern>,
}

impl MiddlewareConfiguration {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if this middleware should apply to the given path and HTTP method
    pub fn should_apply(&self, path: &str, method: &str) -> bool {
        // If no patterns specified, apply to all
        if self.include_patterns.is_empty() && self.exclude_patterns.is_empty() {
            return true;
        }

        // Check exclusions first - if excluded, don't apply
        for pattern in &self.exclude_patterns {
            if pattern.matches(path, method) {
                return false;
            }
        }

        // If include patterns exist, path must match one
        if !self.include_patterns.is_empty() {
            return self
                .include_patterns
                .iter()
                .any(|pattern| pattern.matches(path, method));
        }

        true
    }
}
