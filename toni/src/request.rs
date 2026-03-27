//! Built-in request-scoped provider for accessing the current HTTP request.
//!
//! Inject `Request` into a controller to access method, URI, headers,
//! path/query params, and typed extensions set by middleware — without taking
//! the raw `HttpRequest` as a handler argument every time.
//!
//! # Example
//!
//! ```rust,ignore
//! #[controller("/users", pub struct UserController {
//!     #[inject]
//!     request: Request,
//! })]
//! impl UserController {
//!     #[get("/me")]
//!     fn get_current_user(&self, _req: HttpRequest) -> ToniBody {
//!         let method = self.request.method();
//!         let uri = self.request.uri();
//!         ToniBody::text(format!("Method: {}, URI: {}", method, uri))
//!     }
//! }
//! ```

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use crate::FxHashMap;
use crate::async_trait;
use crate::extractors::FromRequest;
use crate::http_helpers::{HttpRequest, PathParams};
use crate::provider_scope::ProviderScope;
use crate::traits_helpers::{Provider, ProviderFactory};

/// Built-in request-scoped provider for accessing HTTP request metadata.
///
/// # Scope
///
/// `Request` is request-scoped and cannot be injected into singleton providers.
#[derive(Clone)]
pub struct Request {
    inner: Arc<HttpRequest>,
    path_params: HashMap<String, String>,
    query_params: HashMap<String, String>,
}

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
        Box::new(Request::from_request(http_req).expect("infallible"))
    }

    fn get_token_factory(&self) -> String {
        std::any::type_name::<Request>().to_string()
    }

    fn get_scope(&self) -> ProviderScope {
        ProviderScope::Request
    }
}

impl Request {
    pub fn method(&self) -> &str {
        self.inner.method().as_str()
    }

    pub fn uri(&self) -> &http::Uri {
        self.inner.uri()
    }

    /// Get a header value by name (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.inner
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
    }

    pub fn headers(&self) -> &http::HeaderMap {
        self.inner.headers()
    }

    pub fn query_params(&self) -> &HashMap<String, String> {
        &self.query_params
    }

    pub fn path_params(&self) -> &HashMap<String, String> {
        &self.path_params
    }

    pub fn extensions(&self) -> &http::Extensions {
        self.inner.extensions()
    }

    pub fn inner(&self) -> &HttpRequest {
        &self.inner
    }
}

impl FromRequest for Request {
    type Error = std::convert::Infallible;

    fn from_request(req: &HttpRequest) -> Result<Self, Self::Error> {
        let path_params = req
            .extensions()
            .get::<PathParams>()
            .map(|p| p.0.clone())
            .unwrap_or_default();

        let query_params = req
            .uri()
            .query()
            .and_then(|q| serde_urlencoded::from_str(q).ok())
            .unwrap_or_default();

        Ok(Self {
            inner: Arc::new(req.clone()),
            path_params,
            query_params,
        })
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
        let provider =
            Request::from_request(&HttpRequest(http::Request::new(bytes::Bytes::new())))
                .expect("infallible");
        Arc::new(Box::new(provider) as Box<dyn Provider>)
    }
}
