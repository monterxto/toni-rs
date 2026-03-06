//! WebSocket support for execution context
//!
//! Provides WebSocket types that integrate with the unified execution context,
//! enabling guards, interceptors, and error handlers to work with WebSocket connections.

mod broadcast;
mod broadcast_module;
mod broadcast_provider;
mod connection_manager_provider;
mod gateway_trait;
mod gateway_wrapper;
mod handle;
pub mod helpers;
mod ws_client;
mod ws_error;
mod ws_message;

pub use broadcast::{
    BroadcastError, BroadcastService, BroadcastTarget, ClientId, ConnectionManager, RoomId,
    SendError, Sender, TrySendError,
};
pub use broadcast_module::BroadcastModule;
pub use broadcast_provider::{BroadcastServiceManager, BroadcastServiceProvider};
pub use connection_manager_provider::{ConnectionManagerManager, ConnectionManagerProvider};
pub use gateway_trait::GatewayTrait;
pub use gateway_wrapper::GatewayWrapper;
pub use handle::WsGatewayHandle;
pub use ws_client::{WsClient, WsHandshake};
pub use ws_error::{DisconnectReason, WsError};
pub use ws_message::WsMessage;
