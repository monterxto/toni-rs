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
    /// Establish the connection to the remote service.
    ///
    /// Called automatically by [`ClientsModule`] during provider initialisation so
    /// that connection failures surface at startup rather than on the first request.
    /// Implementations that use lazy connections (e.g. reconnect on demand) may
    /// leave this as the default no-op.
    ///
    /// [`ClientsModule`]: crate::rpc::ClientsModule
    async fn connect(&self) -> Result<(), RpcClientError> {
        Ok(())
    }

    /// Flush pending messages and close the connection.
    ///
    /// Called by [`RpcClient::close`] when the caller wants an explicit graceful
    /// shutdown. The default is a no-op; transports that buffer outbound data
    /// (e.g. NATS flush) should override this.
    ///
    /// [`RpcClient::close`]: crate::rpc::RpcClient::close
    async fn close(&self) -> Result<(), RpcClientError> {
        Ok(())
    }

    /// Send a message and wait for the remote reply (request-response).
    async fn send(&self, pattern: &str, data: RpcData) -> Result<RpcData, RpcClientError>;

    /// Send a message without waiting for a reply (fire-and-forget).
    async fn emit(&self, pattern: &str, data: RpcData) -> Result<(), RpcClientError>;
}
