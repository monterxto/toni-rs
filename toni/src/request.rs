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
use crate::extractors::FromRequestParts;
use crate::http_helpers::{PathParams, RequestPart};
use crate::provider_scope::ProviderScope;
use crate::traits_helpers::{Provider, ProviderFactory};

/// Built-in request-scoped provider for accessing HTTP request metadata.
///
/// # Scope
///
/// `Request` is request-scoped and cannot be injected into singleton providers.
#[derive(Clone)]
pub struct Request {
    inner: Arc<RequestPart>,
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
        req: Option<&RequestPart>,
    ) -> Box<dyn Any + Send> {
        let parts = req.expect("Request provider requires a request-scoped context");
        Box::new(Request::from_request_parts(parts).expect("infallible"))
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
        self.inner.method.as_str()
    }

    pub fn uri(&self) -> &http::Uri {
        &self.inner.uri
    }

    /// Get a header value by name (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.inner
            .headers
            .get(name)
            .and_then(|v| v.to_str().ok())
    }

    pub fn headers(&self) -> &http::HeaderMap {
        &self.inner.headers
    }

    pub fn query_params(&self) -> &HashMap<String, String> {
        &self.query_params
    }

    pub fn path_params(&self) -> &HashMap<String, String> {
        &self.path_params
    }

    pub fn extensions(&self) -> &http::Extensions {
        &self.inner.extensions
    }

    pub fn inner(&self) -> &RequestPart {
        &self.inner
    }
}

impl FromRequestParts for Request {
    type Error = std::convert::Infallible;

    fn from_request_parts(parts: &RequestPart) -> Result<Self, Self::Error> {
        let path_params = parts
            .extensions
            .get::<PathParams>()
            .map(|p| p.0.clone())
            .unwrap_or_default();

        let query_params = parts
            .uri
            .query()
            .and_then(|q| serde_urlencoded::from_str(q).ok())
            .unwrap_or_default();

        Ok(Self {
            inner: Arc::new(parts.clone()),
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
        let (parts, ()) = http::Request::builder().body(()).unwrap().into_parts();
        let provider = Request::from_request_parts(&parts).expect("infallible");
        Arc::new(Box::new(provider) as Box<dyn Provider>)
    }
}
