mod common;

use common::TestServer;
use http::header::{HeaderName, HeaderValue};
use serde_json::json;
use serial_test::serial;
use toni::traits_helpers::MiddlewareConsumer;
use toni::{TowerLayer, controller, get, module, post, Body as ToniBody, HttpRequest};
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
// through the bridge and adds the expected CORS headers.

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
            ToniBody::from(req.body.clone())
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
