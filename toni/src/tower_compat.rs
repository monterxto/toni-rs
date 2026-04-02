use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use http_body::Body as HttpBody;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::{BodyExt, Full};
use tower::{Layer, Service, ServiceExt};

use crate::async_trait;
use crate::http_helpers::{
    Body, BoxBody, HttpRequest, HttpResponse, RequestBody, RequestBoxBody, RequestPart,
};
use crate::traits_helpers::middleware::{Middleware, MiddlewareResult, NextHandle, NextInternal};

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

/// Bridges toni's [`NextHandle`] chain as a `tower::Service<http::Request<RequestBoxBody>>`.
///
/// Single-use by design — Tower layers that call the inner service more than
/// once (`tower::retry`, `tower::hedge`) will panic. Those are client-side
/// patterns; server request middleware calls downstream exactly once.
pub struct ToniNextService {
    next: Option<Box<dyn NextInternal>>,
}

impl ToniNextService {
    fn new(next: Box<dyn NextInternal>) -> Self {
        Self { next: Some(next) }
    }
}

impl Service<http::Request<RequestBoxBody>> for ToniNextService {
    type Response = http::Response<BoxBody>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<RequestBoxBody>) -> Self::Future {
        let next = self
            .next
            .take()
            .expect("ToniNextService called more than once");

        Box::pin(async move {
            let (http_parts, body) = req.into_parts();
            let toni_req = HttpRequest::from_parts(http_parts, RequestBody::Streaming(body));
            let toni_resp = next.run_internal(toni_req).await?;

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
/// The Tower service receives the request body as a [`RequestBoxBody`] stream.
/// Buffering only happens if a Tower layer or a downstream extractor actually
/// reads the body — never unconditionally on entry.
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
    L::Service:
        Service<http::Request<RequestBoxBody>, Response = http::Response<B>> + Send + 'static,
    B: HttpBody<Data = Bytes> + Send + Sync + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    <L::Service as Service<http::Request<RequestBoxBody>>>::Error:
        Into<Box<dyn std::error::Error + Send + Sync>>,
    <L::Service as Service<http::Request<RequestBoxBody>>>::Future: Send + 'static,
{
    async fn handle(&self, next: NextHandle) -> MiddlewareResult {
        let (req, inner) = next.into_parts();
        let (parts, body) = req.into_parts();
        let stream_body: RequestBoxBody = match body {
            RequestBody::Buffered(b) => {
                UnsyncBoxBody::new(Full::new(b).map_err(|never: Infallible| match never {}))
            }
            RequestBody::Streaming(s) => s,
        };
        let http_req = http::Request::from_parts(parts, stream_body);
        let mut svc = self.0.layer(ToniNextService::new(inner));
        svc.ready().await.map_err(|e| e.into())?;
        // Must go through dispatch_call rather than calling svc.call(...).await directly.
        // Inlining the call would leave S::Future at a yield point inside this async block,
        // causing the compiler to demand `for<'0> S::Future: Send` (over the hidden dyn Error
        // lifetime in RequestBoxBody) — a bound we cannot satisfy. dispatch_call erases
        // S::Future to Pin<Box<dyn Future + Send + 'static>> in a non-async context where
        // only the explicit S::Future: Send + 'static bound is checked.
        let http_resp = dispatch_call(&mut svc, http_req).await?;
        Ok(to_toni_response(http_resp))
    }
}

fn dispatch_call<S, B>(
    svc: &mut S,
    req: http::Request<RequestBoxBody>,
) -> Pin<
    Box<
        dyn Future<Output = Result<http::Response<B>, Box<dyn std::error::Error + Send + Sync>>>
            + Send
            + 'static,
    >,
>
where
    S: Service<http::Request<RequestBoxBody>, Response = http::Response<B>>,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S::Future: Send + 'static,
    B: 'static,
{
    // No async block here — async block Send-checking universally quantifies
    // over the `dyn Error` lifetime in RequestBoxBody, generating a `for<'0>`
    // HRTB that our `S::Future: Send + 'static` bound cannot satisfy.
    // map_err is a synchronous combinator, so Box::pin checks Send directly
    // against the explicit bound without any HRTB.
    use futures::TryFutureExt;
    Box::pin(svc.call(req).map_err(|e| e.into()))
}
