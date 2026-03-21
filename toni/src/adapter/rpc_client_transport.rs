use async_trait::async_trait;

use crate::rpc::{RpcClientError, RpcData};

/// Interface for RPC client transports.
///
/// Transport crates implement this trait to send messages to remote services.
/// `RpcClient` wraps any `RpcClientTransport` and provides the user-facing API.
///
/// - [`send`] — request-response: waits for a reply
/// - [`emit`] — fire-and-forget: returns once the message is dispatched
///
/// [`send`]: RpcClientTransport::send
/// [`emit`]: RpcClientTransport::emit
#[async_trait]
pub trait RpcClientTransport: Send + Sync + 'static {
    /// Send a message and wait for the remote reply (request-response).
    async fn send(&self, pattern: &str, data: RpcData) -> Result<RpcData, RpcClientError>;

    /// Send a message without waiting for a reply (fire-and-forget).
    async fn emit(&self, pattern: &str, data: RpcData) -> Result<(), RpcClientError>;
}
