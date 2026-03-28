use serde::de::DeserializeOwned;

use super::FromRequest;
use crate::http_helpers::HttpRequest;

/// Extracts and deserializes the request body, auto-detecting content type.
///
/// Supports `application/json` and `application/x-www-form-urlencoded`.
/// For raw bytes, use the [`Bytes`](super::bytes::Bytes) extractor instead.
/// For a raw stream, use [`BodyStream`](super::body_stream::BodyStream).
///
/// # Example
///
/// ```rust,ignore
/// #[derive(Deserialize)]
/// struct CreateUserDto { name: String, email: String }
///
/// #[post("/users")]
/// fn create_user(&self, Body(dto): Body<CreateUserDto>) -> String {
///     format!("Created user: {}", dto.name)
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Body<T>(pub T);

impl<T> Body<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> std::ops::Deref for Body<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::DerefMut for Body<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug)]
pub enum BodyError {
    UnsupportedContentType(String),
    DeserializeError(String),
    CollectError(String),
}

impl std::fmt::Display for BodyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BodyError::UnsupportedContentType(ct) => {
                write!(f, "Unsupported content type: {}", ct)
            }
            BodyError::DeserializeError(msg) => write!(f, "Failed to deserialize body: {}", msg),
            BodyError::CollectError(msg) => write!(f, "Failed to read request body: {}", msg),
        }
    }
}

impl std::error::Error for BodyError {}

fn parse_bytes<T: DeserializeOwned>(bytes: &[u8], content_type: &str) -> Result<T, BodyError> {
    if content_type.contains("application/json") {
        serde_json::from_slice(bytes).map_err(|e| BodyError::DeserializeError(e.to_string()))
    } else if content_type.contains("application/x-www-form-urlencoded") {
        serde_urlencoded::from_bytes(bytes).map_err(|e| BodyError::DeserializeError(e.to_string()))
    } else if content_type.is_empty() {
        if let Ok(v) = serde_json::from_slice(bytes) {
            return Ok(v);
        }
        serde_urlencoded::from_bytes(bytes).map_err(|e| BodyError::DeserializeError(e.to_string()))
    } else {
        Err(BodyError::UnsupportedContentType(content_type.to_string()))
    }
}

impl<T: DeserializeOwned + Send> FromRequest for Body<T> {
    type Error = BodyError;

    async fn from_request(req: HttpRequest) -> Result<Self, Self::Error> {
        let (parts, body) = req.into_parts();
        let bytes = body
            .collect()
            .await
            .map_err(|e| BodyError::CollectError(e.to_string()))?;
        if bytes.is_empty() {
            return Err(BodyError::DeserializeError("Empty body".to_string()));
        }
        let content_type = parts
            .headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();
        Ok(Body(parse_bytes(&bytes, &content_type)?))
    }
}
