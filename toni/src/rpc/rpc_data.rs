use serde::{Deserialize, Serialize};

/// RPC message payload - the actual data being transmitted
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RpcData {
    Json(serde_json::Value),
    Binary(Vec<u8>),
    Text(String),
}

impl RpcData {
    pub fn json(value: serde_json::Value) -> Self {
        Self::Json(value)
    }

    pub fn binary(data: Vec<u8>) -> Self {
        Self::Binary(data)
    }

    pub fn text(data: impl Into<String>) -> Self {
        Self::Text(data.into())
    }

    pub fn as_json(&self) -> Option<&serde_json::Value> {
        match self {
            Self::Json(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_binary(&self) -> Option<&[u8]> {
        match self {
            Self::Binary(data) => Some(data),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(data) => Some(data),
            _ => None,
        }
    }
}
