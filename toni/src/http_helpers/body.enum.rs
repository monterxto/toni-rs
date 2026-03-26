use std::error::Error;
use std::fmt;

use bytes::Bytes;
use futures::Stream;
use http_body_util::BodyExt;
use serde_json::Value;

/// Type-erased response body. Adapters consume this via [`Body::into_box_body`].
///
/// Requires `Send + Sync` — streams passed to [`Body::stream`] must satisfy
/// this bound. Use `futures::stream::iter` or `futures::stream::unfold` for
/// most cases; wrap non-`Sync` state in `Arc<Mutex<...>>` if needed.
pub type BoxBody = http_body_util::combinators::BoxBody<Bytes, Box<dyn Error + Send + Sync>>;

enum BodyInner {
    Buffered(Bytes),
    Streaming(BoxBody),
}

impl fmt::Debug for BodyInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BodyInner::Buffered(b) => write!(f, "Buffered({} bytes)", b.len()),
            BodyInner::Streaming(_) => write!(f, "Streaming(...)"),
        }
    }
}

/// An HTTP response body.
///
/// Use the static constructors for buffered content, or [`Body::stream`] for
/// large or generated responses that should not be fully loaded into memory.
///
/// # Example
///
/// ```rust,ignore
/// // Buffered
/// Body::text("hello")
/// Body::json(json!({"ok": true}))
///
/// // Streaming
/// use futures::stream;
/// use bytes::Bytes;
///
/// Body::stream(stream::iter(vec![
///     Ok::<Bytes, std::io::Error>(Bytes::from("chunk 1 ")),
///     Ok(Bytes::from("chunk 2")),
/// ]))
/// .with_content_type("text/plain; charset=utf-8")
/// ```
#[derive(Debug)]
pub struct Body {
    inner: BodyInner,
    content_type: Option<String>,
}

impl Body {
    /// Plain text body. Sets `Content-Type: text/plain; charset=utf-8`.
    pub fn text(s: impl Into<String>) -> Self {
        Self {
            inner: BodyInner::Buffered(Bytes::from(s.into().into_bytes())),
            content_type: Some("text/plain; charset=utf-8".to_string()),
        }
    }

    /// JSON body from a [`serde_json::Value`]. Sets `Content-Type: application/json`.
    pub fn json(value: Value) -> Self {
        Self {
            inner: BodyInner::Buffered(Bytes::from(
                serde_json::to_vec(&value).unwrap_or_default(),
            )),
            content_type: Some("application/json".to_string()),
        }
    }

    /// Raw binary body. Sets `Content-Type: application/octet-stream`.
    pub fn binary(data: impl Into<Vec<u8>>) -> Self {
        Self {
            inner: BodyInner::Buffered(Bytes::from(data.into())),
            content_type: Some("application/octet-stream".to_string()),
        }
    }

    /// Empty body with no content-type.
    pub fn empty() -> Self {
        Self {
            inner: BodyInner::Buffered(Bytes::new()),
            content_type: None,
        }
    }

    /// Streaming body. Chunks produced by `stream` are forwarded to the adapter
    /// without buffering.
    ///
    /// Content-type is not set automatically — call `.with_content_type()` or
    /// include a `Content-Type` header on the response.
    pub fn stream<S, E>(stream: S) -> Self
    where
        S: Stream<Item = Result<Bytes, E>> + Send + Sync + 'static,
        E: Into<Box<dyn Error + Send + Sync>> + 'static,
    {
        use futures::StreamExt;
        use http_body_util::StreamBody;

        let frames = stream.map(|r| r.map(http_body::Frame::data).map_err(Into::into));
        Self {
            inner: BodyInner::Streaming(BodyExt::boxed(StreamBody::new(frames))),
            content_type: None,
        }
    }

    /// Override or set the content-type.
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// The content-type this body carries, if any.
    pub fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    /// The raw bytes of a buffered body. Returns `None` for streaming bodies.
    pub fn try_bytes(&self) -> Option<&Bytes> {
        match &self.inner {
            BodyInner::Buffered(bytes) => Some(bytes),
            BodyInner::Streaming(_) => None,
        }
    }

    /// Whether this is a streaming body.
    pub fn is_streaming(&self) -> bool {
        matches!(self.inner, BodyInner::Streaming(_))
    }

    /// Consume this body and return a [`BoxBody`] for the adapter to write.
    pub fn into_box_body(self) -> BoxBody {
        match self.inner {
            BodyInner::Buffered(bytes) => http_body_util::Full::new(bytes)
                .map_err(|never: std::convert::Infallible| match never {})
                .boxed(),
            BodyInner::Streaming(box_body) => box_body,
        }
    }
}

impl From<Bytes> for Body {
    fn from(bytes: Bytes) -> Self {
        Self {
            inner: BodyInner::Buffered(bytes),
            content_type: None,
        }
    }
}
