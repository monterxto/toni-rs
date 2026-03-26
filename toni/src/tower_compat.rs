use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use http_body::Body as HttpBody;
use http_body_util::BodyExt;
use tower::{Layer, Service, ServiceExt};

use crate::async_trait;
use crate::http_helpers::{Body, Extensions, HttpRequest, HttpResponse};
use crate::traits_helpers::middleware::{Middleware, MiddlewareResult, Next};

// Carries path params and toni extensions through the http::Request extension
// map so they survive the Tower round-trip.
#[derive(Clone)]
struct ToniPathParams(HashMap<String, String>);

// Wraps toni's entire extension map as a single opaque entry in http::Extensions.
// Tower middleware cannot read these values (it doesn't know the wrapper type),
// but they are restored intact for downstream toni middleware after the layer runs.
#[derive(Clone)]
struct ToniExtensionBridge(Extensions);

fn to_http_request(req: HttpRequest) -> Result<http::Request<Bytes>, http::Error> {
    let body_bytes: Bytes = req.body;

    let mut builder = http::Request::builder()
        .method(req.method.as_str())
        .uri(req.uri.as_str());

    for (name, value) in &req.headers {
        builder = builder.header(name.as_str(), value.as_str());
    }

    let mut http_req = builder.body(body_bytes)?;
    http_req.extensions_mut().insert(ToniPathParams(req.path_params));
    http_req.extensions_mut().insert(ToniExtensionBridge(req.extensions));

    Ok(http_req)
}

// The Tower layer's response body is wrapped as a streaming BoxBody so toni
// adapters can forward it without buffering. Content-Type from Tower headers
// is propagated onto the Body via with_content_type().
fn to_toni_response<B>(resp: http::Response<B>) -> HttpResponse
where
    B: HttpBody<Data = Bytes> + Send + Sync + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    let (parts, body) = resp.into_parts();

    let headers: Vec<(String, String)> = parts
        .headers
        .iter()
        .filter_map(|(name, value)| {
            value.to_str().ok().map(|v| (name.to_string(), v.to_string()))
        })
        .collect();

    let box_body = body.map_err(Into::into).boxed();
    let toni_body = match parts
        .headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    {
        Some(ct) => Body::from_box_body(box_body).with_content_type(ct),
        None => Body::from_box_body(box_body),
    };

    HttpResponse {
        status: parts.status.as_u16(),
        body: Some(toni_body),
        headers,
    }
}

/// Bridges toni's [`Next`] chain as a `tower::Service<http::Request<Bytes>>`.
///
/// Single-use by design — Tower layers that call the inner service more than
/// once (`tower::retry`, `tower::hedge`) will panic. Those are client-side
/// patterns; server request middleware calls downstream exactly once.
pub struct ToniNextService {
    next: Option<Box<dyn Next>>,
}

impl ToniNextService {
    fn new(next: Box<dyn Next>) -> Self {
        Self { next: Some(next) }
    }
}

impl Service<http::Request<Bytes>> for ToniNextService {
    type Response = http::Response<crate::http_helpers::BoxBody>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<Bytes>) -> Self::Future {
        let next = self
            .next
            .take()
            .expect("ToniNextService called more than once");

        Box::pin(async move {
            let (mut parts, body) = req.into_parts();

            let path_params = parts
                .extensions
                .remove::<ToniPathParams>()
                .map(|p| p.0)
                .unwrap_or_default();

            let query_params: HashMap<String, String> = parts
                .uri
                .query()
                .and_then(|q| serde_urlencoded::from_str(q).ok())
                .unwrap_or_default();

            let headers: Vec<(String, String)> = parts
                .headers
                .iter()
                .filter_map(|(name, value)| {
                    value.to_str().ok().map(|v| (name.to_string(), v.to_string()))
                })
                .collect();

            let extensions = parts
                .extensions
                .remove::<ToniExtensionBridge>()
                .map(|b| b.0)
                .unwrap_or_default();

            let toni_req = HttpRequest {
                method: parts.method.to_string(),
                uri: parts.uri.to_string(),
                headers,
                body,
                query_params,
                path_params,
                extensions,
            };

            let toni_resp = next.run(toni_req).await?;

            let status = http::StatusCode::from_u16(toni_resp.status)
                .unwrap_or(http::StatusCode::INTERNAL_SERVER_ERROR);

            let resp_body = match toni_resp.body {
                Some(body) => body.into_box_body(),
                None => http_body_util::Empty::new()
                    .map_err(|never: std::convert::Infallible| match never {})
                    .boxed(),
            };

            let mut builder = http::Response::builder().status(status);
            for (name, value) in &toni_resp.headers {
                builder = builder.header(name.as_str(), value.as_str());
            }

            builder
                .body(resp_body)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        })
    }
}

/// Wraps any [`tower::Layer`] as a toni [`Middleware`].
///
/// Enables the Tower middleware ecosystem (CORS, tracing, compression, rate
/// limiting, timeouts, etc.) inside `configure_middleware` without any
/// per-adapter work. The conversion between toni's request/response types and
/// `http::Request<Bytes>` / `http::Response<Bytes>` is handled transparently.
///
/// # Known limitations
///
/// - Off-the-shelf Tower layers (e.g. `TraceLayer`) cannot read toni-typed
///   extensions — they don't know the internal bridge type. Custom Tower
///   layers can use [`toni_extensions`] to access the toni extension map
///   explicitly.
/// - Tower layers that call the inner service more than once (`tower::retry`,
///   `tower::hedge`) panic at runtime. Use a toni `Interceptor` for retry
///   logic, or apply retry at the provider level for outbound calls.
///
/// # Example
///
/// ```rust,no_run
/// use toni::tower_compat::TowerLayer;
/// use tower_http::cors::CorsLayer;
/// use tower_http::trace::TraceLayer;
///
/// fn configure_middleware(&self, consumer: &mut MiddlewareConsumer) {
///     consumer
///         .apply(TowerLayer::new(CorsLayer::permissive()))
///         .for_routes(vec!["/*"]);
///     consumer
///         .apply(TowerLayer::new(TraceLayer::new_for_http()))
///         .for_routes(vec!["/*"]);
/// }
/// ```
pub struct TowerLayer<L>(L);

/// Returns a reference to the toni [`Extensions`] stored inside an
/// `http::Request`, if the request passed through a [`TowerLayer`].
///
/// Useful when writing a custom `tower::Layer` that needs to read typed
/// values set by preceding toni middleware:
///
/// ```rust,no_run
/// use toni::tower_compat::toni_extensions;
///
/// // inside your Tower layer's Service::call:
/// if let Some(ext) = toni_extensions(&req) {
///     if let Some(user) = ext.get::<MyUser>() {
///         // use user
///     }
/// }
/// ```
pub fn toni_extensions<B>(req: &http::Request<B>) -> Option<&Extensions> {
    req.extensions().get::<ToniExtensionBridge>().map(|b| &b.0)
}

impl<L> TowerLayer<L> {
    pub fn new(layer: L) -> Self {
        Self(layer)
    }
}

#[async_trait]
impl<L, B> Middleware for TowerLayer<L>
where
    L: Layer<ToniNextService> + Send + Sync,
    L::Service: Service<http::Request<Bytes>, Response = http::Response<B>> + Send,
    B: HttpBody<Data = Bytes> + Send + Sync + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    <L::Service as Service<http::Request<Bytes>>>::Error:
        Into<Box<dyn std::error::Error + Send + Sync>>,
    <L::Service as Service<http::Request<Bytes>>>::Future: Send,
{
    async fn handle(&self, req: HttpRequest, next: Box<dyn Next>) -> MiddlewareResult {
        let mut svc = self.0.layer(ToniNextService::new(next));
        let http_req = to_http_request(req)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        let http_resp = svc
            .ready()
            .await
            .map_err(|e| e.into())?
            .call(http_req)
            .await
            .map_err(|e| e.into())?;
        Ok(to_toni_response(http_resp))
    }
}
