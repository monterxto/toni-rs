use std::sync::Arc;

use crate::adapter::RpcClientTransport;
use crate::rpc::{RpcClientError, RpcData};

/// Injectable handle for calling remote RPC services.
///
/// Wraps any [`RpcClientTransport`] and exposes two operations:
///
/// - [`send`] — request-response: dispatches a message and awaits the reply
/// - [`emit`] — fire-and-forget: dispatches a message and returns immediately
///
/// `RpcClient` is `Clone` (clones the inner `Arc`) so it can be shared freely
/// across providers injected from DI.
///
/// # Example
///
/// Register via `provider_value!` inside a module:
///
/// ```rust,no_run
/// provider_value!("INVENTORY_CLIENT", RpcClient::new(NatsClientTransport::new("nats://localhost:4222")))
/// ```
///
/// Inject into a service:
///
/// ```rust,no_run
/// #[injectable(pub struct InventoryService {
///     #[inject(token = "INVENTORY_CLIENT")] client: RpcClient,
/// })]
/// impl InventoryService {
///     async fn notify_restock(&self, payload: serde_json::Value) -> Result<RpcData, RpcClientError> {
///         self.client.send("inventory.restock", RpcData::json(payload)).await
///     }
/// }
/// ```
///
/// [`send`]: RpcClient::send
/// [`emit`]: RpcClient::emit
#[derive(Clone)]
pub struct RpcClient {
    transport: Arc<dyn RpcClientTransport>,
}

impl RpcClient {
    pub fn new(transport: impl RpcClientTransport) -> Self {
        Self {
            transport: Arc::new(transport),
        }
    }

    /// Send a message and wait for the remote reply (request-response).
    pub async fn send(
        &self,
        pattern: impl AsRef<str>,
        data: RpcData,
    ) -> Result<RpcData, RpcClientError> {
        self.transport.send(pattern.as_ref(), data).await
    }

    /// Send a message without waiting for a reply (fire-and-forget).
    pub async fn emit(
        &self,
        pattern: impl AsRef<str>,
        data: RpcData,
    ) -> Result<(), RpcClientError> {
        self.transport.emit(pattern.as_ref(), data).await
    }

    /// Typed request-response: serializes `data` to JSON, sends, and deserializes the reply.
    ///
    /// Shorthand for callers that work with concrete Rust types rather than raw `RpcData`.
    pub async fn send_json<T, R>(
        &self,
        pattern: impl AsRef<str>,
        data: &T,
    ) -> Result<R, RpcClientError>
    where
        T: serde::Serialize,
        R: serde::de::DeserializeOwned,
    {
        let payload = RpcData::from_serialize(data)
            .map_err(|e| RpcClientError::Transport(e.to_string()))?;
        let reply = self.transport.send(pattern.as_ref(), payload).await?;
        reply
            .parse::<R>()
            .map_err(|e| RpcClientError::Transport(e.to_string()))
    }

    /// Typed fire-and-forget: serializes `data` to JSON and emits without waiting for a reply.
    pub async fn emit_json<T>(
        &self,
        pattern: impl AsRef<str>,
        data: &T,
    ) -> Result<(), RpcClientError>
    where
        T: serde::Serialize,
    {
        let payload = RpcData::from_serialize(data)
            .map_err(|e| RpcClientError::Transport(e.to_string()))?;
        self.transport.emit(pattern.as_ref(), payload).await
    }
}
