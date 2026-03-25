//! Built-in request-scoped provider for accessing HTTP request data.
//!
//! The `Request` provider is automatically available in all Toni applications
//! and provides convenient access to HTTP request data without coupling
//! business logic to HTTP types.
//!
//! # Examples
//!
//! ## Basic Usage
//!
//! ```rust
//! use toni::{controller, get, Request, Body as ToniBody, HttpRequest};
//!
//! #[controller("/users", pub struct UserController {
//!     #[inject]
//!     request: Request,  // Built-in, automatically available!
//! })]
//! impl UserController {
//!     #[get("/me")]
//!     fn get_current_user(&self, _req: HttpRequest) -> ToniBody {
//!         // Access request data
//!         let method = self.request.method();
//!         let uri = self.request.uri();
//!
//!         ToniBody::text(format!("Method: {}, URI: {}", method, uri))
//!     }
//! }
//! ```
//!
//! ## Accessing Extensions
//!
//! ```rust
//! use toni::{controller, get, Request, Body as ToniBody, HttpRequest};
//!
//! #[derive(Clone)]
//! struct UserId(String);
//!
//! #[controller("/profile", pub struct ProfileController {
//!     #[inject]
//!     request: Request,
//! })]
//! impl ProfileController {
//!     #[get("/")]
//!     fn get_profile(&self, _req: HttpRequest) -> ToniBody {
//!         // Access typed data from extensions (set by middleware)
//!         if let Some(user_id) = self.request.extensions().get::<UserId>() {
//!             ToniBody::text(format!("Profile for user: {}", user_id.0))
//!         } else {
//!             ToniBody::text("Anonymous user".to_string())
//!         }
//!     }
//! }
//! ```
//!
//! ## Accessing Headers
//!
//! ```rust
//! use toni::{controller, get, Request, Body as ToniBody, HttpRequest};
//!
//! #[controller("/api", pub struct ApiController {
//!     #[inject]
//!     request: Request,
//! })]
//! impl ApiController {
//!     #[get("/data")]
//!     fn get_data(&self, _req: HttpRequest) -> ToniBody {
//!         // Get header value (case-insensitive)
//!         let auth = self.request.header("authorization");
//!         let content_type = self.request.header("Content-Type");
//!
//!         if auth.is_some() {
//!             ToniBody::text("Authenticated".to_string())
//!         } else {
//!             ToniBody::text("Not authenticated".to_string())
//!         }
//!     }
//! }
//! ```
//!
//! # Scope
//!
//! `Request` is a request-scoped provider. This means:
//! - A fresh instance is created for each HTTP request
//! - It can only be injected into request-scoped providers or controllers
//! - Attempting to inject it into a singleton provider will cause a panic at startup
//!
//! # Custom Contexts
//!
//! While `Request` provides convenient access to request data, you can still
//! create custom request contexts for domain-specific needs:
//!
//! ```rust
//! use toni::{injectable, HttpRequest};
//!
//! #[injectable(scope = "request", init = "from_request")]
//! pub struct AuthContext {
//!     user_id: String,
//!     roles: Vec<String>,
//! }
//!
//! impl AuthContext {
//!     pub fn from_request(req: &HttpRequest) -> Self {
//!         // Custom extraction logic
//!         let user_id = extract_user_from_jwt(req);
//!         let roles = load_user_roles(&user_id);
//!
//!         Self { user_id, roles }
//!     }
//! }
//! # fn extract_user_from_jwt(_req: &HttpRequest) -> String { "user123".to_string() }
//! # fn load_user_roles(_id: &str) -> Vec<String> { vec!["admin".to_string()] }
//! ```

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use crate::FxHashMap;
use crate::async_trait;
use crate::extractors::FromRequest;
use crate::http_helpers::{Extensions, HttpRequest};
use crate::provider_scope::ProviderScope;
use crate::traits_helpers::{Provider, ProviderFactory};

/// Built-in request-scoped provider for accessing HTTP request data.
///
/// This provider wraps the `HttpRequest` and provides convenient accessor methods.
/// It is automatically available in all Toni applications.
///
/// # Scope
///
/// `Request` is request-scoped and cannot be injected into singleton providers.
/// Attempting to do so will result in a panic at application startup.
///
/// # Efficiency
///
/// `Request` uses `Arc<HttpRequest>` internally for efficient sharing.
/// Cloning a `Request` instance is cheap as it only increments a reference count.
#[derive(Clone)]
pub struct Request {
    inner: Arc<HttpRequest>,
}

// Manual Provider implementation (can't use macro inside toni crate)
#[async_trait]
impl Provider for Request {
    fn get_token(&self) -> String {
        std::any::type_name::<Request>().to_string()
    }

    async fn execute(
        &self,
        _params: Vec<Box<dyn Any + Send>>,
        req: Option<&HttpRequest>,
    ) -> Box<dyn Any + Send> {
        let http_req = req.expect("Request provider requires HttpRequest");
        Box::new(Request::from_request(http_req))
    }

    fn get_token_factory(&self) -> String {
        std::any::type_name::<Request>().to_string()
    }

    fn get_scope(&self) -> ProviderScope {
        ProviderScope::Request
    }
}

impl Request {
    /// Creates a `Request` from an `HttpRequest`.
    ///
    /// This method is called automatically by the framework during request handling.
    /// You typically don't need to call this manually except in tests.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use toni::{Request, HttpRequest, http_helpers::Extensions};
    /// use std::collections::HashMap;
    ///
    /// let http_req = HttpRequest {
    ///     body: bytes::Bytes::new(),
    ///     headers: vec![("content-type".to_string(), "application/json".to_string())],
    ///     method: "GET".to_string(),
    ///     uri: "/users/123".to_string(),
    ///     query_params: HashMap::new(),
    ///     path_params: HashMap::new(),
    ///     extensions: Extensions::new(),
    /// };
    ///
    /// let request = Request::from_request(&http_req);
    /// assert_eq!(request.method(), "GET");
    /// ```
    pub fn from_request(req: &HttpRequest) -> Self {
        Self {
            inner: Arc::new(req.clone()),
        }
    }

    /// Get the HTTP method (GET, POST, PUT, DELETE, etc.).
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use toni::{Request, HttpRequest, http_helpers::Extensions};
    /// # use std::collections::HashMap;
    /// # let http_req = HttpRequest {
    /// #     body: bytes::Bytes::new(),
    /// #     headers: vec![],
    /// #     method: "POST".to_string(),
    /// #     uri: "/".to_string(),
    /// #     query_params: HashMap::new(),
    /// #     path_params: HashMap::new(),
    /// #     extensions: Extensions::new(),
    /// # };
    /// let request = Request::from_request(&http_req);
    /// assert_eq!(request.method(), "POST");
    /// ```
    pub fn method(&self) -> &str {
        &self.inner.method
    }

    /// Get the request URI.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use toni::{Request, HttpRequest, http_helpers::Extensions};
    /// # use std::collections::HashMap;
    /// # let http_req = HttpRequest {
    /// #     body: bytes::Bytes::new(),
    /// #     headers: vec![],
    /// #     method: "GET".to_string(),
    /// #     uri: "/users/123".to_string(),
    /// #     query_params: HashMap::new(),
    /// #     path_params: HashMap::new(),
    /// #     extensions: Extensions::new(),
    /// # };
    /// let request = Request::from_request(&http_req);
    /// assert_eq!(request.uri(), "/users/123");
    /// ```
    pub fn uri(&self) -> &str {
        &self.inner.uri
    }

    /// Get a header value by name (case-insensitive).
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use toni::{Request, HttpRequest, http_helpers::Extensions};
    /// # use std::collections::HashMap;
    /// let http_req = HttpRequest {
    ///     body: bytes::Bytes::new(),
    ///     headers: vec![
    ///         ("Content-Type".to_string(), "application/json".to_string()),
    ///         ("Authorization".to_string(), "Bearer token123".to_string()),
    ///     ],
    ///     method: "GET".to_string(),
    ///     uri: "/".to_string(),
    ///     query_params: HashMap::new(),
    ///     path_params: HashMap::new(),
    ///     extensions: Extensions::new(),
    /// };
    ///
    /// let request = Request::from_request(&http_req);
    ///
    /// assert_eq!(request.header("content-type"), Some("application/json"));
    /// assert_eq!(request.header("AUTHORIZATION"), Some("Bearer token123"));
    /// assert_eq!(request.header("X-Custom"), None);
    /// ```
    pub fn header(&self, name: &str) -> Option<&str> {
        let name_lower = name.to_lowercase();
        self.inner
            .headers
            .iter()
            .find(|(k, _)| k.to_lowercase() == name_lower)
            .map(|(_, v)| v.as_str())
    }

    /// Get all headers as a slice.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use toni::{Request, HttpRequest, http_helpers::Extensions};
    /// # use std::collections::HashMap;
    /// # let http_req = HttpRequest {
    /// #     body: bytes::Bytes::new(),
    /// #     headers: vec![("content-type".to_string(), "text/plain".to_string())],
    /// #     method: "GET".to_string(),
    /// #     uri: "/".to_string(),
    /// #     query_params: HashMap::new(),
    /// #     path_params: HashMap::new(),
    /// #     extensions: Extensions::new(),
    /// # };
    /// let request = Request::from_request(&http_req);
    /// let headers = request.headers();
    /// assert_eq!(headers.len(), 1);
    /// ```
    pub fn headers(&self) -> &[(String, String)] {
        &self.inner.headers
    }

    /// Get query parameters.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use toni::{Request, HttpRequest, http_helpers::Extensions};
    /// # use std::collections::HashMap;
    /// let mut query_params = HashMap::new();
    /// query_params.insert("page".to_string(), "1".to_string());
    /// query_params.insert("limit".to_string(), "10".to_string());
    ///
    /// let http_req = HttpRequest {
    ///     body: bytes::Bytes::new(),
    ///     headers: vec![],
    ///     method: "GET".to_string(),
    ///     uri: "/users?page=1&limit=10".to_string(),
    ///     query_params,
    ///     path_params: HashMap::new(),
    ///     extensions: Extensions::new(),
    /// };
    ///
    /// let request = Request::from_request(&http_req);
    /// assert_eq!(request.query_params().get("page"), Some(&"1".to_string()));
    /// ```
    pub fn query_params(&self) -> &HashMap<String, String> {
        &self.inner.query_params
    }

    /// Get path parameters.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use toni::{Request, HttpRequest, http_helpers::Extensions};
    /// # use std::collections::HashMap;
    /// let mut path_params = HashMap::new();
    /// path_params.insert("id".to_string(), "123".to_string());
    ///
    /// let http_req = HttpRequest {
    ///     body: bytes::Bytes::new(),
    ///     headers: vec![],
    ///     method: "GET".to_string(),
    ///     uri: "/users/123".to_string(),
    ///     query_params: HashMap::new(),
    ///     path_params,
    ///     extensions: Extensions::new(),
    /// };
    ///
    /// let request = Request::from_request(&http_req);
    /// assert_eq!(request.path_params().get("id"), Some(&"123".to_string()));
    /// ```
    pub fn path_params(&self) -> &HashMap<String, String> {
        &self.inner.path_params
    }

    /// Get the raw request body bytes.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use toni::{Request, HttpRequest, http_helpers::Extensions};
    /// # use bytes::Bytes;
    /// # use std::collections::HashMap;
    /// let http_req = HttpRequest {
    ///     body: Bytes::from("Hello, World!"),
    ///     headers: vec![],
    ///     method: "POST".to_string(),
    ///     uri: "/".to_string(),
    ///     query_params: HashMap::new(),
    ///     path_params: HashMap::new(),
    ///     extensions: Extensions::new(),
    /// };
    ///
    /// let request = Request::from_request(&http_req);
    /// assert_eq!(request.body().as_ref(), b"Hello, World!");
    /// ```
    pub fn body(&self) -> &bytes::Bytes {
        &self.inner.body
    }

    /// Access request extensions.
    ///
    /// Extensions allow middleware to pass typed data to controllers
    /// and request-scoped providers.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use toni::{Request, HttpRequest, http_helpers::Extensions};
    /// # use std::collections::HashMap;
    /// #[derive(Clone)]
    /// struct UserId(String);
    ///
    /// let mut http_req = HttpRequest {
    ///     body: bytes::Bytes::new(),
    ///     headers: vec![],
    ///     method: "GET".to_string(),
    ///     uri: "/".to_string(),
    ///     query_params: HashMap::new(),
    ///     path_params: HashMap::new(),
    ///     extensions: Extensions::new(),
    /// };
    ///
    /// http_req.extensions.insert(UserId("alice".to_string()));
    ///
    /// let request = Request::from_request(&http_req);
    /// assert_eq!(request.extensions().get::<UserId>().unwrap().0, "alice");
    /// ```
    pub fn extensions(&self) -> &Extensions {
        &self.inner.extensions
    }

    /// Get the inner `HttpRequest` (for advanced use cases).
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use toni::{Request, HttpRequest, http_helpers::Extensions};
    /// # use std::collections::HashMap;
    /// # let http_req = HttpRequest {
    /// #     body: bytes::Bytes::new(),
    /// #     headers: vec![],
    /// #     method: "GET".to_string(),
    /// #     uri: "/".to_string(),
    /// #     query_params: HashMap::new(),
    /// #     path_params: HashMap::new(),
    /// #     extensions: Extensions::new(),
    /// # };
    /// let request = Request::from_request(&http_req);
    /// let inner = request.inner();
    /// assert_eq!(inner.method, "GET");
    /// ```
    pub fn inner(&self) -> &HttpRequest {
        &self.inner
    }
}

/// Implement FromRequest trait to allow Request to be used as an extractor
impl FromRequest for Request {
    type Error = std::convert::Infallible;

    fn from_request(req: &HttpRequest) -> Result<Self, Self::Error> {
        Ok(Request::from_request(req))
    }
}

pub struct RequestFactory;

#[async_trait]
impl ProviderFactory for RequestFactory {
    fn get_token(&self) -> String {
        std::any::type_name::<Request>().to_string()
    }

    async fn build(
        &self,
        _deps: FxHashMap<String, Arc<Box<dyn Provider>>>,
    ) -> Arc<Box<dyn Provider>> {
        // Placeholder — the real instance is created per-request in execute()
        let provider = Request {
            inner: Arc::new(HttpRequest {
                body: bytes::Bytes::new(),
                headers: vec![],
                method: String::new(),
                uri: String::new(),
                query_params: HashMap::new(),
                path_params: HashMap::new(),
                extensions: Extensions::new(),
            }),
        };
        Arc::new(Box::new(provider) as Box<dyn Provider>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_request() -> HttpRequest {
        let mut query_params = HashMap::new();
        query_params.insert("page".to_string(), "1".to_string());

        let mut path_params = HashMap::new();
        path_params.insert("id".to_string(), "123".to_string());

        HttpRequest {
            body: bytes::Bytes::from("test body"),
            headers: vec![
                ("content-type".to_string(), "application/json".to_string()),
                ("Authorization".to_string(), "Bearer token123".to_string()),
            ],
            method: "POST".to_string(),
            uri: "/users/123?page=1".to_string(),
            query_params,
            path_params,
            extensions: Extensions::new(),
        }
    }

    #[test]
    fn test_from_request() {
        let http_req = create_test_request();
        let request = Request::from_request(&http_req);

        assert_eq!(request.method(), "POST");
        assert_eq!(request.uri(), "/users/123?page=1");
    }

    #[test]
    fn test_method() {
        let http_req = create_test_request();
        let request = Request::from_request(&http_req);

        assert_eq!(request.method(), "POST");
    }

    #[test]
    fn test_uri() {
        let http_req = create_test_request();
        let request = Request::from_request(&http_req);

        assert_eq!(request.uri(), "/users/123?page=1");
    }

    #[test]
    fn test_header() {
        let http_req = create_test_request();
        let request = Request::from_request(&http_req);

        // Case insensitive
        assert_eq!(request.header("content-type"), Some("application/json"));
        assert_eq!(request.header("CONTENT-TYPE"), Some("application/json"));
        assert_eq!(request.header("Content-Type"), Some("application/json"));

        assert_eq!(request.header("authorization"), Some("Bearer token123"));
        assert_eq!(request.header("X-Custom"), None);
    }

    #[test]
    fn test_headers() {
        let http_req = create_test_request();
        let request = Request::from_request(&http_req);

        let headers = request.headers();
        assert_eq!(headers.len(), 2);
    }

    #[test]
    fn test_query_params() {
        let http_req = create_test_request();
        let request = Request::from_request(&http_req);

        assert_eq!(request.query_params().get("page"), Some(&"1".to_string()));
    }

    #[test]
    fn test_path_params() {
        let http_req = create_test_request();
        let request = Request::from_request(&http_req);

        assert_eq!(request.path_params().get("id"), Some(&"123".to_string()));
    }

    #[test]
    fn test_body() {
        let http_req = create_test_request();
        let request = Request::from_request(&http_req);

        assert_eq!(request.body().as_ref(), b"test body");
    }

    #[test]
    fn test_inner() {
        let http_req = create_test_request();
        let request = Request::from_request(&http_req);

        let inner = request.inner();
        assert_eq!(inner.method, "POST");
        assert_eq!(inner.uri, "/users/123?page=1");
    }

    #[test]
    fn test_arc_sharing() {
        let http_req = create_test_request();
        let request1 = Request::from_request(&http_req);
        let request2 = request1.clone();

        // Both should share the same Arc
        assert_eq!(request1.method(), request2.method());
        assert_eq!(request1.uri(), request2.uri());
    }
}
