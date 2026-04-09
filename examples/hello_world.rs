//! The minimal toni HTTP application
//!
//! Starting point for anyone new to the framework. Shows the three things
//! every toni app needs: a controller, a module, and an adapter.
//!
//! Run with:  cargo run --example hello_world
//! Test:      curl http://127.0.0.1:3000/hello
//!            curl http://127.0.0.1:3000/hello/json

use serde_json::json;
use toni::*;
use toni_axum::AxumAdapter;

#[derive(Clone)]
pub struct HelloController;

#[controller("/hello")]
impl HelloController {
    pub fn new() -> Self {
        Self
    }

    #[get("/")]
    fn hello(&self) -> Body {
        Body::text("Hello, World!".to_string())
    }

    #[get("/json")]
    fn hello_json(&self) -> Body {
        Body::json(json!({
            "message": "Hello, World!",
            "framework": "toni"
        }))
    }
}

#[module(controllers: [HelloController], providers: [])]
impl AppModule {}

#[tokio::main]
async fn main() {
    println!("🚀 toni hello world\n");
    println!("  GET http://127.0.0.1:3000/hello");
    println!("  GET http://127.0.0.1:3000/hello/json");
    println!();

    let mut app = ToniFactory::new()
        .create_with(AppModule::module_definition())
        .await;

    app.use_http_adapter(AxumAdapter::new(), 3000, "127.0.0.1")
        .unwrap();

    app.start().await;
}
