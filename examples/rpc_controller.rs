// RPC controller example with a stub adapter.
//
// This example demonstrates:
// 1. Defining an RPC controller with #[rpc_controller]
// 2. Request-response handlers via #[message_pattern]
// 3. Fire-and-forget handlers via #[event_pattern]
// 4. Automatic DI wiring — the controller is discovered from the module
//
// The stub adapter fires two synthetic messages at startup so the full
// dispatch path (macro → DI → handler) is exercised without a real transport.
// Once both messages are processed the HTTP server keeps running; Ctrl-C to stop.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use anyhow::Result;
use toni::{RpcAdapter, RpcContext, RpcData, RpcMessageCallbacks, ToniFactory};
use toni_macros::{injectable, module, rpc_controller};

// ============================================================================
// Service
// ============================================================================

#[injectable(pub struct OrdersService {})]
impl OrdersService {
    pub fn create_order(&self, item: &str, qty: u32) -> serde_json::Value {
        println!("[OrdersService] Creating order: {} x{}", item, qty);
        serde_json::json!({ "id": 1001, "item": item, "qty": qty, "status": "created" })
    }

    pub fn handle_shipment(&self, order_id: u64) {
        println!("[OrdersService] Order {} marked as shipped", order_id);
    }
}

// ============================================================================
// RPC controller
// ============================================================================

#[rpc_controller(pub struct OrdersController {
    #[inject] service: OrdersService,
})]
impl OrdersController {
    pub fn new(service: OrdersService) -> Self {
        Self { service }
    }

    // Returns a reply — the adapter sends it back to the caller.
    #[message_pattern("order.create")]
    async fn create_order(
        &self,
        data: RpcData,
        _ctx: RpcContext,
    ) -> Result<RpcData, toni::RpcError> {
        let payload = data
            .as_json()
            .ok_or_else(|| toni::RpcError::Internal("expected JSON payload".into()))?;

        let item = payload["item"].as_str().unwrap_or("unknown");
        let qty = payload["qty"].as_u64().unwrap_or(1) as u32;

        let order = self.service.create_order(item, qty);
        Ok(RpcData::json(order))
    }

    // No reply — the adapter sends nothing back.
    #[event_pattern("order.shipped")]
    async fn on_order_shipped(
        &self,
        data: RpcData,
        _ctx: RpcContext,
    ) -> Result<(), toni::RpcError> {
        let payload = data
            .as_json()
            .ok_or_else(|| toni::RpcError::Internal("expected JSON payload".into()))?;

        let order_id = payload["order_id"].as_u64().unwrap_or(0);
        self.service.handle_shipment(order_id);
        Ok(())
    }
}

// ============================================================================
// Module
// ============================================================================

#[module(providers: [OrdersService, OrdersController])]
struct OrdersModule;

// ============================================================================
// Stub adapter — exercises the dispatch path without a real transport
// ============================================================================

struct StubRpcAdapter {
    callbacks: Option<Arc<RpcMessageCallbacks>>,
}

impl StubRpcAdapter {
    fn new() -> Self {
        Self { callbacks: None }
    }
}

impl RpcAdapter for StubRpcAdapter {
    fn bind(&mut self, patterns: &[String], callbacks: Arc<RpcMessageCallbacks>) -> Result<()> {
        println!("[StubRpcAdapter] Bound to patterns: {:?}", patterns);
        self.callbacks = Some(callbacks);
        Ok(())
    }

    fn create(&mut self) -> Result<Pin<Box<dyn Future<Output = ()> + Send + 'static>>> {
        let callbacks = self
            .callbacks
            .take()
            .expect("bind() must be called before create()");

        Ok(Box::pin(async move {
            println!("\n[StubRpcAdapter] Sending test messages...\n");

            // --- request-response ---
            let reply = callbacks
                .message(
                    RpcData::json(serde_json::json!({ "item": "keyboard", "qty": 3 })),
                    RpcContext::new("order.create"),
                )
                .await;

            println!(
                "[StubRpcAdapter] order.create reply: {}",
                reply
                    .as_ref()
                    .and_then(|r| r.as_json())
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "<none>".into())
            );

            // --- fire-and-forget ---
            let reply = callbacks
                .message(
                    RpcData::json(serde_json::json!({ "order_id": 1001 })),
                    RpcContext::new("order.shipped"),
                )
                .await;

            println!(
                "[StubRpcAdapter] order.shipped reply: {} (expected none)",
                reply
                    .as_ref()
                    .and_then(|r| r.as_json())
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "<none>".into())
            );

            println!("\n[StubRpcAdapter] All test messages processed.");
            println!("HTTP server still running on :8080 — Ctrl-C to stop.\n");

            // Hold the future open so `listen()` doesn't return immediately.
            // In a real adapter this would be the accept loop.
            std::future::pending::<()>().await;
        }))
    }
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() {
    println!("RPC Controller Example\n");

    let mut app = ToniFactory::new()
        .create_with(OrdersModule, toni_axum::AxumAdapter::new())
        .await;

    app.use_rpc_adapter(StubRpcAdapter::new()).unwrap();

    app.listen(8080, "127.0.0.1").await;
}
