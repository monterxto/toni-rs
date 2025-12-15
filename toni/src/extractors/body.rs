//! Generic body extractor that auto-detects content type

use serde::de::DeserializeOwned;

use super::FromRequest;
use crate::http_helpers::{Body as HttpBody, HttpRequest};

/// Extractor for request body that auto-detects content type
///
/// Supports:
/// - `application/json` - parses as JSON
/// - `application/x-www-form-urlencoded` - parses as form data
///
/// For raw binary data (`application/octet-stream`), use the `Bytes` extractor instead.
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
///     // Works with both JSON and form data
///     format!("Created user: {}", dto.name)
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Body<T>(pub T);

impl<T> Body<T> {
    /// Extract the inner value
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

/// Error type for body extraction
#[derive(Debug)]
pub enum BodyError {
    /// Content type not supported
    UnsupportedContentType(String),
    /// Failed to deserialize body
    DeserializeError(String),
}

impl std::fmt::Display for BodyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BodyError::UnsupportedContentType(ct) => {
                write!(f, "Unsupported content type: {}", ct)
            }
            BodyError::DeserializeError(msg) => {
                write!(f, "Failed to deserialize body: {}", msg)
            }
        }
    }
}

impl std::error::Error for BodyError {}

impl<T: DeserializeOwned> FromRequest for Body<T> {
    type Error = BodyError;

    fn from_request(req: &HttpRequest) -> Result<Self, Self::Error> {
        // Get content type from headers
        let content_type = req
            .headers
            .iter()
            .find(|(name, _)| name.to_lowercase() == "content-type")
            .map(|(_, value)| value.to_lowercase())
            .unwrap_or_default();

        if content_type.contains("application/json") {
            // Parse as JSON
            match &req.body {
                HttpBody::Json(value) => {
                    let parsed: T = serde_json::from_value(value.clone())
                        .map_err(|e| BodyError::DeserializeError(e.to_string()))?;
                    Ok(Body(parsed))
                }
                HttpBody::Text(text) => {
                    // Try to parse text as JSON
                    let parsed: T = serde_json::from_str(text)
                        .map_err(|e| BodyError::DeserializeError(e.to_string()))?;
                    Ok(Body(parsed))
                }
                HttpBody::Binary(_) => Err(BodyError::DeserializeError(
                    "Expected JSON but got binary data".to_string(),
                )),
            }
        } else if content_type.contains("application/x-www-form-urlencoded") {
            // Parse as form data
            match &req.body {
                HttpBody::Text(text) => {
                    let parsed: T = serde_urlencoded::from_str(text)
                        .map_err(|e| BodyError::DeserializeError(e.to_string()))?;
                    Ok(Body(parsed))
                }
                HttpBody::Json(_) => Err(BodyError::DeserializeError(
                    "Expected form data but got JSON".to_string(),
                )),
                HttpBody::Binary(_) => Err(BodyError::DeserializeError(
                    "Expected form data but got binary data".to_string(),
                )),
            }
        } else if content_type.is_empty() {
            // No content type - try JSON first, then form
            match &req.body {
                HttpBody::Json(value) => {
                    let parsed: T = serde_json::from_value(value.clone())
                        .map_err(|e| BodyError::DeserializeError(e.to_string()))?;
                    Ok(Body(parsed))
                }
                HttpBody::Text(text) => {
                    // Try JSON first
                    if let Ok(parsed) = serde_json::from_str::<T>(text) {
                        return Ok(Body(parsed));
                    }
                    // Fall back to form data
                    let parsed: T = serde_urlencoded::from_str(text)
                        .map_err(|e| BodyError::DeserializeError(e.to_string()))?;
                    Ok(Body(parsed))
                }
                HttpBody::Binary(_) => Err(BodyError::DeserializeError(
                    "Cannot deserialize binary data. Use Bytes extractor for raw binary data"
                        .to_string(),
                )),
            }
        } else {
            Err(BodyError::UnsupportedContentType(content_type))
        }
    }
}
