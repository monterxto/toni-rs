use std::future::Future;
use std::sync::Arc;

use anyhow::Result;

use crate::http_helpers::HttpMethod;
use crate::injector::InstanceWrapper;
use crate::websocket::{ConnectionManager, GatewayWrapper};

pub trait HttpAdapter: Clone + Send + Sync {
    fn add_route(&mut self, path: &str, method: HttpMethod, handler: Arc<InstanceWrapper>);

    fn listen(self, port: u16, hostname: &str) -> impl Future<Output = Result<()>> + Send;

    fn close(&mut self) -> impl Future<Output = Result<()>> + Send {
        async { Ok(()) }
    }

    /// Optional — implement to support WebSocket upgrades on the same port as HTTP.
    ///
    /// **Default:** Returns error (WebSocket not supported)
    /// **Override:** Implement this if your adapter supports WebSocket upgrades
    fn on_upgrade(&mut self, path: &str, gateway: Arc<GatewayWrapper>) -> Result<()> {
        let _ = (path, gateway);
        Err(anyhow::anyhow!(
            "This HTTP adapter does not support WebSocket upgrades"
        ))
    }

    /// Broadcast-aware WebSocket upgrade (optional - for same-port deployment with broadcasting).
    ///
    /// When `BroadcastModule` is imported, `ToniApplication` resolves `ConnectionManager` from
    /// the DI container and calls this method instead of `on_upgrade()`.
    ///
    /// **Default:** Ignores `connection_manager` and falls back to `on_upgrade()`.
    /// **Override:** Implement this to use `handle_ws_connection_with_broadcast()`.
    fn on_upgrade_with_broadcast(
        &mut self,
        path: &str,
        gateway: Arc<GatewayWrapper>,
        connection_manager: Arc<ConnectionManager>,
    ) -> Result<()> {
        let _ = connection_manager;
        self.on_upgrade(path, gateway)
    }
}
