//! WebSocket support for execution context
//!
//! Provides WebSocket types that integrate with the unified execution context,
//! enabling guards, interceptors, and error handlers to work with WebSocket connections.

mod broadcast;
mod gateway_trait;
mod gateway_wrapper;
pub mod helpers;
mod ws_client;
mod ws_error;
mod ws_message;
mod ws_socket;

pub use broadcast::{
    BroadcastError, BroadcastService, BroadcastTarget, ClientId, ConnectionManager, RoomId,
    SendError, Sender, TrySendError,
};
pub use gateway_trait::GatewayTrait;
pub use gateway_wrapper::GatewayWrapper;
pub use ws_client::{WsClient, WsHandshake};
pub use ws_error::{DisconnectReason, WsError};
pub use ws_message::WsMessage;
pub use ws_socket::WsSocket;
