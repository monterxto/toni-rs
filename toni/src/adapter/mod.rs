mod websocket_adapter;

pub(crate) use websocket_adapter::ErasedWebSocketAdapter;
pub use websocket_adapter::{WebSocketAdapter, WsConnectionCallbacks};
