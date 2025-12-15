//! Raw bytes extractor for binary data
//!
//! Use this extractor when you need to handle raw binary data (application/octet-stream)
//! instead of deserializing structured data.

use super::FromRequest;
use crate::http_helpers::{Body as HttpBody, HttpRequest};

/// Extractor for raw binary data
///
/// This extractor handles `application/octet-stream` content type and returns
/// the raw bytes as `Vec<u8>`. This matches NestJS `@Body()` behavior when
/// receiving Buffer data.
///
/// # Example
///
/// ```rust,ignore
/// #[post("/upload")]
/// fn upload_file(&self, Bytes(data): Bytes) -> String {
///     format!("Uploaded {} bytes", data.len())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Bytes(pub Vec<u8>);

impl Bytes {
    /// Extract the inner bytes
    pub fn into_inner(self) -> Vec<u8> {
        self.0
    }

    /// Get a reference to the inner bytes
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

impl std::ops::Deref for Bytes {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Bytes {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Error type for bytes extraction
#[derive(Debug)]
pub enum BytesError {
    /// Content type not supported for binary data
    UnsupportedContentType(String),
    /// No body provided
    MissingBody,
}

impl std::fmt::Display for BytesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BytesError::UnsupportedContentType(ct) => {
                write!(f, "Unsupported content type for binary data: {}", ct)
            }
            BytesError::MissingBody => {
                write!(f, "No body provided")
            }
        }
    }
}

impl std::error::Error for BytesError {}

impl FromRequest for Bytes {
    type Error = BytesError;

    fn from_request(req: &HttpRequest) -> Result<Self, Self::Error> {
        // Get content type from headers
        let content_type = req
            .headers
            .iter()
            .find(|(name, _)| name.to_lowercase() == "content-type")
            .map(|(_, value)| value.to_lowercase())
            .unwrap_or_default();

        // Accept application/octet-stream or empty content-type for binary data
        if content_type.is_empty() || content_type.contains("application/octet-stream") {
            match &req.body {
                HttpBody::Binary(bytes) => Ok(Bytes(bytes.clone())),
                HttpBody::Text(text) => Ok(Bytes(text.as_bytes().to_vec())),
                HttpBody::Json(_) => Err(BytesError::UnsupportedContentType(
                    "Cannot extract bytes from JSON body".to_string(),
                )),
            }
        } else {
            Err(BytesError::UnsupportedContentType(content_type))
        }
    }
}
