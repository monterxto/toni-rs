use bytes::Bytes;

/// An HTTP request. Wraps [`http::Request<Bytes>`] so the full `http` crate
/// surface is available — including `into_parts()` for middleware that needs
/// to inspect or replace headers, body, or extensions.
#[derive(Clone)]
pub struct HttpRequest(pub http::Request<Bytes>);

impl HttpRequest {
    /// Returns a request builder. See [`http::request::Builder`] for the full API.
    pub fn builder() -> http::request::Builder {
        http::Request::builder()
    }

    pub fn into_inner(self) -> http::Request<Bytes> {
        self.0
    }

    pub fn into_parts(self) -> (http::request::Parts, Bytes) {
        self.0.into_parts()
    }

    pub fn from_parts(parts: http::request::Parts, body: Bytes) -> Self {
        Self(http::Request::from_parts(parts, body))
    }
}

impl From<http::Request<Bytes>> for HttpRequest {
    fn from(req: http::Request<Bytes>) -> Self {
        Self(req)
    }
}

impl std::ops::Deref for HttpRequest {
    type Target = http::Request<Bytes>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for HttpRequest {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl std::fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpRequest")
            .field("method", self.0.method())
            .field("uri", self.0.uri())
            .finish()
    }
}
