// RPC controller example using the TCP transport adapter.
//
// This example demonstrates:
// 1. Defining an RPC controller with #[rpc_controller]
// 2. Request-response handlers via #[message_pattern]
// 3. Fire-and-forget handlers via #[event_pattern]
// 4. Returning errors — the adapter sends an error envelope back to the caller
// 5. Automatic DI wiring — the controller is discovered from the module
// 6. Wiring the TcpAdapter as the RPC transport
//
// Wire protocol: newline-delimited JSON over TCP on port 4000.
//
// Test with netcat:
//
//   # success — handler returns Ok(data)
//   echo '{"pattern":"order.create","data":{"item":"keyboard","qty":3},"id":"req-1"}' | nc 127.0.0.1 4000
//   → {"id":"req-1","response":{"id":1001,"item":"keyboard","qty":3,"status":"created"}}
//
//   # error — handler returns Err; adapter sends an error envelope instead of hanging
//   echo '{"pattern":"order.create","data":{"item":"keyboard","qty":0},"id":"req-2"}' | nc 127.0.0.1 4000
//   → {"id":"req-2","err":{"message":"Internal error: qty must be positive","status":"error"}}
//
//   # unknown pattern — framework returns PatternNotFound
//   echo '{"pattern":"does.not.exist","data":{},"id":"req-3"}' | nc 127.0.0.1 4000
//   → {"id":"req-3","err":{"message":"Pattern not found: does.not.exist","status":"not_found"}}
//
//   # fire-and-forget (no id → no reply regardless of outcome)
//   echo '{"pattern":"order.shipped","data":{"order_id":1001}}' | nc 127.0.0.1 4000

use toni::ToniFactory;
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
    println!("RPC Controller Example");
    println!("HTTP: http://127.0.0.1:8080");
    println!("RPC (TCP): 127.0.0.1:4000\n");

    let mut app = ToniFactory::new()
        .create_with(OrdersModule, toni_axum::AxumAdapter::new())
        .await;

    app.use_rpc_adapter(toni_tcp::TcpAdapter::new(4000)).unwrap();

    app.listen(8080, "127.0.0.1").await;
}
