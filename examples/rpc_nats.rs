// RPC controller example using the NATS transport adapter.
//
// Requires a running NATS server: `nats-server` or `docker run -p 4222:4222 nats`
//
// The NATS subject is the handler pattern — no envelope wrapper needed.
// Request-response uses a NATS reply-to inbox set by the caller; fire-and-forget
// messages have no reply-to and produce no response on the wire.
//
// Test with the NATS CLI (https://github.com/nats-io/natscli):
//
//   # request-response — handler returns Ok(data)
//   nats req order.create '{"item":"keyboard","qty":3}'
//   → {"response":{"id":1001,"item":"keyboard","qty":3,"status":"created"}}
//
//   # error — handler returns Err
//   nats req order.create '{"item":"keyboard","qty":0}'
//   → {"err":{"message":"Internal error: qty must be positive","status":"error"}}
//
//   # fire-and-forget — no reply-to, no response
//   nats pub order.shipped '{"order_id":1001}'

use toni::ToniFactory;
use toni_macros::{injectable, module, rpc_controller};

// ============================================================================
// Service
// ============================================================================

#[injectable(pub struct OrdersService {})]
impl OrdersService {
    pub fn create_order(&self, item: &str, qty: u32) -> serde_json::Value {
        println!("  [OrdersService] Creating order: {} x{}", item, qty);
        serde_json::json!({ "id": 1001, "item": item, "qty": qty, "status": "created" })
    }

    pub fn handle_shipment(&self, order_id: u64) {
        println!("  [OrdersService] Order {} marked as shipped", order_id);
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

    #[message_pattern("order.create")]
    async fn create_order(
        &self,
        data: toni::RpcData,
        _ctx: toni::RpcContext,
    ) -> Result<toni::RpcData, toni::RpcError> {
        let payload = data
            .as_json()
            .ok_or_else(|| toni::RpcError::Internal("expected JSON payload".into()))?;

        let item = payload["item"].as_str().unwrap_or("unknown");
        let qty = payload["qty"].as_u64().unwrap_or(1) as u32;

        if qty == 0 {
            return Err(toni::RpcError::Internal("qty must be positive".into()));
        }

        let order = self.service.create_order(item, qty);
        Ok(toni::RpcData::json(order))
    }

    #[event_pattern("order.shipped")]
    async fn on_order_shipped(
        &self,
        data: toni::RpcData,
        _ctx: toni::RpcContext,
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
// Main
// ============================================================================

#[tokio::main]
async fn main() {
    println!("RPC NATS Example");
    println!("HTTP:         http://127.0.0.1:8080");
    println!("NATS subjects: order.create, order.shipped\n");

    let mut app = ToniFactory::new()
        .create_with(OrdersModule)
        .await;

    app.use_http_adapter(toni_axum::AxumAdapter::new("127.0.0.1", 8080)).unwrap();
    app.use_rpc_adapter(toni_nats::NatsAdapter::new("nats://127.0.0.1:4222"))
        .unwrap();

    app.start().await;
}
