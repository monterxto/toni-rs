use std::future::Future;
use std::sync::Arc;

use anyhow::Result;

use crate::http_helpers::{HttpMethod, HttpRequest, HttpResponse, ToResponse};
use crate::injector::InstanceWrapper;
use crate::websocket::{ConnectionManager, GatewayWrapper};

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
    /// **Default:** Returns error — implement to support WebSocket upgrades.
    fn bind_gateway(&mut self, path: &str, gateway: Arc<GatewayWrapper>) -> Result<()> {
        let _ = (path, gateway);
        Err(anyhow::anyhow!(
            "This HTTP adapter does not support WebSocket upgrades"
        ))
    }

    /// Broadcast-aware variant of `bind_gateway`.
    ///
    /// Called instead of `bind_gateway` when `BroadcastModule` is imported.
    ///
    /// **Default:** Ignores `connection_manager` and falls back to `bind_gateway`.
    fn bind_gateway_with_broadcast(
        &mut self,
        path: &str,
        gateway: Arc<GatewayWrapper>,
        connection_manager: Arc<ConnectionManager>,
    ) -> Result<()> {
        let _ = connection_manager;
        self.bind_gateway(path, gateway)
    }
}
