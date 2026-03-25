use serde_json::Value;

use super::Body;

#[derive(Debug)]
pub struct HttpResponseDefault {
    pub body: Option<Body>,
    pub status: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub body: Option<Body>,
    pub status: u16,
    pub headers: Vec<(String, String)>,
}

impl HttpResponse {
    pub fn new() -> Self {
        Self {
            body: None,
            status: 200,
            headers: vec![],
        }
    }

    pub fn builder() -> HttpResponseBuilder {
        HttpResponseBuilder::new()
    }

    pub fn ok() -> HttpResponseBuilder {
        HttpResponseBuilder::new().status(200)
    }

    pub fn created() -> HttpResponseBuilder {
        HttpResponseBuilder::new().status(201)
    }

    pub fn no_content() -> HttpResponseBuilder {
        HttpResponseBuilder::new().status(204)
    }

    pub fn bad_request() -> HttpResponseBuilder {
        HttpResponseBuilder::new().status(400)
    }

    pub fn unauthorized() -> HttpResponseBuilder {
        HttpResponseBuilder::new().status(401)
    }

    pub fn forbidden() -> HttpResponseBuilder {
        HttpResponseBuilder::new().status(403)
    }

    pub fn not_found() -> HttpResponseBuilder {
        HttpResponseBuilder::new().status(404)
    }

    pub fn internal_server_error() -> HttpResponseBuilder {
        HttpResponseBuilder::new().status(500)
    }
}

impl Default for HttpResponse {
    fn default() -> Self {
        Self::new()
    }
}

/// Fluent builder for `HttpResponse`.
///
/// # Example
///
/// ```rust
/// use toni::HttpResponse;
/// use serde_json::json;
///
/// let response = HttpResponse::ok()
///     .header("X-Request-Id", "abc")
///     .json(json!({"message": "Success"}))
///     .build();
/// ```
#[derive(Debug)]
pub struct HttpResponseBuilder {
    status: u16,
    headers: Vec<(String, String)>,
    body: Option<Body>,
}

impl HttpResponseBuilder {
    pub fn new() -> Self {
        Self {
            status: 200,
            headers: vec![],
            body: None,
        }
    }

    pub fn status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn headers(mut self, headers: Vec<(String, String)>) -> Self {
        self.headers.extend(headers);
        self
    }

    pub fn body(mut self, body: Body) -> Self {
        self.body = Some(body);
        self
    }

    /// JSON body. The `Body` already carries `Content-Type: application/json`;
    /// no need to set it separately unless overriding.
    pub fn json(mut self, value: Value) -> Self {
        self.body = Some(Body::json(value));
        self
    }

    /// Plain text body.
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.body = Some(Body::text(text));
        self
    }

    /// Binary body.
    pub fn binary(mut self, data: Vec<u8>) -> Self {
        self.body = Some(Body::binary(data));
        self
    }

    pub fn build(self) -> HttpResponse {
        HttpResponse {
            status: self.status,
            headers: self.headers,
            body: self.body,
        }
    }
}

impl Default for HttpResponseBuilder {
    fn default() -> Self {
        Self::new()
    }
}
