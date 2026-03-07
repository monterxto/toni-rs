//! WebSocket support for execution context
//!
//! Provides WebSocket types that integrate with the unified execution context,
//! enabling guards, interceptors, and error handlers to work with WebSocket connections.

mod broadcast;
mod broadcast_module;
mod broadcast_provider;
mod gateway_trait;
mod gateway_wrapper;
pub mod helpers;
mod ws_client;
mod ws_client_map;
mod ws_error;
mod ws_message;

pub(crate) use broadcast::ConnectionManager;
pub use broadcast::{
    BroadcastError, BroadcastService, BroadcastTarget, ClientId, RoomId, SendError, TrySendError,
    WsSink,
};
pub use broadcast_module::BroadcastModule;
pub use gateway_trait::GatewayTrait;
pub use gateway_wrapper::GatewayWrapper;
pub use ws_client::{WsClient, WsHandshake};
pub(crate) use ws_client_map::WsClientMap;
pub use ws_error::{DisconnectReason, WsError};
pub use ws_message::WsMessage;
