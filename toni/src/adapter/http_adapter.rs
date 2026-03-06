use std::future::Future;
use std::sync::Arc;

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

    fn listen(self, port: u16, hostname: &str) -> impl Future<Output = Result<()>> + Send;

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
