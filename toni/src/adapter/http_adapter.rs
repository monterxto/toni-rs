use std::{future::Future, pin::Pin, sync::Arc};

use anyhow::Result;

use crate::adapter::WsConnectionCallbacks;
use crate::http_helpers::{HttpMethod, HttpRequest, HttpResponse, ToResponse};
use crate::injector::InstanceWrapper;

pub trait HttpAdapter: Clone + Send + Sync {
    type Request;
    type Response;

    fn adapt_request(request: Self::Request) -> impl Future<Output = Result<HttpRequest>>;

    fn adapt_response(
        response: Box<dyn ToResponse<Response = HttpResponse>>,
    ) -> Result<Self::Response>;

    /// Framework adapters should not override this — implement `adapt_request` and `adapt_response`.
    fn handle_request(
        request: Self::Request,
        controller: Arc<InstanceWrapper>,
    ) -> impl Future<Output = Result<Self::Response>> {
        async move {
            let http_request = Self::adapt_request(request).await?;
            let http_response = controller.handle_request(http_request).await;
            Self::adapt_response(http_response)
        }
    }

    fn add_route(&mut self, path: &str, method: HttpMethod, handler: Arc<InstanceWrapper>);

    fn port(&self) -> u16;

    fn hostname(&self) -> &str;

    fn listen(self) -> impl Future<Output = Result<()>> + Send;

    fn close(&mut self) -> impl Future<Output = Result<()>> + Send {
        async { Ok(()) }
    }

    /// Register a WebSocket gateway on the same port as HTTP (upgrade path).
    ///
    /// **Default:** returns error — implement to support WebSocket upgrades on this adapter.
    fn bind_ws(&mut self, path: &str, callbacks: Arc<WsConnectionCallbacks>) -> Result<()> {
        let _ = (path, callbacks);
        Err(anyhow::anyhow!(
            "This HTTP adapter does not support WebSocket upgrades"
        ))
    }
}

pub(crate) trait ErasedHttpAdapter: Send + Sync {
    fn add_route(&mut self, path: &str, method: HttpMethod, handler: Arc<InstanceWrapper>);
    fn bind_ws(&mut self, path: &str, callbacks: Arc<WsConnectionCallbacks>) -> Result<()>;
    fn port(&self) -> u16;
    fn hostname(&self) -> &str;
    fn close(&mut self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
    fn clone_box(&self) -> Box<dyn ErasedHttpAdapter>;
    fn listen_boxed(self: Box<Self>) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'static>>;
}

impl<A: HttpAdapter + 'static> ErasedHttpAdapter for A {
    fn add_route(&mut self, path: &str, method: HttpMethod, handler: Arc<InstanceWrapper>) {
        HttpAdapter::add_route(self, path, method, handler);
    }

    fn bind_ws(&mut self, path: &str, callbacks: Arc<WsConnectionCallbacks>) -> Result<()> {
        HttpAdapter::bind_ws(self, path, callbacks)
    }

    fn port(&self) -> u16 {
        HttpAdapter::port(self)
    }

    fn hostname(&self) -> &str {
        HttpAdapter::hostname(self)
    }

    fn close(&mut self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(HttpAdapter::close(self))
    }

    fn clone_box(&self) -> Box<dyn ErasedHttpAdapter> {
        Box::new(self.clone())
    }

    fn listen_boxed(self: Box<Self>) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'static>> {
        Box::pin(HttpAdapter::listen(*self))
    }
}
