//! # toni-actix
//!
//! Actix-web adapter for the Toni framework.
//!
//! This crate provides an implementation of Toni's `HttpAdapter` trait for the Actix-web framework,
//! allowing you to use Actix-web as the HTTP server for your Toni applications.
//!
//! ## Usage
//!
//! ```ignore
//! use toni::prelude::*;
//! use toni_actix::ActixAdapter;
//!
//! #[actix_web::main]
//! async fn main() {
//!     let adapter = ActixAdapter::new();
//!
//!     let app = ToniFactory::create(AppModule::module_definition(), adapter);
//!     app.listen(3000, "127.0.0.1").await;
//! }
//! ```

mod actix_adapter;
mod actix_route_adapter;

pub use actix_adapter::ActixAdapter;
pub use actix_route_adapter::ActixRouteAdapter;

// Re-export commonly used types from toni
pub use toni::{HttpAdapter, RouteAdapter};
