//! WebSocket support for execution context
//!
//! Provides WebSocket types that integrate with the unified execution context,
//! enabling guards, interceptors, and error handlers to work with WebSocket connections.

mod ws_client;
mod ws_message;

pub use ws_client::{WsClient, WsHandshake};
pub use ws_message::WsMessage;
