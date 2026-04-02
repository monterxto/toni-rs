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

use serde::{Deserialize, Serialize};
use serde_json::json;
use toni::{
    controller,
    extractors::{Json, Query},
    get, injectable, module, post, Body as ToniBody, RpcClient, ToniFactory,
};
use toni_macros::{provider_value, rpc_controller};

// ============================================================================
// DTOs
// ============================================================================

#[derive(Deserialize)]
struct CreateOrderDto {
    item: String,
    qty: u32,
}

#[derive(Serialize)]
struct OrderDto {
    id: u64,
    item: String,
    qty: u32,
    status: &'static str,
}

#[derive(Deserialize)]
struct ShipOrderDto {
    order_id: u64,
}

// ============================================================================
// Service — shared business logic
// ============================================================================

#[injectable(pub struct OrdersService {})]
impl OrdersService {
    fn create_order(&self, item: &str, qty: u32) -> OrderDto {
        println!("  [OrdersService] Creating order: {} x{}", item, qty);
        OrderDto {
            id: 1001,
            item: item.to_string(),
            qty,
            status: "created",
        }
    }

    fn handle_shipment(&self, order_id: u64) {
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
        payload: CreateOrderDto,
        _ctx: toni::RpcContext,
    ) -> Result<OrderDto, toni::RpcError> {
        if payload.qty == 0 {
            return Err(toni::RpcError::Internal("qty must be positive".into()));
        }
        Ok(self.service.create_order(&payload.item, payload.qty))
    }

    #[event_pattern("order.shipped")]
    async fn on_order_shipped(
        &self,
        payload: ShipOrderDto,
        _ctx: toni::RpcContext,
    ) -> Result<(), toni::RpcError> {
        self.service.handle_shipment(payload.order_id);
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
    async fn create_order(&self, Query(params): Query<CreateOrderDto>) -> ToniBody {
        println!(
            "[HTTP] GET /order/create → calling order.create via RpcClient (item={}, qty={})",
            params.item, params.qty
        );

        let req_dto = json!({ "item": params.item, "qty": params.qty });
        match self
            .client
            .send_json::<_, serde_json::Value>("order.create", &req_dto)
            .await
        {
            Ok(order) => ToniBody::json(order),
            Err(e) => ToniBody::json(json!({ "error": e.to_string() })),
        }
    }

    #[post("/ship")]
    async fn ship_order(&self, Json(payload): Json<serde_json::Value>) -> ToniBody {
        let order_id = payload["order_id"].as_u64().unwrap_or(0);
        println!(
            "[HTTP] POST /order/ship → emitting order.shipped for order_id={} via RpcClient",
            order_id
        );

        match self.client.emit_json("order.shipped", &payload).await {
            Ok(()) => ToniBody::json(json!({ "status": "accepted" })),
            Err(e) => ToniBody::json(json!({ "error": e.to_string() })),
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

    let mut app = ToniFactory::new().create_with(OrdersModule).await;

    app.use_http_adapter(toni_axum::AxumAdapter::new(), 8080, "127.0.0.1")
        .unwrap();
    app.use_rpc_adapter(toni_nats::NatsAdapter::new("nats://127.0.0.1:4222"))
        .unwrap();

    app.start().await;
}
