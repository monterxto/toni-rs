//! tokio-tungstenite adapter for standalone WebSocket deployment with the Toni framework.
//!
//! Provides `TungsteniteAdapter`, which implements `WebSocketAdapter` for separate-port
//! WebSocket servers — gateways that declare `port = N` in the `#[websocket_gateway]` macro
//! are routed here instead of through the HTTP adapter.
//!
//! # Example
//!
//! ```rust,ignore
//! app.use_websocket_adapter(TungsteniteAdapter::new()).unwrap();
//! app.listen(3000, "127.0.0.1").await;
//! // Gateways with port = 4000 automatically bind to 4000 via TungsteniteAdapter.
//! ```

mod adapter;

pub use adapter::TungsteniteAdapter;
