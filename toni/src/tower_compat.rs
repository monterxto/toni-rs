use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use http_body::Body as HttpBody;
use http_body_util::{BodyExt, Full};
use tower::{Layer, Service, ServiceExt};

use crate::async_trait;
use crate::http_helpers::{Body, BoxBody, HttpRequest, HttpResponse, RequestBody, RequestPart};
use crate::traits_helpers::middleware::{Middleware, MiddlewareResult, Next};

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
            value
                .to_str()
                .ok()
                .map(|v| (name.to_string(), v.to_string()))
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

/// Bridges toni's [`Next`] chain as a `tower::Service<http::Request<Full<Bytes>>>`.
///
/// Single-use by design — Tower layers that call the inner service more than
/// once (`tower::retry`, `tower::hedge`) will panic. Those are client-side
/// patterns; server request middleware calls downstream exactly once.
///
/// Uses `Full<Bytes>` as the body type (infallible error) to avoid the HRTB
/// lifetime issue that arises when `async_trait` sees `Box<dyn Error>` in
/// service bounds. True streaming through Tower layers is deferred to the
/// `feat/async-trait-free` branch.
pub struct ToniNextService {
    next: Option<Box<dyn Next>>,
}

impl ToniNextService {
    fn new(next: Box<dyn Next>) -> Self {
        Self { next: Some(next) }
    }
}

impl Service<http::Request<Full<Bytes>>> for ToniNextService {
    type Response = http::Response<BoxBody>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<Full<Bytes>>) -> Self::Future {
        let next = self
            .next
            .take()
            .expect("ToniNextService called more than once");

        Box::pin(async move {
            let (http_parts, full_body) = req.into_parts();
            let bytes = full_body
                .collect()
                .await
                .map_err(|never: Infallible| match never {})?
                .to_bytes();
            let toni_req = HttpRequest::from_parts(http_parts, RequestBody::Buffered(bytes));
            let toni_resp = next.run(toni_req).await?;

            let status = http::StatusCode::from_u16(toni_resp.status)
                .unwrap_or(http::StatusCode::INTERNAL_SERVER_ERROR);

            let resp_body = match toni_resp.body {
                Some(body) => body.into_box_body(),
                None => Full::new(Bytes::new())
                    .map_err(|never: Infallible| match never {})
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
/// per-adapter work.
///
/// The Tower service receives the request body as a fully-buffered `Full<Bytes>`.
/// Streaming bodies are collected before entering the Tower layer.
// TODO: pass the streaming body through Tower
// rather than collecting it. Requires a conversion layer between
// RequestBoxBody and Tower's Body type.
///
/// # Known limitations
///
/// Tower layers that call the inner service more than once (`tower::retry`,
/// `tower::hedge`) panic at runtime. Use a toni `Interceptor` for retry
/// logic, or apply retry at the provider level for outbound calls.
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
impl<L, B> Middleware for TowerLayer<L>
where
    L: Layer<ToniNextService> + Send + Sync + 'static,
    L::Service: Service<http::Request<Full<Bytes>>, Response = http::Response<B>> + Send + 'static,
    B: HttpBody<Data = Bytes> + Send + Sync + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    <L::Service as Service<http::Request<Full<Bytes>>>>::Error:
        Into<Box<dyn std::error::Error + Send + Sync>>,
    <L::Service as Service<http::Request<Full<Bytes>>>>::Future: Send,
{
    async fn handle(&self, req: HttpRequest, next: Box<dyn Next>) -> MiddlewareResult {
        let (parts, body) = req.into_parts();
        let bytes: Bytes = match body {
            RequestBody::Buffered(b) => b,
            RequestBody::Streaming(s) => s.collect().await?.to_bytes(),
        };
        let http_req = http::Request::from_parts(parts, Full::new(bytes));

        let mut svc = self.0.layer(ToniNextService::new(next));
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
