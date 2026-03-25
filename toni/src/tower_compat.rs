use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use tower::{Layer, Service, ServiceExt};

use crate::async_trait;
use crate::http_helpers::{Body, Extensions, HttpRequest, HttpResponse};
use crate::traits_helpers::middleware::{Middleware, MiddlewareResult, Next};

// Carries path params through the http::Request extension map so they survive
// the Tower round-trip. Tower middleware has no concept of path params.
#[derive(Clone)]
struct ToniPathParams(HashMap<String, String>);

fn to_http_request(req: HttpRequest) -> Result<http::Request<Bytes>, http::Error> {
    let body_bytes: Bytes = match req.body {
        Body::Text(text) => Bytes::from(text.into_bytes()),
        Body::Json(json) => Bytes::from(serde_json::to_vec(&json).unwrap_or_default()),
        Body::Binary(bytes) => Bytes::from(bytes),
    };

    let mut builder = http::Request::builder()
        .method(req.method.as_str())
        .uri(req.uri.as_str());

    for (name, value) in &req.headers {
        builder = builder.header(name.as_str(), value.as_str());
    }

    let mut http_req = builder.body(body_bytes)?;
    http_req.extensions_mut().insert(ToniPathParams(req.path_params));

    Ok(http_req)
}

// The response body comes back as raw Bytes — Tower may have transformed it
// (e.g. CompressionLayer). Always Body::Binary; the adapter resolves Content-Type
// from the headers that Tower set.
fn to_toni_response(resp: http::Response<Bytes>) -> HttpResponse {
    let (parts, body) = resp.into_parts();

    let headers: Vec<(String, String)> = parts
        .headers
        .iter()
        .filter_map(|(name, value)| {
            value.to_str().ok().map(|v| (name.to_string(), v.to_string()))
        })
        .collect();

    HttpResponse {
        status: parts.status.as_u16(),
        body: Some(Body::Binary(body.to_vec())),
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
    type Response = http::Response<Bytes>;
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

            let content_type = headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                .map(|(_, v)| v.as_str())
                .unwrap_or("");

            let toni_body = if content_type.contains("application/json") {
                serde_json::from_slice(&body)
                    .map(Body::Json)
                    .unwrap_or_else(|_| Body::Binary(body.to_vec()))
            } else {
                match String::from_utf8(body.to_vec()) {
                    Ok(text) => Body::Text(text),
                    Err(e) => Body::Binary(e.into_bytes()),
                }
            };

            let toni_req = HttpRequest {
                method: parts.method.to_string(),
                uri: parts.uri.to_string(),
                headers,
                body: toni_body,
                query_params,
                path_params,
                extensions: Extensions::new(),
            };

            let toni_resp = next.run(toni_req).await?;

            let status = http::StatusCode::from_u16(toni_resp.status)
                .unwrap_or(http::StatusCode::INTERNAL_SERVER_ERROR);

            let resp_bytes: Bytes = match toni_resp.body {
                Some(Body::Text(text)) => Bytes::from(text.into_bytes()),
                Some(Body::Json(json)) => {
                    Bytes::from(serde_json::to_vec(&json).unwrap_or_default())
                }
                Some(Body::Binary(bytes)) => Bytes::from(bytes),
                None => Bytes::new(),
            };

            let mut builder = http::Response::builder().status(status);
            for (name, value) in &toni_resp.headers {
                builder = builder.header(name.as_str(), value.as_str());
            }

            builder
                .body(resp_bytes)
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
/// - Typed extensions set by preceding toni middleware are not forwarded
///   through the Tower layer — if auth data lives in `HttpRequest.extensions`,
///   place `TowerLayer` outermost in the chain.
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

impl<L> TowerLayer<L> {
    pub fn new(layer: L) -> Self {
        Self(layer)
    }
}

#[async_trait]
impl<L> Middleware for TowerLayer<L>
where
    L: Layer<ToniNextService> + Send + Sync,
    L::Service: Service<http::Request<Bytes>, Response = http::Response<Bytes>> + Send,
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
