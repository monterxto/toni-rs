mod common;

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use common::TestServer;
use http::header::{HeaderName, HeaderValue};
use serde_json::json;
use serial_test::serial;
use toni::async_trait;
use toni::traits_helpers::middleware::{Middleware, MiddlewareResult, Next};
use toni::traits_helpers::MiddlewareConsumer;
use toni::{HttpRequest, TowerLayer, controller, get, module, post, Body as ToniBody};
use tower::Layer;
use tower::ServiceBuilder;
use reqwest;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::set_header::SetResponseHeaderLayer;

// ── Test 1: basic header injection ───────────────────────────────────────────
//
// Verifies that a Tower layer runs and its response-side effect (a header) is
// visible on the other side of the conversion round-trip.

#[serial]
#[tokio_localset_test::localset_test]
async fn tower_layer_adds_response_header() {
    #[controller("/", pub struct PingController {})]
    impl PingController {
        #[get("/ping")]
        fn ping(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::text("pong")
        }
    }

    #[module(controllers: [PingController])]
    impl TestModule {
        fn configure_middleware(&self, consumer: &mut MiddlewareConsumer) {
            consumer
                .apply(TowerLayer::new(SetResponseHeaderLayer::overriding(
                    HeaderName::from_static("x-tower-test"),
                    HeaderValue::from_static("was-here"),
                )))
                .for_routes(vec!["/*"]);
        }
    }

    let server = TestServer::start(TestModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/ping"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("x-tower-test").unwrap(),
        "was-here",
        "Tower SetResponseHeaderLayer should inject x-tower-test header"
    );
    let body = resp.text().await.unwrap();
    assert_eq!(body, "pong");
}

// ── Test 2: CorsLayer ─────────────────────────────────────────────────────────
//
// Verifies that a real-world tower-http layer (CorsLayer::permissive) works
// and adds the expected CORS headers.

#[serial]
#[tokio_localset_test::localset_test]
async fn tower_layer_cors_permissive() {
    #[controller("/api", pub struct ApiController {})]
    impl ApiController {
        #[get("/data")]
        fn get_data(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::text("ok")
        }
    }

    #[module(controllers: [ApiController])]
    impl CorsModule {
        fn configure_middleware(&self, consumer: &mut MiddlewareConsumer) {
            consumer
                .apply(TowerLayer::new(CorsLayer::permissive()))
                .for_routes(vec!["/*"]);
        }
    }

    let server = TestServer::start(CorsModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/api/data"))
        .header("origin", "https://example.com")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("access-control-allow-origin")
            .unwrap(),
        "*",
        "CorsLayer::permissive should add access-control-allow-origin: *"
    );
}

// ── Test 3: request body survives the round-trip ──────────────────────────────
//
// Verifies that a JSON POST body passes through the HttpRequest → http::Request
// → HttpRequest conversion intact, and the controller receives the original data.
// The Tower layer also adds a header to prove it ran.

#[serial]
#[tokio_localset_test::localset_test]
async fn tower_layer_request_body_round_trip() {
    #[controller("/echo", pub struct EchoController {})]
    impl EchoController {
        #[post("/json")]
        fn echo_json(&self, req: HttpRequest) -> ToniBody {
            ToniBody::from(req.body().clone())
        }
    }

    #[module(controllers: [EchoController])]
    impl EchoModule {
        fn configure_middleware(&self, consumer: &mut MiddlewareConsumer) {
            consumer
                .apply(TowerLayer::new(SetResponseHeaderLayer::overriding(
                    HeaderName::from_static("x-tower-ran"),
                    HeaderValue::from_static("true"),
                )))
                .for_routes(vec!["/*"]);
        }
    }

    let server = TestServer::start(EchoModule::module_definition()).await;
    let payload = json!({"message": "hello tower", "count": 42});

    let resp = server
        .client()
        .post(server.url("/echo/json"))
        .json(&payload)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("x-tower-ran").unwrap(),
        "true",
        "Tower layer should have run"
    );

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body, payload, "JSON body should survive the Tower round-trip");
}

// ── Test 4: extensions visible to Tower ───────────────────────────────────────
//
// A toni middleware sets a typed extension. A custom Tower layer reads it via
// req.extensions() directly — no bridge needed, because HttpRequest IS
// http::Request<Bytes>.

#[derive(Clone)]
struct RequestId(String);

// Custom Tower layer that reads a toni-typed extension and echoes it as a
// response header. Works like any standard Tower layer — extensions set by
// toni middleware are visible in http::Extensions directly.
#[derive(Clone)]
struct EchoExtensionLayer;

#[derive(Clone)]
struct EchoExtensionService<S> {
    inner: S,
}

impl<S> Layer<S> for EchoExtensionLayer {
    type Service = EchoExtensionService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        EchoExtensionService { inner }
    }
}

impl<S, B> tower::Service<http::Request<B>> for EchoExtensionService<S>
where
    S: tower::Service<http::Request<B>, Response = http::Response<toni::http_helpers::BoxBody>>
        + Send,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    B: Send + 'static,
{
    type Response = http::Response<toni::http_helpers::BoxBody>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, req: http::Request<B>) -> Self::Future {
        // Read the RequestId extension set by the preceding toni middleware.
        let request_id = req
            .extensions()
            .get::<RequestId>()
            .map(|id| id.0.clone())
            .unwrap_or_else(|| "not-found".to_string());

        let fut = self.inner.call(req);
        Box::pin(async move {
            let mut resp = fut.await.map_err(Into::into)?;
            resp.headers_mut().insert(
                HeaderName::from_static("x-request-id-echo"),
                HeaderValue::from_str(&request_id).unwrap(),
            );
            Ok(resp)
        })
    }
}

// Toni middleware that stamps a typed RequestId extension before Tower runs.
struct StampRequestIdMiddleware;

#[async_trait]
impl Middleware for StampRequestIdMiddleware {
    async fn handle(&self, mut req: HttpRequest, next: Box<dyn Next>) -> MiddlewareResult {
        req.extensions_mut().insert(RequestId("req-42".to_string()));
        next.run(req).await
    }
}

#[serial]
#[tokio_localset_test::localset_test]
async fn tower_layer_reads_toni_extensions() {
    #[controller("/", pub struct ExtController {})]
    impl ExtController {
        #[get("/ext")]
        fn ext(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::text("ok")
        }
    }

    #[module(controllers: [ExtController])]
    impl ExtModule {
        fn configure_middleware(&self, consumer: &mut MiddlewareConsumer) {
            // Toni middleware runs first and stamps the extension.
            consumer
                .apply(StampRequestIdMiddleware)
                .for_routes(vec!["/*"]);
            // Tower layer runs second and reads the extension via toni_extensions().
            consumer
                .apply(TowerLayer::new(EchoExtensionLayer))
                .for_routes(vec!["/*"]);
        }
    }

    let server = TestServer::start(ExtModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/ext"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("x-request-id-echo").unwrap(),
        "req-42",
        "Tower layer should read the RequestId extension set by preceding toni middleware"
    );
}

// ── Test 5: ServiceBuilder composition ───────────────────────────────────────
//
// Two Tower layers stacked via ServiceBuilder and applied as a single TowerLayer.
// Verifies the documented "idiomatic way to compose multiple Tower middlewares".

#[serial]
#[tokio_localset_test::localset_test]
async fn tower_service_builder_composition() {
    #[controller("/", pub struct ComposedController {})]
    impl ComposedController {
        #[get("/composed")]
        fn composed(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::text("composed")
        }
    }

    #[module(controllers: [ComposedController])]
    impl ComposedModule {
        fn configure_middleware(&self, consumer: &mut MiddlewareConsumer) {
            let stack = ServiceBuilder::new()
                .layer(SetResponseHeaderLayer::overriding(
                    HeaderName::from_static("x-layer-a"),
                    HeaderValue::from_static("a"),
                ))
                .layer(SetResponseHeaderLayer::overriding(
                    HeaderName::from_static("x-layer-b"),
                    HeaderValue::from_static("b"),
                ))
                .into_inner();

            consumer
                .apply(TowerLayer::new(stack))
                .for_routes(vec!["/*"]);
        }
    }

    let server = TestServer::start(ComposedModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/composed"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("x-layer-a").unwrap(), "a");
    assert_eq!(resp.headers().get("x-layer-b").unwrap(), "b");
    assert_eq!(resp.text().await.unwrap(), "composed");
}

// ── Test 6: Tower layer interleaved with toni middleware ──────────────────────
//
// Confirms that toni middleware and Tower layers can be applied in the same
// configure_middleware and that both run in declaration order.

#[serial]
#[tokio_localset_test::localset_test]
async fn tower_and_toni_middleware_interleaved() {
    struct AddToniHeader;

    #[async_trait]
    impl Middleware for AddToniHeader {
        async fn handle(&self, req: HttpRequest, next: Box<dyn Next>) -> MiddlewareResult {
            let mut resp = next.run(req).await?;
            resp.headers.push(("x-toni-mw".to_string(), "ran".to_string()));
            Ok(resp)
        }
    }

    #[controller("/", pub struct InterleavedController {})]
    impl InterleavedController {
        #[get("/interleaved")]
        fn interleaved(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::text("ok")
        }
    }

    #[module(controllers: [InterleavedController])]
    impl InterleavedModule {
        fn configure_middleware(&self, consumer: &mut MiddlewareConsumer) {
            consumer
                .apply(AddToniHeader)
                .for_routes(vec!["/*"]);
            consumer
                .apply(TowerLayer::new(SetResponseHeaderLayer::overriding(
                    HeaderName::from_static("x-tower-mw"),
                    HeaderValue::from_static("ran"),
                )))
                .for_routes(vec!["/*"]);
        }
    }

    let server = TestServer::start(InterleavedModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/interleaved"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("x-toni-mw").unwrap(), "ran");
    assert_eq!(resp.headers().get("x-tower-mw").unwrap(), "ran");
}

// ── Test 7: CompressionLayer body transformation ───────────────────────────────
//
// CompressionLayer rewrites the response body bytes (gzip). This is the
// definitive test that Tower body transformations work end-to-end — the
// previous tests only verified header injection, not body rewriting.
//
// reqwest auto-decompresses gzip when the `gzip` feature is enabled, so the
// asserted body is the original plaintext. The Content-Encoding header confirms
// compression actually fired.

#[serial]
#[tokio_localset_test::localset_test]
async fn tower_compression_layer_transforms_body() {
    // Large enough that gzip will actually compress (small strings may not be
    // worth compressing and some implementations skip them).
    let large_body = "toni ".repeat(500);
    let expected = large_body.clone();

    #[controller("/", pub struct CompressController {})]
    impl CompressController {
        #[get("/data")]
        fn data(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::text("toni ".repeat(500))
        }
    }

    #[module(controllers: [CompressController])]
    impl CompressModule {
        fn configure_middleware(&self, consumer: &mut MiddlewareConsumer) {
            consumer
                .apply(TowerLayer::new(CompressionLayer::new()))
                .for_routes(vec!["/*"]);
        }
    }

    let server = TestServer::start(CompressModule::module_definition()).await;

    // Disable auto-decompression so we can inspect Content-Encoding directly.
    let client = reqwest::Client::builder()
        .no_gzip()
        .build()
        .unwrap();

    let resp = client
        .get(server.url("/data"))
        .header("accept-encoding", "gzip")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get("content-encoding").unwrap(),
        "gzip",
        "CompressionLayer should set Content-Encoding: gzip"
    );
    // With compression disabled on the client side, the raw bytes are gzip.
    // Verify they differ from the plaintext — the body was actually transformed.
    let raw = resp.bytes().await.unwrap();
    assert_ne!(raw.as_ref(), expected.as_bytes(), "body should be gzip-compressed, not plaintext");
}
