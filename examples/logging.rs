//! Logging setup for toni applications.
//!
//! toni emits structured log events via the [`tracing`] crate. The framework
//! never installs a subscriber — that is always the application's responsibility.
//! This example shows the two common setups.
//!
//! # Default (pretty stdout)
//!
//! ```
//! cargo run --example logging
//! ```
//!
//! # Filtered by level or target
//!
//! ```
//! RUST_LOG=info cargo run --example logging
//! RUST_LOG=toni=debug,tower_http=warn cargo run --example logging
//! ```
//!
//! If no subscriber is installed, all framework events are silently discarded —
//! useful in tests or when you bring your own logging backend (e.g. `tracing-appender`,
//! JSON via `tracing-subscriber::fmt().json()`).

use toni::*;
use toni_axum::AxumAdapter;
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Clone)]
struct HelloController;

#[controller("/hello")]
impl HelloController {
    pub fn new() -> Self {
        Self
    }

    #[get("/")]
    fn hello(&self) -> Body {
        Body::text("Hello, toni!".to_string())
    }
}

#[module(controllers: [HelloController], providers: [])]
impl AppModule {}

#[tokio::main]
async fn main() {
    // Install a subscriber before creating the application so bootstrap
    // events (adapter registration, gateway discovery, server start) are captured.
    //
    // EnvFilter reads RUST_LOG; falls back to `info` when the var is absent.
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    println!("cargo run --example logging");
    println!("RUST_LOG=debug cargo run --example logging");
    println!();
    println!("  GET http://127.0.0.1:3000/hello");
    println!();

    let mut app = ToniFactory::new()
        .create_with(AppModule::module_definition())
        .await;

    app.use_http_adapter(AxumAdapter::new(), 3000, "127.0.0.1")
        .unwrap();

    app.start().await;
}
