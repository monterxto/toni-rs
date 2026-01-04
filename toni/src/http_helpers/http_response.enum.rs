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

    /// Create a response builder for fluent API construction
    pub fn builder() -> HttpResponseBuilder {
        HttpResponseBuilder::new()
    }

    /// Convenience method for 200 OK response
    pub fn ok() -> HttpResponseBuilder {
        HttpResponseBuilder::new().status(200)
    }

    /// Convenience method for 201 Created response
    pub fn created() -> HttpResponseBuilder {
        HttpResponseBuilder::new().status(201)
    }

    /// Convenience method for 204 No Content response
    pub fn no_content() -> HttpResponseBuilder {
        HttpResponseBuilder::new().status(204)
    }

    /// Convenience method for 400 Bad Request response
    pub fn bad_request() -> HttpResponseBuilder {
        HttpResponseBuilder::new().status(400)
    }

    /// Convenience method for 401 Unauthorized response
    pub fn unauthorized() -> HttpResponseBuilder {
        HttpResponseBuilder::new().status(401)
    }

    /// Convenience method for 403 Forbidden response
    pub fn forbidden() -> HttpResponseBuilder {
        HttpResponseBuilder::new().status(403)
    }

    /// Convenience method for 404 Not Found response
    pub fn not_found() -> HttpResponseBuilder {
        HttpResponseBuilder::new().status(404)
    }

    /// Convenience method for 500 Internal Server Error response
    pub fn internal_server_error() -> HttpResponseBuilder {
        HttpResponseBuilder::new().status(500)
    }
}

impl Default for HttpResponse {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for constructing HTTP responses with a fluent API
///
/// # Examples
///
/// ```rust
/// use toni::{HttpResponse, Body};
/// use serde_json::json;
///
/// // Basic usage
/// let response = HttpResponse::ok()
///     .body(Body::Text("Success".to_string()))
///     .build();
///
/// // With headers
/// let response = HttpResponse::ok()
///     .header("X-Custom-Header", "value")
///     .json(json!({"message": "Success"}))
///     .build();
///
/// // Custom status
/// let response = HttpResponse::builder()
///     .status(202)
///     .text("Accepted")
///     .build();
/// ```
#[derive(Debug)]
pub struct HttpResponseBuilder {
    status: u16,
    headers: Vec<(String, String)>,
    body: Option<Body>,
}

impl HttpResponseBuilder {
    /// Create a new response builder with default values (200 status, no headers, no body)
    pub fn new() -> Self {
        Self {
            status: 200,
            headers: vec![],
            body: None,
        }
    }

    /// Set the HTTP status code
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use toni::HttpResponse;
    /// let response = HttpResponse::builder()
    ///     .status(201)
    ///     .build();
    /// assert_eq!(response.status, 201);
    /// ```
    pub fn status(mut self, status: u16) -> Self {
        self.status = status;
        self
    }

    /// Add a header to the response
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use toni::HttpResponse;
    /// let response = HttpResponse::ok()
    ///     .header("X-Custom-Header", "value")
    ///     .header("Cache-Control", "no-cache")
    ///     .build();
    /// assert_eq!(response.headers.len(), 2);
    /// ```
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Set multiple headers at once
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use toni::HttpResponse;
    /// let response = HttpResponse::ok()
    ///     .headers(vec![
    ///         ("X-Header-1".to_string(), "value1".to_string()),
    ///         ("X-Header-2".to_string(), "value2".to_string()),
    ///     ])
    ///     .build();
    /// ```
    pub fn headers(mut self, headers: Vec<(String, String)>) -> Self {
        self.headers.extend(headers);
        self
    }

    /// Set the response body
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use toni::{HttpResponse, Body};
    /// let response = HttpResponse::ok()
    ///     .body(Body::Text("Hello".to_string()))
    ///     .build();
    /// ```
    pub fn body(mut self, body: Body) -> Self {
        self.body = Some(body);
        self
    }

    /// Set a JSON body (convenience method)
    ///
    /// Automatically sets the Content-Type header to application/json
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use toni::HttpResponse;
    /// # use serde_json::json;
    /// let response = HttpResponse::ok()
    ///     .json(json!({"message": "Success", "code": 200}))
    ///     .build();
    /// ```
    pub fn json(mut self, value: serde_json::Value) -> Self {
        // Add Content-Type header if not already set
        if !self
            .headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        {
            self.headers
                .push(("Content-Type".to_string(), "application/json".to_string()));
        }
        self.body = Some(Body::Json(value));
        self
    }

    /// Set a text body (convenience method)
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use toni::HttpResponse;
    /// let response = HttpResponse::ok()
    ///     .text("Hello, World!")
    ///     .build();
    /// ```
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.body = Some(Body::Text(text.into()));
        self
    }

    /// Set a binary body (convenience method)
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use toni::HttpResponse;
    /// let data = vec![0u8, 1, 2, 3];
    /// let response = HttpResponse::ok()
    ///     .binary(data)
    ///     .build();
    /// ```
    pub fn binary(mut self, data: Vec<u8>) -> Self {
        self.body = Some(Body::Binary(data));
        self
    }

    /// Build the final HttpResponse
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use toni::HttpResponse;
    /// let response = HttpResponse::ok()
    ///     .text("Success")
    ///     .build();
    /// assert_eq!(response.status, 200);
    /// ```
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
