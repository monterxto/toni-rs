use bytes::Bytes;
use http_body_util::BodyExt;

/// Type-erased streaming request body. `Send + !Sync` — single-use by design.
/// `UnsyncBoxBody` only requires `Body + Send`, so adapters that produce
/// `!Sync` streams (e.g. axum's `Body`) can wrap them without buffering.
pub type RequestBoxBody =
    http_body_util::combinators::UnsyncBoxBody<Bytes, Box<dyn std::error::Error + Send + Sync>>;

/// An HTTP request body — either already buffered or a stream yet to be read.
///
/// Adapters produce `Streaming` by default; extractors that need raw bytes
/// call [`RequestBody::collect`] to buffer on demand. [`BodyStream`] takes
/// the stream directly without buffering.
///
/// [`BodyStream`]: crate::extractors::BodyStream
pub enum RequestBody {
    Buffered(Bytes),
    Streaming(RequestBoxBody),
}

impl RequestBody {
    /// Buffer the body into [`Bytes`], collecting the stream if needed.
    pub async fn collect(self) -> Result<Bytes, Box<dyn std::error::Error + Send + Sync>> {
        match self {
            RequestBody::Buffered(b) => Ok(b),
            RequestBody::Streaming(stream) => {
                let collected = stream.collect().await?;
                Ok(collected.to_bytes())
            }
        }
    }

    /// An empty buffered body. Used when constructing placeholder requests
    /// (e.g. in `RequestFactory::build`) and when no body is expected.
    pub fn empty() -> Self {
        RequestBody::Buffered(Bytes::new())
    }

    pub fn is_buffered(&self) -> bool {
        matches!(self, RequestBody::Buffered(_))
    }

    /// Returns the buffered bytes without consuming, if already buffered.
    pub fn as_bytes(&self) -> Option<&Bytes> {
        match self {
            RequestBody::Buffered(b) => Some(b),
            RequestBody::Streaming(_) => None,
        }
    }
}

impl std::fmt::Debug for RequestBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestBody::Buffered(b) => write!(f, "Buffered({} bytes)", b.len()),
            RequestBody::Streaming(_) => write!(f, "Streaming(...)"),
        }
    }
}

impl From<Bytes> for RequestBody {
    fn from(b: Bytes) -> Self {
        RequestBody::Buffered(b)
    }
}

impl From<Vec<u8>> for RequestBody {
    fn from(v: Vec<u8>) -> Self {
        RequestBody::Buffered(Bytes::from(v))
    }
}

impl From<RequestBoxBody> for RequestBody {
    fn from(b: RequestBoxBody) -> Self {
        RequestBody::Streaming(b)
    }
}
