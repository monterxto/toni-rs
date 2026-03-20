mod rpc_adapter;
mod websocket_adapter;

pub(crate) use rpc_adapter::ErasedRpcAdapter;
pub use rpc_adapter::{RpcAdapter, RpcMessageCallbacks};
pub(crate) use websocket_adapter::ErasedWebSocketAdapter;
pub use websocket_adapter::{WebSocketAdapter, WsConnectionCallbacks};
