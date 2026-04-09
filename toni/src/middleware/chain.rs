use async_trait::async_trait;
use std::sync::Arc;

use crate::{
    http_helpers::{HttpRequest, HttpResponse},
    traits_helpers::middleware::{Middleware, MiddlewareResult, NextHandle, NextInternal},
};

pub struct FinalHandler {
    handler: Arc<
        dyn Fn(
                HttpRequest,
            )
                -> std::pin::Pin<Box<dyn std::future::Future<Output = HttpResponse> + Send>>
            + Send
            + Sync,
    >,
}

impl FinalHandler {
    pub fn new<F>(handler: F) -> Self
    where
        F: Fn(
                HttpRequest,
            )
                -> std::pin::Pin<Box<dyn std::future::Future<Output = HttpResponse> + Send>>
            + Send
            + Sync
            + 'static,
    {
        Self {
            handler: Arc::new(handler),
        }
    }
}

#[async_trait]
impl NextInternal for FinalHandler {
    async fn run_internal(self: Box<Self>, req: HttpRequest) -> MiddlewareResult {
        let response = (self.handler)(req).await;
        Ok(response)
    }
}

pub struct ChainLink {
    middleware: Arc<dyn Middleware>,
    next: Box<dyn NextInternal>,
}

impl ChainLink {
    pub fn new(middleware: Arc<dyn Middleware>, next: Box<dyn NextInternal>) -> Self {
        Self { middleware, next }
    }
}

#[async_trait]
impl NextInternal for ChainLink {
    async fn run_internal(self: Box<Self>, req: HttpRequest) -> MiddlewareResult {
        let next_handle = NextHandle::new(req, self.next);
        self.middleware.handle(next_handle).await
    }
}

pub struct MiddlewareChain {
    middleware_stack: Vec<Arc<dyn Middleware>>,
}

impl MiddlewareChain {
    pub fn new() -> Self {
        Self {
            middleware_stack: Vec::new(),
        }
    }

    pub fn use_middleware(&mut self, middleware: Arc<dyn Middleware>) {
        self.middleware_stack.push(middleware);
    }

    pub async fn execute<F>(&self, req: HttpRequest, final_handler: F) -> MiddlewareResult
    where
        F: Fn(
                HttpRequest,
            )
                -> std::pin::Pin<Box<dyn std::future::Future<Output = HttpResponse> + Send>>
            + Send
            + Sync
            + 'static,
    {
        if !self.middleware_stack.is_empty() {
            tracing::trace!(
                count = self.middleware_stack.len(),
                "executing middleware chain"
            );
        }
        let mut inner: Box<dyn NextInternal> = Box::new(FinalHandler::new(final_handler));

        for middleware in self.middleware_stack.iter().rev() {
            inner = Box::new(ChainLink::new(middleware.clone(), inner));
        }

        inner.run_internal(req).await
    }

    pub fn len(&self) -> usize {
        self.middleware_stack.len()
    }

    pub fn is_empty(&self) -> bool {
        self.middleware_stack.is_empty()
    }
}

impl Default for MiddlewareChain {
    fn default() -> Self {
        Self::new()
    }
}
