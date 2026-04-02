use super::request_body::RequestBody;

/// Request metadata — method, URI, version, headers, and extensions — without a body.
///
/// This is a type alias for [`http::request::Parts`] from the `http` crate.
/// It is `Clone`: cloning preserves all fields including extensions (http 1.x behaviour).
pub type RequestPart = http::request::Parts;

/// A full HTTP request — metadata plus a (potentially streaming) body.
///
/// A thin newtype over [`http::Request<RequestBody>`]. Not `Clone` because the body
/// may be a single-use stream: whoever calls [`RequestBody::collect`] first drains it,
/// and subsequent readers see nothing. Use `into_parts()` to decompose, collect the bytes,
/// then reconstruct with `RequestBody::Buffered` if the body needs to be read more than once.
pub struct HttpRequest(pub http::Request<RequestBody>);

impl HttpRequest {
    /// Returns a request builder. See [`http::request::Builder`] for the full API.
    pub fn builder() -> http::request::Builder {
        http::Request::builder()
    }

    pub fn into_parts(self) -> (RequestPart, RequestBody) {
        self.0.into_parts()
    }

    pub fn from_parts(parts: RequestPart, body: RequestBody) -> Self {
        Self(http::Request::from_parts(parts, body))
    }

    pub fn into_inner(self) -> http::Request<RequestBody> {
        self.0
    }
}

impl From<http::Request<RequestBody>> for HttpRequest {
    fn from(req: http::Request<RequestBody>) -> Self {
        Self(req)
    }
}

impl From<http::Request<bytes::Bytes>> for HttpRequest {
    fn from(req: http::Request<bytes::Bytes>) -> Self {
        let (parts, body) = req.into_parts();
        Self(http::Request::from_parts(
            parts,
            RequestBody::Buffered(body),
        ))
    }
}

impl std::ops::Deref for HttpRequest {
    type Target = http::Request<RequestBody>;

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
            .field("method", self.method())
            .field("uri", self.uri())
            .finish()
    }
}
