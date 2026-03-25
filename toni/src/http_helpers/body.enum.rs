use bytes::Bytes;
use serde_json::Value;

/// An HTTP body with an optional content-type hint for the adapter.
///
/// Use the constructors — the internal representation is not part of the
/// public API.
#[derive(Debug, Clone)]
pub struct Body {
    bytes: Bytes,
    content_type: Option<String>,
}

impl Body {
    /// Plain text body. Sets `Content-Type: text/plain; charset=utf-8`.
    pub fn text(s: impl Into<String>) -> Self {
        Self {
            bytes: Bytes::from(s.into().into_bytes()),
            content_type: Some("text/plain; charset=utf-8".to_string()),
        }
    }

    /// JSON body from a [`serde_json::Value`]. Sets `Content-Type: application/json`.
    pub fn json(value: Value) -> Self {
        Self {
            bytes: Bytes::from(serde_json::to_vec(&value).unwrap_or_default()),
            content_type: Some("application/json".to_string()),
        }
    }

    /// Raw binary body. Sets `Content-Type: application/octet-stream`.
    pub fn binary(data: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: Bytes::from(data.into()),
            content_type: Some("application/octet-stream".to_string()),
        }
    }

    /// Empty body with no content-type.
    pub fn empty() -> Self {
        Self {
            bytes: Bytes::new(),
            content_type: None,
        }
    }

    /// Override or set the content-type.
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// The raw bytes of this body.
    pub fn bytes(&self) -> &Bytes {
        &self.bytes
    }

    /// Whether the body is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// The content-type this body carries, if any.
    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    /// Consume this body and return the raw bytes.
    pub fn into_bytes(self) -> Bytes {
        self.bytes
    }
}

impl From<Bytes> for Body {
    fn from(bytes: Bytes) -> Self {
        Self {
            bytes,
            content_type: None,
        }
    }
}
