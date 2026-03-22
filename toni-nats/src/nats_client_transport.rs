use std::time::Duration;

use bytes::Bytes;
use tokio::sync::OnceCell;
use toni::{async_trait, RpcClientError, RpcClientTransport, RpcData};

/// NATS transport for [`RpcClient`].
///
/// Connections are established lazily on the first [`send`] or [`emit`] call so
/// the struct can be constructed synchronously inside a `provider_value!` or
/// `provider_factory!` block.
///
/// # Example
///
/// ```rust,no_run
/// provider_value!(
///     "INVENTORY_CLIENT",
///     toni::RpcClient::new(toni_nats::NatsClientTransport::new("nats://localhost:4222"))
/// )
/// ```
///
/// [`RpcClient`]: toni::RpcClient
/// [`send`]: NatsClientTransport::send
/// [`emit`]: NatsClientTransport::emit
pub struct NatsClientTransport {
    url: String,
    timeout: Duration,
    client: OnceCell<async_nats::Client>,
}

impl NatsClientTransport {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            timeout: Duration::from_secs(5),
            client: OnceCell::new(),
        }
    }

    /// Override the request-response timeout (default: 5 s).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    async fn get_or_connect(&self) -> Result<&async_nats::Client, RpcClientError> {
        let url = self.url.clone();
        self.client
            .get_or_try_init(|| async move {
                async_nats::connect(&url)
                    .await
                    .map_err(|e| RpcClientError::Transport(e.to_string()))
            })
            .await
    }
}

fn data_to_bytes(data: RpcData) -> Bytes {
    match data {
        RpcData::Json(v) => Bytes::from(v.to_string()),
        RpcData::Text(s) => Bytes::from(s.into_bytes()),
        RpcData::Binary(b) => Bytes::from(b),
    }
}

/// Parse the toni RPC response envelope produced by `NatsAdapter`:
/// `{"response":<json>}` or `{"err":{"message":"...","status":"..."}}`.
///
/// Falls back to raw binary if the payload is not a recognized envelope.
fn parse_response(bytes: &[u8]) -> Result<RpcData, RpcClientError> {
    match serde_json::from_slice::<serde_json::Value>(bytes) {
        Ok(v) => {
            if let Some(response) = v.get("response") {
                Ok(RpcData::json(response.clone()))
            } else if let Some(err) = v.get("err") {
                let message = err
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error")
                    .to_string();
                let status = err
                    .get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("error")
                    .to_string();
                Err(RpcClientError::Remote { message, status })
            } else {
                Ok(RpcData::json(v))
            }
        }
        Err(_) => Ok(RpcData::Binary(bytes.to_vec())),
    }
}

#[async_trait]
impl RpcClientTransport for NatsClientTransport {
    async fn connect(&self) -> Result<(), RpcClientError> {
        self.get_or_connect().await?;
        Ok(())
    }

    async fn close(&self) -> Result<(), RpcClientError> {
        if let Some(client) = self.client.get() {
            client
                .flush()
                .await
                .map_err(|e| RpcClientError::Transport(e.to_string()))?;
        }
        Ok(())
    }

    async fn send(&self, pattern: &str, data: RpcData) -> Result<RpcData, RpcClientError> {
        let client = self.get_or_connect().await?;
        let subject = pattern.to_string();
        let payload = data_to_bytes(data);

        let response = tokio::time::timeout(self.timeout, client.request(subject, payload))
            .await
            .map_err(|_| RpcClientError::Timeout)?
            .map_err(|e| RpcClientError::Transport(e.to_string()))?;

        parse_response(&response.payload)
    }

    async fn emit(&self, pattern: &str, data: RpcData) -> Result<(), RpcClientError> {
        let client = self.get_or_connect().await?;
        let subject = pattern.to_string();
        let payload = data_to_bytes(data);

        client
            .publish(subject, payload)
            .await
            .map_err(|e| RpcClientError::Transport(e.to_string()))
    }
}
