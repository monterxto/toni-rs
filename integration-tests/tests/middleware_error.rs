// Verifies that HttpError returned from Middleware::handle maps to the correct
// HTTP status code rather than collapsing to 500.
//
// Before the fix, Err(e) in the middleware chain was re-boxed as io::Error,
// losing type information, and always produced 500.

mod common;

use common::TestServer;
use serial_test::serial;
use toni::async_trait;
use toni::errors::HttpError;
use toni::traits_helpers::MiddlewareConsumer;
use toni::traits_helpers::middleware::{Middleware, MiddlewareResult, Next};
use toni::{HttpRequest, controller, get, module, Body as ToniBody};

// ── Test 1: custom status code ────────────────────────────────────────────────

struct RejectWith(HttpError);

#[async_trait]
impl Middleware for RejectWith {
    async fn handle(&self, _req: HttpRequest, _next: Box<dyn Next>) -> MiddlewareResult {
        Err(Box::new(self.0.clone()))
    }
}

#[serial]
#[tokio_localset_test::localset_test]
async fn middleware_http_error_preserves_status() {
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
                .apply(RejectWith(HttpError::custom(429, "rate limit exceeded")))
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

    assert_eq!(resp.status(), 429, "HttpError status should be preserved, not collapsed to 500");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["statusCode"], 429);
    assert_eq!(body["message"], "rate limit exceeded");
}

// ── Test 2: named variant (Unauthorized) ─────────────────────────────────────

#[serial]
#[tokio_localset_test::localset_test]
async fn middleware_http_error_unauthorized() {
    #[controller("/", pub struct AuthController {})]
    impl AuthController {
        #[get("/secret")]
        fn secret(&self, _req: HttpRequest) -> ToniBody {
            ToniBody::text("secret")
        }
    }

    #[module(controllers: [AuthController])]
    impl AuthModule {
        fn configure_middleware(&self, consumer: &mut MiddlewareConsumer) {
            consumer
                .apply(RejectWith(HttpError::unauthorized("token expired")))
                .for_routes(vec!["/*"]);
        }
    }

    let server = TestServer::start(AuthModule::module_definition()).await;
    let resp = server
        .client()
        .get(server.url("/secret"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 401);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["statusCode"], 401);
    assert_eq!(body["message"], "token expired");
}
