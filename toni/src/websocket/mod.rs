//! WebSocket support for execution context
//!
//! Provides WebSocket types that integrate with the unified execution context,
//! enabling guards, interceptors, and error handlers to work with WebSocket connections.

mod gateway_trait;
mod gateway_wrapper;
mod ws_client;
mod ws_error;
mod ws_message;

pub use gateway_trait::GatewayTrait;
pub use gateway_wrapper::GatewayWrapper;
pub use ws_client::{WsClient, WsHandshake};
pub use ws_error::{DisconnectReason, WsError};
pub use ws_message::WsMessage;
