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

    /// Deserialize the payload into a typed value.
    ///
    /// All three variants are tried as JSON — `Json` via `from_value`, `Text`
    /// via `from_str`, and `Binary` via `from_slice`.
    pub fn parse<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        match self {
            Self::Json(v) => serde_json::from_value(v.clone()),
            Self::Text(s) => serde_json::from_str(s),
            Self::Binary(b) => serde_json::from_slice(b),
        }
    }

    /// Serialize a value into an `RpcData::Json` payload.
    pub fn from_serialize<T: serde::Serialize>(v: &T) -> Result<Self, serde_json::Error> {
        serde_json::to_value(v).map(Self::Json)
    }
}

impl From<serde_json::Value> for RpcData {
    fn from(v: serde_json::Value) -> Self {
        Self::Json(v)
    }
}
