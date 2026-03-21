// RPC client example — calling a remote service from inside a toni app.
//
// Requires a running NATS server: `nats-server` or `docker run -p 4222:4222 nats`
//
// This app acts as both server and client on the same NATS connection to keep
// the example self-contained. In production the client and server would live in
// separate processes/services.
//
// HTTP endpoints:
//   GET /order/create?item=keyboard&qty=3  → calls "order.create" via RpcClient, returns reply
//   POST /order/ship                        → emits "order.shipped" (fire-and-forget)
//
// NATS handlers (served by the same process):
//   order.create   → returns the created order JSON
//   order.shipped  → logs the shipment
//
// Run:
//   cargo run --example rpc_nats_client
//
// Test:
//   curl "http://localhost:8080/order/create?item=keyboard&qty=3"
//   curl -X POST "http://localhost:8080/order/ship" \
//        -H 'Content-Type: application/json' \
//        -d '{"order_id":1001}'

use serde_json::json;
use toni::{
    Body as ToniBody, HttpRequest, RpcClient, RpcData, ToniFactory,
    controller, get, injectable, module, post,
};
use toni_macros::{event_pattern, message_pattern, provider_value, rpc_controller};

// ============================================================================
// Service — shared business logic
// ============================================================================

#[injectable(pub struct OrdersService {})]
impl OrdersService {
    pub fn create_order(&self, item: &str, qty: u32) -> serde_json::Value {
        println!("  [OrdersService] Creating order: {} x{}", item, qty);
        json!({ "id": 1001, "item": item, "qty": qty, "status": "created" })
    }

    pub fn handle_shipment(&self, order_id: u64) {
        println!("  [OrdersService] Order {} marked as shipped", order_id);
    }
}

// ============================================================================
// RPC controller — receives messages from NATS
// ============================================================================

#[rpc_controller(pub struct OrdersRpcController {
    #[inject] service: OrdersService,
})]
impl OrdersRpcController {
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
// HTTP controller — uses RpcClient to call the NATS handlers above
// ============================================================================

#[controller(
    "/order",
    pub struct OrdersHttpController {
        // Injected by the "ORDER_SERVICE_CLIENT" token registered in the module.
        #[inject("ORDER_SERVICE_CLIENT")]
        client: RpcClient,
    }
)]
impl OrdersHttpController {
    #[get("/create")]
    async fn create_order(&self, req: HttpRequest) -> ToniBody {
        let item = req
            .query_params
            .get("item")
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());

        let qty: u32 = req
            .query_params
            .get("qty")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);

        println!(
            "[HTTP] GET /order/create → calling order.create via RpcClient (item={}, qty={})",
            item, qty
        );

        match self
            .client
            .send("order.create", RpcData::json(json!({ "item": item, "qty": qty })))
            .await
        {
            Ok(reply) => ToniBody::Json(
                reply
                    .as_json()
                    .cloned()
                    .unwrap_or_else(|| json!({"error": "non-JSON reply"})),
            ),
            Err(e) => ToniBody::Json(json!({ "error": e.to_string() })),
        }
    }

    #[post("/ship")]
    async fn ship_order(&self, req: HttpRequest) -> ToniBody {
        let payload = match &req.body {
            ToniBody::Json(v) => v.clone(),
            ToniBody::Text(s) => serde_json::from_str(s).unwrap_or_else(|_| json!({})),
            ToniBody::Binary(_) => json!({}),
        };

        let order_id = payload["order_id"].as_u64().unwrap_or(0);
        println!(
            "[HTTP] POST /order/ship → emitting order.shipped for order_id={} via RpcClient",
            order_id
        );

        match self
            .client
            .emit("order.shipped", RpcData::json(payload))
            .await
        {
            Ok(()) => ToniBody::Json(json!({ "status": "accepted" })),
            Err(e) => ToniBody::Json(json!({ "error": e.to_string() })),
        }
    }
}

// ============================================================================
// Module
// ============================================================================

#[module(
    providers: [
        OrdersService,
        OrdersRpcController,
        // Register the RpcClient under a named token so it can be injected.
        provider_value!(
            "ORDER_SERVICE_CLIENT",
            toni::RpcClient::new(toni_nats::NatsClientTransport::new("nats://127.0.0.1:4222"))
        ),
    ],
    controllers: [OrdersHttpController],
)]
struct OrdersModule;

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() {
    println!("RPC NATS Client Example");
    println!("HTTP: http://127.0.0.1:8080");
    println!("  GET  /order/create?item=keyboard&qty=3  → request-response via RpcClient");
    println!("  POST /order/ship   {{\"order_id\":1001}}    → fire-and-forget via RpcClient");
    println!();

    let mut app = ToniFactory::new()
        .create_with(OrdersModule, toni_axum::AxumAdapter::new())
        .await;

    app.use_rpc_adapter(toni_nats::NatsAdapter::new("nats://127.0.0.1:4222"))
        .unwrap();

    app.listen(8080, "127.0.0.1").await;
}
