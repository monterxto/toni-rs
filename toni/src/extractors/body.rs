//! Generic body extractor that auto-detects content type

use serde::de::DeserializeOwned;

use super::FromRequest;
use crate::http_helpers::HttpRequest;

/// Extractor for request body that auto-detects content type.
///
/// Supports `application/json` and `application/x-www-form-urlencoded`.
/// For raw binary data, use the [`Bytes`](super::bytes::Bytes) extractor.
///
/// # Example
///
/// ```rust,ignore
/// #[derive(Deserialize)]
/// struct CreateUserDto {
///     name: String,
///     email: String,
/// }
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
}

impl std::fmt::Display for BodyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BodyError::UnsupportedContentType(ct) => {
                write!(f, "Unsupported content type: {}", ct)
            }
            BodyError::DeserializeError(msg) => write!(f, "Failed to deserialize body: {}", msg),
        }
    }
}

impl std::error::Error for BodyError {}

impl<T: DeserializeOwned> FromRequest for Body<T> {
    type Error = BodyError;

    fn from_request(req: &HttpRequest) -> Result<Self, Self::Error> {
        if req.body.is_empty() {
            return Err(BodyError::DeserializeError("Empty body".to_string()));
        }

        let content_type = req
            .headers
            .iter()
            .find(|(name, _)| name.to_lowercase() == "content-type")
            .map(|(_, value)| value.to_lowercase())
            .unwrap_or_default();

        if content_type.contains("application/json") {
            let parsed: T = serde_json::from_slice(&req.body)
                .map_err(|e| BodyError::DeserializeError(e.to_string()))?;
            Ok(Body(parsed))
        } else if content_type.contains("application/x-www-form-urlencoded") {
            let parsed: T = serde_urlencoded::from_bytes(&req.body)
                .map_err(|e| BodyError::DeserializeError(e.to_string()))?;
            Ok(Body(parsed))
        } else if content_type.is_empty() {
            if let Ok(parsed) = serde_json::from_slice::<T>(&req.body) {
                return Ok(Body(parsed));
            }
            let parsed: T = serde_urlencoded::from_bytes(&req.body)
                .map_err(|e| BodyError::DeserializeError(e.to_string()))?;
            Ok(Body(parsed))
        } else {
            Err(BodyError::UnsupportedContentType(content_type))
        }
    }
}
