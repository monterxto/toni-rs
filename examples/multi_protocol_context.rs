//! Writing guards and interceptors that work across HTTP, WebSocket, and RPC
//!
//! toni routes all three protocols through a unified Context. Guards and
//! interceptors receive the same type regardless of protocol — they switch
//! on `context.protocol_type()` to extract the protocol-specific data they need.
//!
//! Run with:  cargo run --example multi_protocol_context

use std::collections::HashMap;
use toni::{
    Body, Context, HttpRequest, ProtocolType, RpcContext, RpcData, WsClient, WsMessage,
};
use toni::websocket::WsHandshake;

// ---- universal auth guard ----------------------------------------------------
//
// HTTP:      Bearer token in Authorization header
// WebSocket: token query param in the upgrade handshake
// RPC:       authorization metadata key

struct UniversalAuthGuard;

impl UniversalAuthGuard {
    fn can_activate(&self, context: &Context) -> bool {
        let token = match context.protocol_type() {
            ProtocolType::Http => {
                let (request, _) = context.switch_to_http().expect("HTTP context");
                request
                    .header("authorization")
                    .and_then(|h| h.strip_prefix("Bearer "))
            }
            ProtocolType::WebSocket => {
                let (client, _, _) = context.switch_to_ws().expect("WebSocket context");
                client.handshake.query.get("token").map(|s| s.as_str())
            }
            ProtocolType::Rpc => {
                let (_, rpc_ctx) = context.switch_to_rpc().expect("RPC context");
                rpc_ctx.get_metadata("authorization")
            }
        };

        token.map_or(false, |t| t == "valid-secret")
    }
}

// ---- universal logging interceptor -------------------------------------------

struct LoggingInterceptor;

impl LoggingInterceptor {
    fn log_request(&self, context: &Context) {
        match context.protocol_type() {
            ProtocolType::Http => {
                let (req, _) = context.switch_to_http().unwrap();
                println!(
                    "[HTTP]      {} {} (agent: {:?})",
                    req.method,
                    req.uri,
                    req.header("user-agent")
                );
            }
            ProtocolType::WebSocket => {
                let (client, message, event) = context.switch_to_ws().unwrap();
                println!(
                    "[WebSocket] event='{}' client={} message={:?}",
                    event, client.id, message
                );
            }
            ProtocolType::Rpc => {
                let (data, rpc_ctx) = context.switch_to_rpc().unwrap();
                println!("[RPC]       pattern='{}' data={:?}", rpc_ctx.pattern, data);
            }
        }
    }
}

// ---- main --------------------------------------------------------------------

fn main() {
    let guard = UniversalAuthGuard;
    let logger = LoggingInterceptor;

    // HTTP — valid token in Authorization header
    println!("--- HTTP ---");
    let http_ctx = Context::from_request(HttpRequest {
        method: "GET".to_string(),
        uri: "/api/orders".to_string(),
        headers: vec![
            ("authorization".to_string(), "Bearer valid-secret".to_string()),
            ("user-agent".to_string(), "example/1.0".to_string()),
        ],
        body: Body::Text(String::new()),
        query_params: HashMap::new(),
        path_params: HashMap::new(),
        extensions: Default::default(),
    });
    logger.log_request(&http_ctx);
    println!("auth: {}\n", guard.can_activate(&http_ctx));

    // WebSocket — token in handshake query params
    println!("--- WebSocket ---");
    let ws_ctx = Context::from_websocket(
        WsClient {
            id: "client-123".to_string(),
            handshake: WsHandshake {
                query: HashMap::from([("token".to_string(), "valid-secret".to_string())]),
                headers: HashMap::new(),
                remote_addr: Some("127.0.0.1:8080".to_string()),
            },
            extensions: Default::default(),
        },
        WsMessage::text(r#"{"action":"subscribe","channel":"updates"}"#),
        "message",
        None,
    );
    logger.log_request(&ws_ctx);
    println!("auth: {}\n", guard.can_activate(&ws_ctx));

    // RPC — authorization in metadata
    println!("--- RPC ---");
    let rpc_ctx = Context::from_rpc(
        RpcData::json(serde_json::json!({"order_id": 123})),
        RpcContext::new("order.process")
            .with_metadata("authorization", "valid-secret")
            .with_metadata("client-id", "service-456"),
        None,
    );
    logger.log_request(&rpc_ctx);
    println!("auth: {}", guard.can_activate(&rpc_ctx));
}
