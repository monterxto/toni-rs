use crate::http_helpers::Extensions;
use std::collections::HashMap;

/// RPC execution context - metadata about the RPC call
/// Covers gRPC, Kafka, NATS, Redis, MQTT, RabbitMQ, TCP transports
#[derive(Debug, Clone)]
pub struct RpcContext {
    /// Transport-specific pattern/topic/channel identifier
    pub pattern: String,

    /// Message metadata (headers, properties, etc.)
    pub metadata: HashMap<String, String>,

    /// Type-erased transport-specific extensions
    pub extensions: Extensions,
}

impl RpcContext {
    pub fn new(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            metadata: HashMap::new(),
            extensions: Extensions::new(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn get_metadata(&self, key: &str) -> Option<&str> {
        self.metadata.get(key).map(|s| s.as_str())
    }
}
