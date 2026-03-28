use std::any::Any;
use std::sync::Arc;

use crate::adapter::RpcClientTransport;
use crate::async_trait;
use crate::provider_scope::ProviderScope;
use crate::rpc::{RpcClientError, RpcData};
use crate::traits_helpers::{ProviderContext, Provider};

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
/// Register via `provide_factory!` with the `lifecycle` flag inside a module's
/// `providers` list:
///
/// ```ignore
/// provide_factory!("INVENTORY_CLIENT", |config: ConfigService| {
///     RpcClient::new(NatsClientTransport::new(config.get("NATS_URL")))
/// }, lifecycle)
/// ```
///
/// Inject into a service:
///
/// ```ignore
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
        let payload =
            RpcData::from_serialize(data).map_err(|e| RpcClientError::Transport(e.to_string()))?;
        let reply = self.transport.send(pattern.as_ref(), payload).await?;
        reply
            .parse::<R>()
            .map_err(|e| RpcClientError::Transport(e.to_string()))
    }

    /// Establish the connection to the remote service eagerly.
    ///
    /// Transports are lazy by default — they connect on the first `send` or `emit`.
    /// Call this explicitly (e.g. in an `#[on_application_bootstrap]` hook) when
    /// you want to surface connection failures at startup rather than on the first
    /// request.
    pub async fn connect(&self) -> Result<(), RpcClientError> {
        self.transport.connect().await
    }

    /// Gracefully close the connection to the remote service.
    ///
    /// Flushes any pending messages before closing. Call this in an
    /// `#[on_application_shutdown]` hook to ensure in-flight data is not lost
    /// before the process exits.
    pub async fn close(&self) -> Result<(), RpcClientError> {
        self.transport.close().await
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
        let payload =
            RpcData::from_serialize(data).map_err(|e| RpcClientError::Transport(e.to_string()))?;
        self.transport.emit(pattern.as_ref(), payload).await
    }
}

#[async_trait]
impl Provider for RpcClient {
    fn get_token(&self) -> String {
        std::any::type_name::<Self>().to_string()
    }

    fn get_token_factory(&self) -> String {
        std::any::type_name::<Self>().to_string()
    }

    async fn execute(
        &self,
        _params: Vec<Box<dyn Any + Send>>,
        _ctx: ProviderContext<'_>,
    ) -> Box<dyn Any + Send> {
        Box::new(self.clone())
    }

    fn get_scope(&self) -> ProviderScope {
        ProviderScope::Singleton
    }

    async fn on_application_bootstrap(&self) {
        if let Err(e) = self.connect().await {
            eprintln!("[RpcClient] connect failed at bootstrap: {e}");
        }
    }

    async fn on_application_shutdown(&self, _signal: Option<String>) {
        if let Err(e) = self.close().await {
            eprintln!("[RpcClient] close failed at shutdown: {e}");
        }
    }
}
