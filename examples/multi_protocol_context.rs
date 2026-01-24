//! Multi-Protocol Execution Context Examples
//!
//! Demonstrates writing guards and interceptors that work across HTTP, WebSocket,
//! and future RPC protocols using Toni's unified Context.

use std::collections::HashMap;
use toni::{Context, Protocol, ProtocolType, WsClient, WsHandshake, WsMessage};

// ============================================================================
// Universal Guards
// ============================================================================

/// Auth guard works across all protocols by extracting tokens from protocol-specific locations
pub struct UniversalAuthGuard;

impl UniversalAuthGuard {
    pub fn can_activate(&self, context: &Context) -> bool {
        // HTTP uses Bearer tokens in headers, WebSocket uses query params during handshake
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
        };

        token.map_or(false, |t| self.validate_token(t))
    }

    fn validate_token(&self, token: &str) -> bool {
        token == "valid-secret-token"
    }
}

// ============================================================================
// Universal Interceptors
// ============================================================================
pub struct LoggingInterceptor;

impl LoggingInterceptor {
    pub fn log_request(&self, context: &Context) {
        match context.protocol_type() {
            ProtocolType::Http => {
                let (request, _) = context.switch_to_http().unwrap();
                println!(
                    "[HTTP] {} {} from {:?}",
                    request.method,
                    request.uri,
                    request.header("user-agent")
                );
            }
            ProtocolType::WebSocket => {
                let (client, message, event) = context.switch_to_ws().unwrap();
                println!(
                    "[WebSocket] event='{}' client={} message={:?}",
                    event, client.id, message
                );
            }
        }
    }
}

/// Rate limiter extracts client identifiers from protocol-specific sources
pub struct RateLimitGuard {
    _max_requests: u32,
}

impl RateLimitGuard {
    pub fn new(max_requests: u32) -> Self {
        Self {
            _max_requests: max_requests,
        }
    }

    pub fn can_activate(&self, context: &Context) -> bool {
        let client_id = match context.protocol_type() {
            ProtocolType::Http => {
                let (request, _) = context.switch_to_http().expect("HTTP context");
                request
                    .header("x-client-id")
                    .unwrap_or("anonymous")
                    .to_string()
            }
            ProtocolType::WebSocket => {
                let (client, _, _) = context.switch_to_ws().expect("WebSocket context");
                client.id.clone()
            }
        };

        self.check_rate_limit(&client_id)
    }

    fn check_rate_limit(&self, _client_id: &str) -> bool {
        // Stub: real implementation would check Redis/in-memory store
        true
    }
}

// ============================================================================
// Example Usage
// ============================================================================

fn main() {
    println!("=== Multi-Protocol Context Examples ===\n");

    println!("--- HTTP Protocol ---");
    let http_protocol = Protocol::http(create_mock_http_request());
    let http_context = create_mock_context(http_protocol);

    let auth_guard = UniversalAuthGuard;
    println!("HTTP Auth: {}", auth_guard.can_activate(&http_context));

    let logger = LoggingInterceptor;
    logger.log_request(&http_context);

    let rate_limiter = RateLimitGuard::new(100);
    println!(
        "HTTP Rate Limit: {}\n",
        rate_limiter.can_activate(&http_context)
    );

    println!("--- WebSocket Protocol ---");
    let ws_protocol = Protocol::websocket(
        create_mock_ws_client(),
        WsMessage::text(r#"{"action":"subscribe","channel":"updates"}"#),
        "message",
    );
    let ws_context = create_mock_context(ws_protocol);

    println!("WebSocket Auth: {}", auth_guard.can_activate(&ws_context));
    logger.log_request(&ws_context);
    println!(
        "WebSocket Rate Limit: {}",
        rate_limiter.can_activate(&ws_context)
    );
}

// ============================================================================
// Mock Helpers (for demonstration only)
// ============================================================================

fn create_mock_http_request() -> toni::http_helpers::HttpRequest {
    // Mock implementation - in real usage this comes from the HTTP server
    unimplemented!("Mock HTTP request for example purposes")
}

fn create_mock_ws_client() -> WsClient {
    let mut query = HashMap::new();
    query.insert("token".to_string(), "valid-secret-token".to_string());

    WsClient {
        id: "client-123".to_string(),
        handshake: WsHandshake {
            query,
            headers: HashMap::new(),
            remote_addr: Some("127.0.0.1:8080".to_string()),
        },
        extensions: Default::default(),
    }
}

fn create_mock_context(_protocol: Protocol) -> Context {
    // Mock implementation - in real usage this is created by the framework
    unimplemented!("Mock context for example purposes")
}
