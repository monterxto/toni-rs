use std::{future::Future, pin::Pin, sync::Arc};

use anyhow::Result;

use crate::adapter::WsConnectionCallbacks;
use crate::http_helpers::{HttpMethod, HttpRequest, HttpResponse};

/// Callbacks the framework supplies to an HTTP adapter for one route.
///
/// The adapter calls `handle` with the converted `HttpRequest` and gets back an
/// `HttpResponse` — it never touches controllers, middleware, or guards directly.
pub struct HttpRequestCallbacks {
    handle: Arc<dyn Fn(HttpRequest) -> Pin<Box<dyn Future<Output = HttpResponse> + Send>> + Send + Sync>,
}

impl HttpRequestCallbacks {
    pub(crate) fn new(
        handle: impl Fn(HttpRequest) -> Pin<Box<dyn Future<Output = HttpResponse> + Send>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            handle: Arc::new(handle),
        }
    }

    /// Called by the adapter with the converted request for this route.
    pub async fn handle(&self, req: HttpRequest) -> HttpResponse {
        (self.handle)(req).await
    }
}

pub trait HttpAdapter: Send + Sync + 'static {
    /// Register a route. The adapter stores `callbacks` and calls `callbacks.handle`
    /// with the converted request on each incoming connection to this path.
    fn bind(&mut self, method: HttpMethod, path: &str, callbacks: Arc<HttpRequestCallbacks>);

    /// Register a WebSocket upgrade path on the same port as HTTP.
    ///
    /// **Default:** returns error — implement to support WebSocket upgrades on this adapter.
    fn bind_ws(&mut self, path: &str, callbacks: Arc<WsConnectionCallbacks>) -> Result<()> {
        let _ = (path, callbacks);
        Err(anyhow::anyhow!(
            "This HTTP adapter does not support WebSocket upgrades"
        ))
    }

    /// Seal configuration and return the running server future.
    ///
    /// Called once after all `bind` and `bind_ws` calls. The returned future is the
    /// accept loop — the framework joins it alongside any WS/RPC server futures so no
    /// top-level spawn is needed in the adapter.
    fn create(
        &mut self,
        port: u16,
        hostname: &str,
    ) -> Result<Pin<Box<dyn Future<Output = ()> + Send + 'static>>>;

    fn close(&mut self) -> impl Future<Output = Result<()>> + Send {
        async { Ok(()) }
    }
}

pub(crate) trait ErasedHttpAdapter: Send + Sync {
    fn bind(&mut self, method: HttpMethod, path: &str, callbacks: Arc<HttpRequestCallbacks>);
    fn bind_ws(&mut self, path: &str, callbacks: Arc<WsConnectionCallbacks>) -> Result<()>;
    fn create(
        &mut self,
        port: u16,
        hostname: &str,
    ) -> Result<Pin<Box<dyn Future<Output = ()> + Send + 'static>>>;
    fn close(&mut self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>>;
}

impl<A: HttpAdapter + 'static> ErasedHttpAdapter for A {
    fn bind(&mut self, method: HttpMethod, path: &str, callbacks: Arc<HttpRequestCallbacks>) {
        HttpAdapter::bind(self, method, path, callbacks);
    }

    fn bind_ws(&mut self, path: &str, callbacks: Arc<WsConnectionCallbacks>) -> Result<()> {
        HttpAdapter::bind_ws(self, path, callbacks)
    }

    fn create(
        &mut self,
        port: u16,
        hostname: &str,
    ) -> Result<Pin<Box<dyn Future<Output = ()> + Send + 'static>>> {
        HttpAdapter::create(self, port, hostname)
    }

    fn close(&mut self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(HttpAdapter::close(self))
    }
}
