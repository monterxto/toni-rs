//! Raw bytes extractor for binary data

use super::FromRequest;
use crate::http_helpers::HttpRequest;

/// Extractor for raw binary request body.
///
/// Accepts `application/octet-stream` and requests with no content-type.
///
/// # Example
///
/// ```rust,ignore
/// #[post("/upload")]
/// fn upload(&self, Bytes(data): Bytes) -> String {
///     format!("Uploaded {} bytes", data.len())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Bytes(pub Vec<u8>);

impl Bytes {
    pub fn into_inner(self) -> Vec<u8> {
        self.0
    }

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

#[derive(Debug)]
pub enum BytesError {
    UnsupportedContentType(String),
    MissingBody,
}

impl std::fmt::Display for BytesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BytesError::UnsupportedContentType(ct) => {
                write!(f, "Unsupported content type for binary data: {}", ct)
            }
            BytesError::MissingBody => write!(f, "No body provided"),
        }
    }
}

impl std::error::Error for BytesError {}

impl FromRequest for Bytes {
    type Error = BytesError;

    fn from_request(req: &HttpRequest) -> Result<Self, Self::Error> {
        let content_type = req
            .headers
            .iter()
            .find(|(name, _)| name.to_lowercase() == "content-type")
            .map(|(_, value)| value.to_lowercase())
            .unwrap_or_default();

        if content_type.is_empty() || content_type.contains("application/octet-stream") {
            Ok(Bytes(req.body.to_vec()))
        } else {
            Err(BytesError::UnsupportedContentType(content_type))
        }
    }
}
