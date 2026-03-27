//! Bearer token authentication middleware
//!
//! Shows how to implement the Middleware trait and apply it selectively to
//! routes. Public routes pass through untouched; protected routes require
//! a valid Authorization header.
//!
//! Run with:  cargo run --example middleware_examples
//!
//! Test public route (no token needed):
//!   curl http://127.0.0.1:3000/api/public
//!
//! Test protected route — missing token (401):
//!   curl http://127.0.0.1:3000/api/profile
//!
//! Test protected route — valid token (200):
//!   curl -H "Authorization: Bearer secret" http://127.0.0.1:3000/api/profile

use serde_json::json;
use toni::{
    async_trait,
    http_helpers::{Body, HttpRequest, HttpResponse},
    middleware::{Middleware, MiddlewareResult, Next},
    traits_helpers::MiddlewareConsumer,
    *,
};
use toni_axum::AxumAdapter;

struct AuthMiddleware {
    valid_token: String,
}

impl AuthMiddleware {
    fn new(token: impl Into<String>) -> Self {
        Self {
            valid_token: token.into(),
        }
    }
}

#[async_trait]
impl Middleware for AuthMiddleware {
    async fn handle(&self, req: HttpRequest, next: Box<dyn Next>) -> MiddlewareResult {
        let token = req
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));

        if token == Some(self.valid_token.as_str()) {
            return next.run(req).await;
        }

        let mut response = HttpResponse::new();
        response.status = 401;
        response.body = Some(Body::json(json!({
            "statusCode": 401,
            "error": "Unauthorized",
            "message": "Missing or invalid Bearer token"
        })));
        Ok(response)
    }
}

#[controller("/api", pub struct ApiController;)]
impl ApiController {
    #[get("/public")]
    fn public(&self) -> Body {
        Body::text("Public — no token required".to_string())
    }

    #[get("/profile")]
    fn profile(&self) -> Body {
        Body::text("Authenticated — token accepted".to_string())
    }
}

#[module(controllers: [ApiController], providers: [])]
impl AppModule {
    fn configure_middleware(&self, consumer: &mut MiddlewareConsumer) {
        consumer
            .apply(AuthMiddleware::new("secret"))
            .for_routes(vec!["/api/profile"]);
    }
}

#[tokio::main]
async fn main() {
    println!("🔐 toni auth middleware\n");
    println!("  GET http://127.0.0.1:3000/api/public   (no token needed)");
    println!("  GET http://127.0.0.1:3000/api/profile  (requires: Authorization: Bearer secret)");
    println!();

    let mut app = ToniFactory::new()
        .create_with(AppModule::module_definition())
        .await;

    app.use_http_adapter(AxumAdapter::new("127.0.0.1", 3000))
        .unwrap();

    app.start().await;
}
